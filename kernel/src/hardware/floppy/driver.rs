use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::task::{Context, Poll, Waker};
use core::{future::Future, pin::Pin};

use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::{boxed::Box, task::Wake};
use idos_api::io::error::{IoError, IoResult};
use idos_api::io::{AsyncOp, ASYNC_OP_READ};

use crate::hardware::dma::DmaChannelRegisters;
use crate::io::filesystem::install_task_dev;
use crate::io::handle::Handle;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::task::actions::handle::{open_interrupt_handle, open_message_queue};
use crate::task::actions::io::{close_sync, driver_io_complete, send_io_op, write_sync};
use crate::task::actions::memory::map_memory;
use crate::task::actions::sync::{block_on_wake_set, create_wake_set};
use crate::task::actions::yield_coop;
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;
use crate::task::paging::page_on_demand;
use crate::task::switching::get_current_id;
use idos_api::io::driver::DriverCommand;
use idos_api::ipc::Message;

use super::controller::{Command, ControllerError, DriveSelect, DriveType, FloppyController};
use super::geometry::ChsGeometry;

pub struct FloppyDeviceDriver {
    controller: FloppyController,
    dma_vaddr: VirtualAddress,
    dma_paddr: PhysicalAddress,
    interrupt_received: Arc<AtomicBool>,
    attached: [DriveType; 2],
    selected_drive: Option<DriveSelect>,

    next_instance: AtomicU32,
    open_instances: BTreeMap<u32, OpenFile>,
}

impl FloppyDeviceDriver {
    pub fn new(interrupt_flag: Arc<AtomicBool>) -> Self {
        let dma_vaddr = map_memory(None, 0x1000, MemoryBacking::IsaDma).unwrap();
        let dma_paddr = page_on_demand(dma_vaddr).unwrap();

        Self {
            controller: FloppyController::new(),
            dma_vaddr,
            dma_paddr,
            interrupt_received: interrupt_flag,
            attached: [DriveType::None, DriveType::None],
            selected_drive: None,

            next_instance: AtomicU32::new(1),
            open_instances: BTreeMap::new(),
        }
    }

    pub fn set_device(&mut self, index: usize, drive_type: DriveType) {
        self.attached[index] = drive_type;
    }

    pub async fn init(&mut self) -> Result<(), ControllerError> {
        let mut response = [0];

        self.send_command(Command::Version, &[]).await?;
        self.controller.get_response(&mut response)?;
        if response[0] != 0x90 {
            return Err(ControllerError::UnsupportedController);
        }
        // 0x57 = 0b01010111
        //           | enable implied seek
        //            | enable fifo
        //             | disable polling
        //              | threshold is 8 bytes
        self.send_command(Command::Configure, &[0, 0x57, 0]).await?;
        self.send_command(Command::Lock, &[]).await?;
        self.controller.get_response(&mut response)?;
        if response[0] != 0x10 {
            return Err(ControllerError::InvalidResponse);
        }

        self.reset().await?;

        // enable motors, recalibrate
        match self.attached[0] {
            DriveType::None => (),
            _ => {
                self.controller.ensure_motor_on(DriveSelect::Primary);
                self.recalibrate(DriveSelect::Primary).await?;
            }
        }
        match self.attached[1] {
            DriveType::None => (),
            _ => {
                self.controller.ensure_motor_on(DriveSelect::Secondary);
                self.recalibrate(DriveSelect::Secondary).await?;
            }
        }
        Ok(())
    }

    fn clear_interrupt(&self) {
        self.interrupt_received.store(false, Ordering::SeqCst);
    }

    fn wait_for_interrupt(&self) -> InterruptFuture {
        InterruptFuture::new(self.interrupt_received.clone())
    }

    async fn reset(&self) -> Result<(), ControllerError> {
        self.clear_interrupt();
        self.controller.dor_write(0);
        // stall a bit
        yield_coop();
        // Motors off, reset + IRQ enabled, select disk 0
        self.controller.dor_write(0x0c);
        self.wait_for_interrupt().await;

        let mut sense = [0, 0];
        for _ in 0..4 {
            self.controller.send_command(Command::SenseInterrupt, &[])?;
            self.controller.get_response(&mut sense)?;
        }

        // TODO: Set the data rate correctly for the drive type
        self.controller.ccr_write(0);
        // SRT=8, HUT=0, HLT=5, NDMA=0
        self.controller
            .send_command(Command::Specify, &[8 << 4, 5 << 1])?;

        Ok(())
    }

    async fn select_drive(&mut self, drive: DriveSelect) {
        if self.selected_drive == Some(drive) {
            return;
        }
        let dor = self.controller.dor_read();
        let flag = match drive {
            DriveSelect::Primary => 0,
            DriveSelect::Secondary => 1,
        };
        self.controller.dor_write((dor & 0xfc) | flag);
        self.selected_drive = Some(drive);
    }

    async fn recalibrate(&mut self, drive: DriveSelect) -> Result<(), ControllerError> {
        self.select_drive(drive).await;
        let mut st0 = [0, 0];
        for _retry in 0..2 {
            self.controller.send_command(Command::Recalibrate, &[0])?;
            self.wait_for_interrupt().await;
            self.controller.send_command(Command::SenseInterrupt, &[])?;
            self.controller.get_response(&mut st0)?;

            if st0[0] & 0x20 == 0x20 {
                break;
            }
        }
        Ok(())
    }

    async fn send_command(&self, command: Command, params: &[u8]) -> Result<(), ControllerError> {
        if self.controller.get_status() & 0xc0 != 0x80 {
            self.reset().await?;
        }

        self.clear_interrupt();
        self.controller.send_command(command, params)
    }

    async fn dma(
        &self,
        command: Command,
        drive_number: u8,
        chs: ChsGeometry,
    ) -> Result<(), ControllerError> {
        self.send_command(
            command,
            &[
                (chs.head << 2) as u8 | drive_number,
                chs.cylinder as u8,
                chs.head as u8,
                chs.sector as u8,
                2,
                18,   // Last sector on track
                0x1b, // GAP1 default size
                0xff,
            ],
        )
        .await?;

        self.wait_for_interrupt().await;
        let mut response = [0, 0, 0, 0, 0, 0, 0];
        self.controller.get_response(&mut response)?;
        // TODO: process response

        Ok(())
    }

    async fn dma_read(
        &mut self,
        drive: DriveSelect,
        chs: ChsGeometry,
    ) -> Result<(), ControllerError> {
        self.select_drive(drive).await;
        let drive_number = match drive {
            DriveSelect::Primary => 0,
            DriveSelect::Secondary => 1,
        };
        self.dma(Command::ReadData, drive_number, chs).await
    }

    async fn dma_write(
        &mut self,
        drive: DriveSelect,
        chs: ChsGeometry,
    ) -> Result<(), ControllerError> {
        self.select_drive(drive).await;
        let drive_number = match drive {
            DriveSelect::Primary => 0,
            DriveSelect::Secondary => 1,
        };
        self.dma(Command::WriteData, drive_number, chs).await
    }

    fn get_dma_buffer(&self) -> &mut [u8] {
        unsafe {
            let buffer_ptr = self.dma_vaddr.as_ptr_mut::<u8>();
            let buffer_len = 0x1000;
            core::slice::from_raw_parts_mut(buffer_ptr, buffer_len)
        }
    }

    fn dma_prepare(&self, sector_count: usize, dma_mode: u8) {
        let dma_channel = DmaChannelRegisters::for_channel(2);
        dma_channel.set_address(self.dma_paddr);
        dma_channel.set_count((sector_count * super::geometry::SECTOR_SIZE) as u32 - 1);
        dma_channel.set_mode(dma_mode);
    }

    // Async IO methods:

    pub fn open(&mut self, sub_driver: u32) -> IoResult {
        match self.attached.get(sub_driver as usize) {
            None => return Err(IoError::NotFound),
            _ => (),
        }
        let drive = match sub_driver {
            1 => DriveSelect::Secondary,
            _ => DriveSelect::Primary,
        };
        let file = OpenFile { drive };
        let instance = self.next_instance.fetch_add(1, Ordering::SeqCst);
        self.open_instances.insert(instance, file);
        Ok(instance)
    }

    pub async fn read(&mut self, instance: u32, buffer: &mut [u8], offset: u32) -> IoResult {
        if buffer.is_empty() {
            return Ok(0);
        }

        let drive_select = match self.open_instances.get(&instance) {
            Some(file) => file.drive,
            None => return Err(IoError::FileHandleInvalid),
        };

        let mut buf_offset = 0usize;
        let mut position = offset as usize;
        let total = buffer.len();

        while buf_offset < total {
            let first_sector = position / super::geometry::SECTOR_SIZE;
            let read_offset = position % super::geometry::SECTOR_SIZE;
            let remaining = total - buf_offset;
            let last_sector = (position + remaining - 1) / super::geometry::SECTOR_SIZE;
            let mut sector_count = last_sector - first_sector + 1;

            // Clamp to DMA buffer size
            let dma_buf_size = 0x1000;
            let max_sectors = dma_buf_size / super::geometry::SECTOR_SIZE;
            if sector_count > max_sectors {
                sector_count = max_sectors;
            }

            // Clamp to track boundary: don't read past the last sector on the
            // current track, since the floppy controller can't cross tracks in
            // a single DMA transfer.
            let chs = ChsGeometry::from_lba(first_sector);
            let sectors_left_on_track = super::geometry::SECTORS_PER_TRACK + 1 - chs.sector;
            if sector_count > sectors_left_on_track {
                sector_count = sectors_left_on_track;
            }

            self.dma_prepare(sector_count, 0x56);
            self.dma_read(drive_select, chs)
                .await
                .map_err(|_| IoError::FileSystemError)?;

            let dma_buffer = self.get_dma_buffer();
            let available = sector_count * super::geometry::SECTOR_SIZE - read_offset;
            let copy_len = remaining.min(available);

            for i in 0..copy_len {
                buffer[buf_offset + i] = dma_buffer[read_offset + i];
            }

            buf_offset += copy_len;
            position += copy_len;
        }

        Ok(total as u32)
    }

    pub async fn write(&mut self, instance: u32, buffer: &[u8], offset: u32) -> IoResult {
        if buffer.is_empty() {
            return Ok(0);
        }

        let drive_select = match self.open_instances.get(&instance) {
            Some(file) => file.drive,
            None => return Err(IoError::FileHandleInvalid),
        };

        let mut buf_offset = 0usize;
        let mut position = offset as usize;
        let total = buffer.len();

        while buf_offset < total {
            let first_sector = position / super::geometry::SECTOR_SIZE;
            let write_offset = position % super::geometry::SECTOR_SIZE;
            let remaining = total - buf_offset;
            let last_sector = (position + remaining - 1) / super::geometry::SECTOR_SIZE;
            let mut sector_count = last_sector - first_sector + 1;

            // Clamp to DMA buffer size
            let dma_buf_size = 0x1000;
            let max_sectors = dma_buf_size / super::geometry::SECTOR_SIZE;
            if sector_count > max_sectors {
                sector_count = max_sectors;
            }

            // Clamp to track boundary
            let chs = ChsGeometry::from_lba(first_sector);
            let sectors_left_on_track = super::geometry::SECTORS_PER_TRACK + 1 - chs.sector;
            if sector_count > sectors_left_on_track {
                sector_count = sectors_left_on_track;
            }

            let available = sector_count * super::geometry::SECTOR_SIZE - write_offset;
            let copy_len = remaining.min(available);

            // If writing a partial sector, read-modify-write: read existing data first
            if write_offset != 0 || copy_len % super::geometry::SECTOR_SIZE != 0 {
                self.dma_prepare(sector_count, 0x56);
                self.dma_read(drive_select, chs)
                    .await
                    .map_err(|_| IoError::FileSystemError)?;
            }

            let dma_buffer = self.get_dma_buffer();
            for i in 0..copy_len {
                dma_buffer[write_offset + i] = buffer[buf_offset + i];
            }

            // DMA mode 0x5A = channel 2, single transfer, memory→peripheral
            self.dma_prepare(sector_count, 0x5A);
            self.dma_write(drive_select, chs)
                .await
                .map_err(|_| IoError::FileSystemError)?;

            buf_offset += copy_len;
            position += copy_len;
        }

        Ok(total as u32)
    }

    pub fn close(&mut self, instance: u32) -> IoResult {
        self.open_instances
            .remove(&instance)
            .map(|_| 1)
            .ok_or(IoError::FileHandleInvalid)
    }
}

struct OpenFile {
    drive: DriveSelect,
}

struct InterruptFuture {
    flag: Arc<AtomicBool>,
}

impl InterruptFuture {
    pub fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }
}

impl Future for InterruptFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.flag.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

struct NoOpWaker {}

impl NoOpWaker {
    pub fn new() -> Waker {
        Waker::from(Arc::new(Self {}))
    }
}

impl Wake for NoOpWaker {
    fn wake(self: Arc<Self>) {}

    fn wake_by_ref(self: &Arc<Self>) {}
}

struct DriverTask<'task> {
    future: Pin<Box<dyn Future<Output = ()> + 'task>>,
}

impl<'task> DriverTask<'task> {
    pub fn new(task: impl Future<Output = ()> + 'task) -> Self {
        Self {
            future: Box::pin(task),
        }
    }

    pub fn poll(&mut self, cx: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }
}

pub fn run_driver() -> ! {
    let task_id = get_current_id();
    crate::kprintln!("Install Floppy device driver ({:?})\n", task_id);

    let interrupt_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    // I know this event loop won't create multiple mutable references, but the
    // borrow checker doesn't...
    let driver_impl = Arc::new(RefCell::new(FloppyDeviceDriver::new(
        interrupt_flag.clone(),
    )));

    // detect drives
    let mut fd_count = 0;
    let drives = DriveType::read_cmos();
    for drive_type in drives {
        crate::kprintln!("    {}\n", drive_type);
        if let DriveType::None = drive_type {
            continue;
        }

        driver_impl.borrow_mut().set_device(fd_count, drive_type);
        let sub_id = fd_count as u32;
        fd_count += 1;
        let dev_name = alloc::format!("FD{}", fd_count);
        crate::kprintln!("Install driver as DEV:\\{}\n", dev_name);
        install_task_dev(dev_name.as_str(), task_id, sub_id);
    }

    let init_request = async {
        match driver_impl.clone().borrow_mut().init().await {
            Ok(_) => (),
            Err(_) => {
                crate::kprintln!("=!=! Failed to init floppy controller");
            }
        }
    };

    let _ = write_sync(Handle::new(0), &[fd_count as u8], 0);
    let _ = close_sync(Handle::new(0));

    // The first async action to run on the floppy controller should be
    // initialization
    let mut active_request: Option<DriverTask> = Some(DriverTask::new(init_request));
    let mut pending_requests: VecDeque<(TaskID, Message)> = VecDeque::new();

    // run event loop
    let messages = open_message_queue();
    let floppy_irq = open_interrupt_handle(6);
    let wake_set = create_wake_set();

    let mut interrupt_ready: [u8; 1] = [0];
    let mut incoming_message = Message::empty();
    let mut interrupt_read = AsyncOp::new(ASYNC_OP_READ, interrupt_ready.as_mut_ptr() as u32, 1, 0);
    let _ = send_io_op(floppy_irq, &interrupt_read, Some(wake_set));
    let mut message_read = AsyncOp::new(
        ASYNC_OP_READ,
        &mut incoming_message as *mut Message as u32,
        core::mem::size_of::<Message>() as u32,
        0,
    );
    let _ = send_io_op(messages, &message_read, Some(wake_set));

    loop {
        if interrupt_read.is_complete() {
            // acknowledge interrupt
            let _ = write_sync(floppy_irq, &[], 0);
            interrupt_flag.store(true, Ordering::SeqCst);
            interrupt_read = AsyncOp::new(ASYNC_OP_READ, interrupt_ready.as_mut_ptr() as u32, 1, 0);
            let _ = send_io_op(floppy_irq, &interrupt_read, Some(wake_set));
        } else if message_read.is_complete() {
            let sender = message_read.return_value.load(Ordering::SeqCst);
            pending_requests.push_back((TaskID::new(sender), incoming_message.clone()));

            message_read = AsyncOp::new(
                ASYNC_OP_READ,
                &mut incoming_message as *mut Message as u32,
                core::mem::size_of::<Message>() as u32,
                0,
            );
            let _ = send_io_op(messages, &message_read, Some(wake_set));
        } else {
            if active_request.is_none() {
                active_request = pending_requests.pop_front().map(|(_sender, message)| {
                    DriverTask::new(handle_driver_request(driver_impl.clone(), message))
                });
            }

            if let Some(ref mut req) = active_request {
                let waker = NoOpWaker::new();
                let mut cx = Context::from_waker(&waker);
                match req.poll(&mut cx) {
                    Poll::Ready(_) => {
                        active_request = None;
                    }
                    Poll::Pending => {}
                }
            }
            block_on_wake_set(wake_set, None);
        }
    }
}

async fn handle_driver_request(driver_ref: Arc<RefCell<FloppyDeviceDriver>>, message: Message) {
    match DriverCommand::from_u32(message.message_type) {
        DriverCommand::OpenRaw => {
            let sub_driver = message.args[0];
            let response = driver_ref.borrow_mut().open(sub_driver);
            driver_io_complete(message.unique_id, response);
        }
        DriverCommand::Read => {
            let instance = message.args[0];
            let buffer_ptr = message.args[1] as *mut u8;
            let buffer_len = message.args[2] as usize;
            let offset = message.args[3];
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_len) };
            let result = driver_ref.borrow_mut().read(instance, buffer, offset).await;
            driver_io_complete(message.unique_id, result);
        }
        DriverCommand::Write => {
            let instance = message.args[0];
            let buffer_ptr = message.args[1] as *const u8;
            let buffer_len = message.args[2] as usize;
            let offset = message.args[3];
            let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr, buffer_len) };
            let result = driver_ref.borrow_mut().write(instance, buffer, offset).await;
            driver_io_complete(message.unique_id, result);
        }
        _ => driver_io_complete(message.unique_id, Err(IoError::UnsupportedOperation)),
    }
}
