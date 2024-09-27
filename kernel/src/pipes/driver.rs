use core::sync::atomic::{AtomicUsize, Ordering, AtomicU32};

use alloc::boxed::Box;
use alloc::vec::Vec;
use idos_api::io::error::IOError;
use spin::{Once, RwLock};

use crate::collections::SlotList;
use crate::io::filesystem::driver::DriverID;
use crate::io::driver::kernel_driver::KernelDriver;
use crate::files::path::Path;
use crate::io::filesystem::driver::AsyncIOCallback;
use crate::io::driver::comms::IOResult;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::virt::scratch::UnmappedPage;
use crate::task::paging::get_current_physical_address;
use crate::task::switching::get_task;

pub static PIPES: RwLock<SlotList<Pipe>> = RwLock::new(SlotList::new());
pub static OPEN_PIPES: RwLock<SlotList<PipeEnd>> = RwLock::new(SlotList::new());
const BUFFER_SIZE: usize = 512;

// TODO: Move this into its own mod when we've cleaned up legacy pipes

/// Pipe is not a general-purpose read-write buffer. It's designed to the use
/// case of async handles.
/// Reading: If data is available in the buffer, the read will immediately
/// finish and the Op will be completed. If there is not enough data yet, the
/// Op callback data will be stored for a future write to complete the Op.
/// Writing: If there is no active read, data is stored in a buffer to be read
/// later. If the buffer fills up, the write will "succeed," but will reflect
/// how many bytes were actually written. If there is an active read that is
/// blocked on data, the write will directly put bytes in the read buffer until
/// it's full. Any remainder will be stored in the Pipe buffer. If the read
/// completes, the write method `take`s the callback data from the stored
/// `Option` and uses it to complete the original read Op.
///
/// TODO: do you want this to be super efficient and lock-free like the
/// ringbuffer? It could be, but it's tricky to synchronize 3 atomic pointers,
/// and I'd rather unblock it and move onto other things.
pub struct Pipe {
    read_callback: RwLock<Option<AsyncIOCallback>>,
    read_ptr: AtomicU32,
    read_len: AtomicUsize,
    read_progress: AtomicUsize,

    pipe_buffer: RwLock<Box<[u8]>>,
    write_head: AtomicUsize,
    write_tail: AtomicUsize,

    open_readers: AtomicUsize,
    open_writers: AtomicUsize,
}

pub enum ReadMode {
    NonBlocking,
    Blocking(AsyncIOCallback),
}

impl Pipe {
    pub fn new() -> Self {
        let mut buffer: Vec<u8> = Vec::with_capacity(BUFFER_SIZE);
        for _ in 0..BUFFER_SIZE {
            buffer.push(0);
        }
        Self {
            read_callback: RwLock::new(None),
            read_ptr: AtomicU32::new(0),
            read_len: AtomicUsize::new(0),
            read_progress: AtomicUsize::new(0),

            pipe_buffer: RwLock::new(buffer.into_boxed_slice()),
            write_head: AtomicUsize::new(0),
            write_tail: AtomicUsize::new(0),

            open_readers: AtomicUsize::new(1),
            open_writers: AtomicUsize::new(1),
        }
    }

    /// Returns the number of bytes that can be read immediately without blocking
    pub fn available_len(&self) -> usize {
        let write_head = self.write_head.load(Ordering::SeqCst);
        let write_tail = self.write_tail.load(Ordering::SeqCst);
        write_head - write_tail
    }

    /// Attempts to fill a buffer with a series of bytes. If it cannot be
    /// immediately filled, the callback info is stored and the function
    /// returns None. When a write provides enough data to fill the buffer, it
    /// will use the callback info to complete the async io operation.
    /// If it can be immediately filled, it returns Some number of bytes read,
    /// so that the caller can immediately resolve the file operation
    /// associated with this read.
    pub fn read(&self, buffer: &mut [u8], read_mode: ReadMode) -> Option<usize> {
        let write_head = self.write_head.load(Ordering::SeqCst);
        let mut write_tail = self.write_tail.load(Ordering::SeqCst);
        let available_data = write_head - write_tail;
        let mut written = 0;
        let pipe_buffer = self.pipe_buffer.read();
        loop {
            // if the buffer has been filled, complete the request immediately
            if written == buffer.len() {
                self.write_tail.store(write_tail, Ordering::SeqCst);
                return Some(written);
            }
            // if the buffer has been emptied, block until a future write
            if written >= available_data {
                break;
            }
            let buffer_index = write_tail % BUFFER_SIZE;
            buffer[written] = pipe_buffer[buffer_index];
            write_tail += 1;
            written += 1;
        }
        self.write_tail.store(write_tail, Ordering::SeqCst);

        if let ReadMode::Blocking(callback) = read_mode {
            self.read_len.store(buffer.len(), Ordering::SeqCst);
            self.read_progress.store(written, Ordering::SeqCst);
            let read_ptr = get_current_physical_address(VirtualAddress::new(buffer.as_ptr() as u32)).unwrap().as_u32();
            self.read_ptr.store(read_ptr, Ordering::SeqCst);
            
            if let Some(_) = self.read_callback.write().replace(callback) {
                panic!("Pipe: mutiple parallel reads should not be possible");
            }

            None
        } else {
            Some(written)
        }
    }

    /// Write a buffer of bytes to the pipe, to be read out the other end.
    /// If a read is alreading pending, those bytes will be written directly to
    /// the read buffer. If enough bytes are provided, the read will be
    /// completed.
    /// After any pending read, remaining bytes are written to the Pipe's
    /// internal buffer. These will be available to future read calls.
    /// If there is not enough room in the Pipe's internal buffer to store the
    /// write buffer, it will cleanly exit, having filled as much space as it
    /// could. The callsite can attempt further writes to get the rest of the
    /// data through the pipe.
    /// The function returns a tuple with 2 elements:
    ///   - The first element is the number of bytes that were successfully
    ///   written. If it's less than the buffer len, that means the pipe was
    ///   filled and another write should be attempted later.
    ///   - The second element is callback info for any read that was completed
    ///   as a result of this write. If a read did finish, it will contain the
    ///   info necessary for the Pipe driver to complete the original read op
    pub fn write(&self, buffer: &[u8]) -> (usize, Option<(usize, AsyncIOCallback)>) {
        let mut write_head = self.write_head.load(Ordering::SeqCst);
        let write_tail = self.write_tail.load(Ordering::SeqCst);
        let mut read_progress = self.read_progress.load(Ordering::SeqCst);
        let read_len = self.read_len.load(Ordering::SeqCst);
        let mut pipe_buffer = self.pipe_buffer.write();
        let mut written = 0;

        let callback = if read_len > 0 {
            // write directly to the read buffer, rather than the pipe
            let read_ptr = self.read_ptr.load(Ordering::SeqCst);
            
            // TODO: implement some kind of shared buffer reference that
            // contains between 1 and 2 physical frames
            let buffer_page = UnmappedPage::map(PhysicalAddress::new(read_ptr & 0xfffff000));
            if (read_ptr & 0xfff) + (read_len as u32) > 0x1000 {
                panic!("Read buffer is too large to fit in one scratch page");
            }

            let read_buffer = unsafe {
                let ptr = buffer_page.virtual_address().as_ptr_mut::<u8>().add((read_ptr as usize) & 0xfff);
                core::slice::from_raw_parts_mut(ptr, read_len)
            };

            loop {
                if read_progress >= read_len {
                    break;
                }
                if written >= buffer.len() {
                    break;
                }
                read_buffer[read_progress] = buffer[written];

                read_progress += 1;
                written += 1;
            }
            self.read_progress.store(read_progress, Ordering::SeqCst);
            if read_progress >= read_len {
                self.read_callback.write().take().map(|cb| (read_len, cb))
            } else {
                None
            }
        } else {
            None
        };

        loop {
            let data_available = write_head - write_tail;
            // buffer is full
            if data_available % BUFFER_SIZE >= BUFFER_SIZE - 1 {
                break;
            }
            if written == buffer.len() {
                break;
            }
            let buffer_index = write_head % BUFFER_SIZE;
            pipe_buffer[buffer_index] = buffer[written];
            write_head += 1;
            written += 1;
            read_progress += 1;
        }
        self.write_head.store(write_head, Ordering::SeqCst);

        return (written, callback);
    }

    pub fn open_reader(&self) {
        let prev = self.open_readers.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            panic!("Cannot reopen a closed pipe");
        }
    }

    pub fn close_reader(&self) {
        let prev = self.open_readers.fetch_sub(1, Ordering::SeqCst);
        if prev == 0 {
            panic!("Pipe already closed");
        }
    }

    pub fn close_writer(&self) {
        let prev = self.open_writers.fetch_sub(1, Ordering::SeqCst);
        if prev == 0 {
            panic!("Pipe already closed");
        }
    }

    pub fn is_write_open(&self) -> bool {
        let open = self.open_writers.load(Ordering::SeqCst);
        open > 0
    }

    pub fn is_read_open(&self) -> bool {
        let open = self.open_readers.load(Ordering::SeqCst);
        open > 0
    }
}

pub enum PipeEnd {
    Reader(usize),
    Writer(usize),
}

pub fn create_pipe() -> (u32, u32) {
    let pipe_index = PIPES.write().insert(Pipe::new());
    let mut open_pipes = OPEN_PIPES.write();
    let reader = PipeEnd::Reader(pipe_index);
    let writer = PipeEnd::Writer(pipe_index);
    (
        open_pipes.insert(reader) as u32,
        open_pipes.insert(writer) as u32,
    )
}

struct PipeDriver {
}

impl PipeDriver {
    fn begin_read(pipe_index: usize, buffer: &mut [u8], io_callback: AsyncIOCallback) -> Option<IOResult> {
        let pipes = PIPES.read();
        let pipe = match pipes.get(pipe_index) {
            Some(pipe) => pipe,
            None => return Some(Err(IOError::NotFound)),
        };
        if !pipe.is_write_open() {
            // if the write end is not open, flush any remaining contents
            // and then allow the read to complete without blocking
            return Some(Ok(pipe.read(buffer, ReadMode::NonBlocking).unwrap() as u32));
        }
        pipe.read(buffer, ReadMode::Blocking(io_callback)).map(|bytes_read| Ok(bytes_read as u32))
    }

    fn write(pipe_index: usize, buffer: &[u8]) -> Option<IOResult> {
        let pipes = PIPES.read();
        let pipe = match pipes.get(pipe_index) {
            Some(pipe) => pipe,
            None => return Some(Err(IOError::NotFound)),
        };
        if !pipe.is_read_open() {
            return Some(Err(IOError::WriteToClosedIO));
        }
        let (written, callback_opt) = pipe.write(buffer);
        if let Some((read, callback)) = callback_opt {
            let (task_id, io_index, op_id) = callback;
            let task_lock = match get_task(task_id) {
                Some(lock) => lock,
                None => return Some(Ok(written as u32)),
            };
            task_lock.write().async_io_complete(io_index, op_id, Ok(read as u32));
        }
        Some(Ok(written as u32))
    }

    fn close_reader(pipe_index: usize) {
        let mut pipes = PIPES.write();
        let pipe = match pipes.get(pipe_index) {
            Some(pipe) => pipe,
            None => return,
        };
        pipe.close_reader();
        if pipe.is_read_open() || pipe.is_write_open() {
            return;
        }
        // if the pipe has no more readers or writers, delete the pipe
        pipes.remove(pipe_index);
    }

    fn close_writer(pipe_index: usize) {
        let mut pipes = PIPES.write();
        let pipe = match pipes.get(pipe_index) {
            Some(pipe) => pipe,
            None => return,
        };
        pipe.close_writer();
        if pipe.is_read_open() || pipe.is_write_open() {
            return;
        }
        pipes.remove(pipe_index);
    }

    pub fn get_open_pipes() -> Vec<usize> {
        let pipes = PIPES.read();
        pipes.enumerate().map(|(index, _)| index).collect()
    }
}

impl KernelDriver for PipeDriver {
    fn open(&self, _path: Option<Path>, _io_callback: AsyncIOCallback) -> Option<IOResult> {
        Some(Err(IOError::UnsupportedOperation))
    }

    fn read(&self, instance: u32, buffer: &mut [u8], _offset: u32, io_callback: AsyncIOCallback) -> Option<IOResult> {
        let pipe_index: usize = {
            match OPEN_PIPES.read().get(instance as usize) {
                Some(PipeEnd::Reader(index)) => *index,
                None => return Some(Err(IOError::FileHandleInvalid)),
                _ => return Some(Err(IOError::FileHandleWrongType)),
            }
        };
        Self::begin_read(pipe_index, buffer, io_callback)
    }

    fn write(&self, instance: u32, buffer: &[u8], _offset: u32, io_callback: AsyncIOCallback) -> Option<IOResult> {
        let pipe_index: usize = {
            match OPEN_PIPES.read().get(instance as usize) {
                Some(PipeEnd::Writer(index)) => *index,
                None => return Some(Err(IOError::FileHandleInvalid)),
                _ => return Some(Err(IOError::FileHandleWrongType)),
            }
        };
        Self::write(pipe_index, buffer)
    }

    fn close(&self, instance: u32, io_callback: AsyncIOCallback) -> Option<IOResult> {
        match OPEN_PIPES.read().get(instance as usize) {
            Some(PipeEnd::Reader(index)) => {
                Self::close_reader(*index);
            },
            Some(PipeEnd::Writer(index)) => {
                Self::close_writer(*index);
            },
            None => return Some(Err(IOError::FileHandleInvalid)),
        }

        OPEN_PIPES.write().remove(instance as usize);
        Some(Ok(0))
    }
}

pub static PIPE_DRIVER_ID: Once<DriverID> = Once::new();

pub fn install() {
    PIPE_DRIVER_ID.call_once(|| {
        crate::io::filesystem::install_kernel_fs("", Box::new(PipeDriver {}))
    });
}

pub fn get_pipe_drive_id() -> DriverID {
    *PIPE_DRIVER_ID.get().expect("PIPE FS not initialized")
}

#[cfg(test)]
mod tests {
    use super::{Pipe, ReadMode};
    use crate::task::id::TaskID;
    use crate::io::async_io::{AsyncOpID, ASYNC_OP_READ};
    use crate::io::handle::{Handle, PendingHandleOp};
    use crate::task::actions::handle::{create_kernel_task, create_pipe_handles, handle_op_read, handle_op_write, handle_op_close, transfer_handle};
    use crate::task::actions::lifecycle::terminate;
    use crate::task::actions::yield_coop;
    use idos_api::io::error::IOError;

    // pipe tests

    #[test_case]
    fn pipe_basic_rw() {
        let pipe = Pipe::new();
        let mut read_buffer: [u8; 3] = [0; 3];
        let write_buffer: [u8; 4] = [0xaa, 0xbb, 0xcc, 0xdd];
        pipe.write(&write_buffer);
        let mut read_result = pipe.read(&mut read_buffer, ReadMode::Blocking((TaskID::new(0), 0, AsyncOpID::new(0))));
        assert_eq!(read_result, Some(3));
        assert_eq!(read_buffer, [0xaa, 0xbb, 0xcc]);

        pipe.write(&write_buffer);
        read_result = pipe.read(&mut read_buffer, ReadMode::Blocking((TaskID::new(0), 0, AsyncOpID::new(0))));
        assert_eq!(read_result, Some(3));
        assert_eq!(read_buffer, [0xdd, 0xaa, 0xbb]);
    }

    #[test_case]
    fn blocking_read() {
        let pipe = Pipe::new();
        let mut read_buffer: [u8; 3] = [0; 3];
        let write_buffer: [u8; 4] = [0xaa, 0xbb, 0xcc, 0xdd];

        let callback = (TaskID::new(0), 1, AsyncOpID::new(2));
        let mut read_result = pipe.read(&mut read_buffer, ReadMode::Blocking(callback));
        assert_eq!(read_result, None);

        let write_result = pipe.write(&write_buffer);
        assert_eq!(write_result, (4, Some((3, callback))));
        assert_eq!(pipe.available_len(), 1);

        assert_eq!(read_buffer, [0xaa, 0xbb, 0xcc]);
    }

    #[test_case]
    fn write_exactly_fills_read() {
        let pipe = Pipe::new();
        let mut read_buffer: [u8; 3] = [0; 3];
        let write_buffer: [u8; 3] = [0xaa, 0xbb, 0xcc];

        let callback = (TaskID::new(0), 1, AsyncOpID::new(2));
        let mut read_result = pipe.read(&mut read_buffer, ReadMode::Blocking(callback));
        assert_eq!(read_result, None);

        let write_result = pipe.write(&write_buffer);
        assert_eq!(write_result, (3, Some((3, callback))));
        assert_eq!(pipe.available_len(), 0);
        assert_eq!(read_buffer, [0xaa, 0xbb, 0xcc]);
    }

    #[test_case]
    fn write_does_not_fill_read() {
        let pipe = Pipe::new();
        let mut read_buffer: [u8; 3] = [0; 3];
        let write_buffer: [u8; 2] = [0xaa, 0xbb];

        let callback = (TaskID::new(0), 1, AsyncOpID::new(2));
        let mut read_result = pipe.read(&mut read_buffer, ReadMode::Blocking(callback));
        assert_eq!(read_result, None);

        let mut write_result = pipe.write(&write_buffer);
        assert_eq!(write_result, (2, None));
        assert_eq!(pipe.available_len(), 0);
        write_result = pipe.write(&write_buffer);
        assert_eq!(write_result, (2, Some((3, callback))));
        assert_eq!(pipe.available_len(), 1);
        assert_eq!(read_buffer, [0xaa, 0xbb, 0xaa]);
    }

    // pipe fs tests

    #[test_case]
    fn same_task_write_then_read() {
        let (reader, writer) = create_pipe_handles();
        let write_op = handle_op_write(writer, &[1, 3, 5]);
        assert_eq!(write_op.wait_for_completion(), 3);
        let mut read_buffer: [u8; 3] = [0; 3];
        let read_op = handle_op_read(reader, &mut read_buffer, 0);
        assert_eq!(read_op.wait_for_completion(), 3);
        assert_eq!(read_buffer, [1, 3, 5]);
    }

    #[test_case]
    fn same_task_read_then_write() {
        // want to make sure this doesn't cause a deadlock somewhere
        let (reader, writer) = create_pipe_handles();
        let mut read_buffer: [u8; 3] = [0; 3];
        let read_op = handle_op_read(reader, &mut read_buffer, 0);
        
        let write_op = handle_op_write(writer, &[2, 4, 6]);
        assert_eq!(write_op.wait_for_completion(), 3);
        assert_eq!(read_op.wait_for_completion(), 3);
        assert_eq!(read_buffer, [2, 4, 6]);
    }

    #[test_case]
    fn write_then_read() {
        let (reader, writer) = create_pipe_handles();
        let write_op = handle_op_write(writer, &[12, 8]);
        assert_eq!(write_op.wait_for_completion(), 2);

        fn child_task_body() -> ! {
            let reader = Handle::new(0);
            let mut read_buffer: [u8; 2] = [0; 2];
            let read_op = handle_op_read(reader, &mut read_buffer, 0);
            assert_eq!(read_op.wait_for_completion(), 2);
            assert_eq!(read_buffer, [12, 8]);
            terminate(1);
        }

        let (child_handle, child_id) = create_kernel_task(child_task_body, Some("CHILD"));
        transfer_handle(reader, child_id);

        let op = PendingHandleOp::new(child_handle, ASYNC_OP_READ, 0, 0, 0);
        op.wait_for_completion();
    }

    #[test_case]
    fn read_then_write() {
        let (reader, writer) = create_pipe_handles();
        fn child_task_body() -> ! {
            let reader = Handle::new(0);
            let mut read_buffer: [u8; 2] = [0; 2];
            let read_op = handle_op_read(reader, &mut read_buffer, 0);
            assert_eq!(read_op.wait_for_completion(), 2);
            assert_eq!(read_buffer, [22, 80]);
            terminate(1);
        }

        let (child_handle, child_id) = create_kernel_task(child_task_body, Some("CHILD"));
        transfer_handle(reader, child_id);
        yield_coop();
        let write_op = handle_op_write(writer, &[22, 80]);
        assert_eq!(write_op.wait_for_completion(), 2);

        let op = PendingHandleOp::new(child_handle, ASYNC_OP_READ, 0, 0, 0);
        op.wait_for_completion();
    }

    #[test_case]
    fn read_when_write_closed() {
        let (reader, writer) = create_pipe_handles();
        handle_op_write(writer, &[1, 2, 3, 4]).wait_for_completion();
        handle_op_close(writer).wait_for_completion();
        let mut read_buffer: [u8; 4] = [0; 4];
        let mut read_op = handle_op_read(reader, &mut read_buffer, 0);
        assert_eq!(read_op.wait_for_completion(), 4);
        assert_eq!(read_buffer, [1, 2, 3, 4]);
        read_op = handle_op_read(reader, &mut read_buffer, 0);
        // does not block, immediately returns zero length / EOF
        assert_eq!(read_op.wait_for_completion(), 0);
    }

    #[test_case]
    fn write_when_read_closed() {
        let (reader, writer) = create_pipe_handles();
        handle_op_close(reader).wait_for_completion();
        let write_buffer: [u8; 3] = [12, 14, 18];
        let write_op = handle_op_write(writer, &[12, 14, 18]);
        assert_eq!(write_op.wait_for_completion(), 0x80000000 | IOError::WriteToClosedIO as u32);
    }
}
