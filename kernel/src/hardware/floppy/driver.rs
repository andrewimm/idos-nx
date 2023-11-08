use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Waker, Poll};
use core::{pin::Pin, future::Future};

use alloc::sync::Arc;
use alloc::{boxed::Box, task::Wake};
use alloc::collections::VecDeque;
use idos_api::io::error::IOError;

use crate::io::driver::comms::{decode_command_and_id, DriverCommand, IOResult, DRIVER_RESPONSE_MAGIC};
use crate::task::actions::{yield_coop, send_message};
use crate::{task::{switching::get_current_id, actions::{handle::{open_message_queue, open_interrupt_handle, create_notify_queue, add_handle_to_notify_queue, wait_on_notify}, memory::map_memory}, messaging::Message, id::TaskID, memory::MemoryBacking, paging::page_on_demand}, memory::address::{VirtualAddress, PhysicalAddress}, io::{async_io::{OPERATION_FLAG_INTERRUPT, INTERRUPT_OP_LISTEN, OPERATION_FLAG_MESSAGE, MESSAGE_OP_READ, INTERRUPT_OP_ACK}, handle::PendingHandleOp}};

use super::controller::{FloppyController, Command, ControllerError, DriveSelect};
use super::geometry::ChsGeometry;

pub struct FloppyDeviceDriver {
    controller: FloppyController,
    dma_vaddr: VirtualAddress,
    dma_paddr: PhysicalAddress,
    interrupt_received: Arc<AtomicBool>,
}

impl FloppyDeviceDriver {
    pub fn new() -> Self {
        let dma_vaddr = map_memory(None, 0x1000, MemoryBacking::DMA).unwrap();
        let dma_paddr = page_on_demand(dma_vaddr).unwrap();

        Self {
            controller: FloppyController::new(),
            dma_vaddr,
            dma_paddr,
            interrupt_received: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn raise_interrupt(&self) {
        self.interrupt_received.store(true, Ordering::SeqCst);
    }

    pub async fn init(&self) -> Result<(), ControllerError> {
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
        
        crate::kprintln!("FLOPPY INIT SUCCESSFUL");

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
        self.controller.send_command(Command::Specify, &[8 << 4, 5 << 1])?;

        Ok(())
    }

    async fn send_command(&self, command: Command, params: &[u8]) -> Result<(), ControllerError> {
        if self.controller.get_status() & 0xc0 != 0x80 {
            self.reset().await?;
        }

        self.clear_interrupt();
        self.controller.send_command(command, params)
    }

    async fn dma(&self, command: Command, drive_number: u8, chs: ChsGeometry) -> Result<(), ControllerError> {
        self.send_command(
            command,
            &[
                (chs.head << 2) as u8 | drive_number,
                chs.cylinder as u8,
                chs.head as u8,
                chs.sector as u8,
                2,
                18, // Last sector on track
                0x1b, // GAP1 default size
                0xff,
            ],
        ).await?;

        self.wait_for_interrupt().await;
        let mut response = [0, 0, 0, 0, 0, 0, 0];
        self.controller.get_response(&mut response)?;
        // TODO: process response

        Ok(())
    }

    async fn dma_read(&self, drive: DriveSelect, chs: ChsGeometry) -> Result<(), ControllerError> {
        //self.select_drive(drive);
        let drive_number = match drive {
            DriveSelect::Primary => 0,
            DriveSelect::Secondary => 1,
        };
        self.dma(Command::ReadData, drive_number, chs).await
    }

    // Async IO methods:
    
    pub fn open(&self) -> IOResult {
        Ok(1)
    }

    pub async fn read(&self, instance: u32, buffer: &mut [u8]) -> IOResult {
        for i in 0..buffer.len() {
            buffer[i] = b'A';
        }
        Ok(buffer.len() as u32)
    }
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
    fn wake(self: Arc<Self>) {
    }

    fn wake_by_ref(self: &Arc<Self>) {
    }
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

    // detect drives

    crate::io::filesystem::install_async_dev("FD", task_id);
    
    // run event loop
    let messages = open_message_queue();
    let mut incoming_message = Message(0, 0, 0, 0);
    let interrupt = open_interrupt_handle(6);
    let notify = create_notify_queue();
    add_handle_to_notify_queue(notify, messages);
    add_handle_to_notify_queue(notify, interrupt);

    let mut driver_impl = FloppyDeviceDriver::new();

    let mut interrupt_read = PendingHandleOp::new(interrupt, OPERATION_FLAG_INTERRUPT | INTERRUPT_OP_LISTEN, 0, 0, 0);
    let mut message_read = PendingHandleOp::new(messages, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &mut incoming_message as *mut Message as u32, 0, 0);

    let init_request = async {
        match driver_impl.init().await {
            Ok(_) => (),
            Err(_) => {
                crate::kprintln!("=!=! Failed to init floppy controller");
            },
        }
    };

    // The first async action to run on the floppy controller should be
    // initialization
    let mut active_request: Option<DriverTask> = Some(DriverTask::new(init_request));

    let mut pending_requests: VecDeque<(TaskID, Message)> = VecDeque::new();

    loop {
        if interrupt_read.is_complete() {
            PendingHandleOp::new(interrupt, OPERATION_FLAG_INTERRUPT | INTERRUPT_OP_ACK, 0, 0, 0);
            driver_impl.raise_interrupt();
            interrupt_read = PendingHandleOp::new(interrupt, OPERATION_FLAG_INTERRUPT | INTERRUPT_OP_LISTEN, 0, 0, 0);
        } else if let Some(sender) = message_read.get_result() {
            pending_requests.push_back((TaskID::new(sender), incoming_message.clone()));

            message_read = PendingHandleOp::new(messages, OPERATION_FLAG_MESSAGE | MESSAGE_OP_READ, &mut incoming_message as *mut Message as u32, 0, 0);
        } else {
            if active_request.is_none() {
                active_request = pending_requests.pop_front().map(|(sender, message)| {
                    DriverTask::new(
                        handle_driver_request(&driver_impl, sender, message)
                    )
                });
            }

            if let Some(ref mut req) = active_request {
                let waker = NoOpWaker::new();
                let mut cx = Context::from_waker(&waker);
                match req.poll(&mut cx) {
                    Poll::Ready(_) => {
                        active_request = None;
                    },
                    Poll::Pending => {},
                }
            }
            wait_on_notify(notify, None);
        }
    }
}

async fn handle_driver_request(driver: &FloppyDeviceDriver, respond_to: TaskID, message: Message) {
    let (command, request_id) = decode_command_and_id(message.0);
        match command {
            DriverCommand::Open => {
                let response = driver.open();
                send_response(respond_to, request_id, response);
            },
            DriverCommand::Read => {
                let instance = message.1;
                let buffer_ptr = message.2 as *mut u8;
                let buffer_len = message.3 as usize;
                let buffer = unsafe {
                    core::slice::from_raw_parts_mut(buffer_ptr, buffer_len)
                };
                let result = driver.read(instance, buffer).await;
                send_response(respond_to, request_id, result);
            },
            _ => send_response(respond_to, request_id, Err(IOError::UnsupportedOperation)),
        }
}

fn send_response(task: TaskID, request_id: u32, result: IOResult) {
    let message = match result {
        Ok(result) => {
            let code = result & 0x7fffffff;
            Message(DRIVER_RESPONSE_MAGIC, request_id, code, 0)
        },
        Err(err) => {
            let code = Into::<u32>::into(err) | 0x80000000;
            Message(DRIVER_RESPONSE_MAGIC, request_id, code, 0)
        },
    };
    send_message(task, message, 0xffffffff);
}
