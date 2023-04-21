use alloc::collections::{VecDeque, BTreeMap};
use alloc::sync::Arc;
use spin::{RwLock, Mutex, Once, MutexGuard};
use crate::filesystem::drivers::asyncfs::{encode_request, ASYNC_RESPONSE_MAGIC};
use crate::task::actions::lifecycle::wait_for_io;
use crate::task::actions::{read_message_blocking, send_message};
use crate::task::id::TaskID;
use crate::task::messaging::Message;
use crate::task::switching::{get_current_id, get_task};

#[derive(Copy, Clone, Debug)]
pub enum AsyncIO {
    // Open(path str pointer, path str length)
    Open(u32, u32),
    // Open a handle to the driver itself, with no path
    OpenRaw,
    // Read(buffer pointer, buffer length)
    Read(u32, u32),
    // Write(buffer pointer, buffer length)
    Write(u32, u32),
    // Close(handle id)
    Close(u32),
}

pub type AsyncResponse = Arc<Mutex<Option<u32>>>;

/// Enqueue a new IO request. Assuming the target is a valid FS driver, the
/// current task will be IO-blocked until the request completes. The Arbiter
/// will consume the AsyncIO request and send an appropriate message to the FS
/// driver.
pub fn begin_io(driver_id: TaskID, io: AsyncIO, response: AsyncResponse) {
    let current_id = get_current_id();
    // Add the request to the queue
    get_arbiter_queue().push_back(
        IncomingRequest {
            driver_id,
            requestor_id: current_id,
            io,
            response,
        }
    );
    
    // Make sure the arbiter is awake
    let id = get_arbiter_task_id();
    send_message(id, Message::empty(), 0xffffffff);
    wait_for_io(None);
}

struct IncomingRequest {
    pub driver_id: TaskID,
    pub requestor_id: TaskID,
    pub io: AsyncIO,
    pub response: AsyncResponse,
}

static ARBITER_TASK_ID: RwLock<TaskID> = RwLock::new(TaskID::new(0));
static ARBITER_QUEUE: Once<Mutex<VecDeque<IncomingRequest>>> = Once::new();
static OUTBOUND: Mutex<BTreeMap<TaskID, VecDeque<IncomingRequest>>> = Mutex::new(BTreeMap::new());

pub fn get_arbiter_task_id() -> TaskID {
    *ARBITER_TASK_ID.read()
}

fn get_arbiter_queue() -> MutexGuard<'static, VecDeque<IncomingRequest>> {
    ARBITER_QUEUE.call_once(|| {
        Mutex::new(VecDeque::new())
    }).lock()
}

fn add_outbound(request: IncomingRequest) -> usize {
    let mut tree = OUTBOUND.lock();
    let id = request.driver_id;
    match tree.get_mut(&id) {
        Some(queue) => {
            queue.push_back(request);
            queue.len()
        },
        None => {
            let mut queue = VecDeque::new();
            queue.push_back(request);
            let len = queue.len();
            tree.insert(id, queue);
            len
        },
    }
}

fn pop_pending_request(sender: TaskID) -> Option<IncomingRequest> {
    let mut tree = OUTBOUND.lock();
    let queue = tree.get_mut(&sender)?;
    queue.pop_front()
}

fn peek_pending_request(sender: TaskID) -> Option<AsyncIO> {
    let mut tree = OUTBOUND.lock();
    let queue = tree.get_mut(&sender)?;
    queue.front().map(|front| front.io.clone())
}

/// The core loop of the Arbiter task. The Arbiter exists as an independent
/// kernel-level task so that other 
pub fn arbiter_task() -> ! {
    let id = get_current_id();
    *ARBITER_TASK_ID.write() = id;

    loop {
        // reading this is only necessary when we start handling responses from
        // drivers
        let (message_read, _) = read_message_blocking(None);

        crate::kprint!("= Arbiter woke up\n");

        if let Some(next_message) = message_read {
            let (sender, message) = next_message.open();
            if message.0 == ASYNC_RESPONSE_MAGIC {
                // it's a response to a request
                match pop_pending_request(sender) {
                    Some(request) => {
                        // TODO: error handling
                        request.response.lock().replace(message.1);

                        crate::kprint!("  IO complete, resume {:?}\n", request.requestor_id);
                        if let Some(task_lock) = get_task(request.requestor_id) {
                            let mut task = task_lock.write();
                            task.io_complete();
                        }
                    },
                    None => (),
                }

                // if the driver has other messages queued up, send another one
                match peek_pending_request(sender) {
                    Some(request_io) => {
                        crate::kprint!("  Another IO request queued, beginning it!\n");
                        let next_message = encode_request(request_io);
                        send_message(sender, next_message, 0xffffffff);
                    },
                    None => (),
                }
            }
        }

        {
            let mut queue = get_arbiter_queue();
            // Once the task is awake, read all the incoming requests.
            // The Arbiter will use the task ID to determine the destination of
            // this request. If no request is currently outstanding, 
            //
            // Notice that this logic is specific to file IO operations, but 
            // has no concept of higher level file systems. That allows the
            // same Arbiter task to be used for device drivers in the DEV: FS.
            loop {
                let head = queue.pop_front();
                match head {
                    Some(request) => {
                        let driver_id = request.driver_id;
                        let io = request.io.clone();
                        crate::kprint!("  IO Req: {:?}, to {:?}\n", io, request.driver_id);
                        // look up queue for id, or create it if it doesn't exist
                        let len = add_outbound(request);
                        if len <= 1 {
                            // it's the only pending request to that driver
                            
                            // TODO: actually implement kernel-side buffers and
                            // pass the data to the driver
                            let message = encode_request(io);
                            send_message(driver_id, message, 0xffffffff);
                            crate::kprint!("  Async message sent to {:?}\n", driver_id);
                        }
                    },
                    None => break,
                }
            }
        }

        crate::kprint!("= Arbiter sleep\n");
    }
}
