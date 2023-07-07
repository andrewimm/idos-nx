use core::sync::atomic::{AtomicU32, Ordering};

use crate::collections::SlotList;
use crate::files::cursor::SeekMethod;
use crate::files::error::IOError;
use crate::filesystem::drivers::asyncfs::AsyncDriver;
use crate::hardware::dma::DmaChannelRegisters;
use crate::interrupts::pic::install_interrupt_handler;
use crate::memory::address::{VirtualAddress, PhysicalAddress};
use crate::task::actions::io::{open_pipe, transfer_handle, write_file, read_file};
use crate::task::actions::lifecycle::{create_kernel_task, wait_for_io};
use crate::task::actions::memory::map_memory;
use crate::task::actions::{read_message_blocking, send_message, yield_coop};
use crate::task::files::FileHandle;
use crate::task::id::TaskID;
use crate::task::memory::MemoryBacking;
use crate::task::paging::page_on_demand;
use crate::task::switching::{get_current_id, get_task};
use crate::filesystem::install_device_driver;
use super::controller::{DriveSelect, DriveType, Command, ControllerError, FloppyController};

pub struct FloppyDriver {
    attached: [Option<AttachedDrive>; 2],
    selected_drive: Option<DriveSelect>,
    controller: FloppyController,
    open_handle_map: SlotList<OpenHandle>,
    dma_vaddr: VirtualAddress,
    dma_paddr: PhysicalAddress,
}

impl FloppyDriver {
    pub fn new() -> Self {
        let dma_vaddr = map_memory(None, 0x1000, MemoryBacking::DMA).unwrap();
        let dma_paddr = page_on_demand(dma_vaddr).unwrap();
        Self {
            attached: [None, None],
            selected_drive: None,
            controller: FloppyController::new(),
            open_handle_map: SlotList::new(),
            dma_vaddr,
            dma_paddr,
        }
    }

    pub fn init(&mut self) -> Result<(), ControllerError> {
        let mut response = [0];

        self.send_command(Command::Version, &[])?;
        self.controller.get_response(&mut response)?;
        if response[0] != 0x90 {
            return Err(ControllerError::UnsupportedController);
        }
        // 0x57 = 0b01010111
        //           | enable implied seek
        //            | enable fifo
        //             | disable polling
        //              | threshold is 8 bytes
        self.send_command(Command::Configure, &[0, 0x57, 0])?;
        self.send_command(Command::Lock, &[])?;
        self.controller.get_response(&mut response)?;
        assert_eq!(response[0], 0x10);

        self.reset()?;
        
        // TODO: only turn on the motors when the drives are needed
        if !self.attached[0].is_none() {
            self.controller.ensure_motor_on(DriveSelect::Primary);

            self.recalibrate(DriveSelect::Primary)?;
        }
        if !self.attached[1].is_none() {
            self.controller.ensure_motor_on(DriveSelect::Secondary);

            self.recalibrate(DriveSelect::Secondary)?;
        }

        Ok(())
    }

    pub fn set_device(&mut self, index: usize, drive_type: DriveType) {
        self.attached[index] = Some(
            AttachedDrive {
                open_count: 0,
                drive_type,
            }
        );
    }

    fn select_drive(&mut self, drive: DriveSelect) {
        if self.selected_drive == Some(drive) {
            return;
        }
        let dor = self.controller.dor_read();
        let flag = match drive {
            DriveSelect::Primary => 0,
            DriveSelect::Secondary => 1,
        };
        self.controller.dor_write(
            (dor & 0xfc) | flag
        );
        self.selected_drive = Some(drive);
    }

    fn send_command(&self, command: Command, params: &[u8]) -> Result<(), ControllerError> {
        if self.controller.get_status() & 0xc0 != 0x80 {
            self.reset()?;
        }

        self.mark_ready_for_interrupt();
        self.controller.send_command(command, params)
    }

    fn reset(&self) -> Result<(), ControllerError> {
        self.mark_ready_for_interrupt();
        self.controller.dor_write(0);
        yield_coop();
        // Motors off, reset + IRQ enabled, select disk 0
        self.controller.dor_write(0x0c);
        self.wait_for_interrupt(None);

        let mut sense = [0, 0];
        for _ in 0..4 {
            self.send_command(Command::SenseInterrupt, &[])?;
            self.controller.get_response(&mut sense)?;
        }

        // TODO: Set the data rate correctly for different drive type
        self.controller.ccr_write(0);
        // SRT=8, HUT=0, HLT=5, NDMA=0
        self.send_command(Command::Specify, &[8 << 4, 5 << 1])?;

        Ok(())
    }

    fn recalibrate(&mut self, drive: DriveSelect) -> Result<(), ControllerError> {
        self.select_drive(drive);

        let mut st0 = [0, 0];
        for _retry in 0..2 {
            self.send_command(Command::Recalibrate, &[0])?;
            self.wait_for_interrupt(None);
            self.send_command(Command::SenseInterrupt, &[])?;
            self.controller.get_response(&mut st0)?;

            if st0[0] & 0x20 == 0x20 {
                break;
            }
        }

        Ok(())
    }

    fn mark_ready_for_interrupt(&self) {
        let task_id = get_current_id();
        let _ = BLOCKED_DRIVER_TASK.swap(task_id.into(), Ordering::SeqCst);
    }

    fn wait_for_interrupt(&self, timeout: Option<u32>) {
        let blocked_id = BLOCKED_DRIVER_TASK.load(Ordering::SeqCst);
        if blocked_id == 0 {
            return;
        }
        wait_for_io(timeout);
    }

    fn dma(&self, command: Command, drive_number: u8, chs: ChsGeometry) -> Result<(), ControllerError> {
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
        )?;

        self.wait_for_interrupt(None);
        let mut response = [0, 0, 0, 0, 0, 0, 0];
        self.controller.get_response(&mut response)?;
        // TODO: process response
        
        Ok(())
    }

    fn read(&mut self, drive: DriveSelect, chs: ChsGeometry) -> Result<(), ControllerError> {
        self.select_drive(drive);
        let drive_number = match drive {
            DriveSelect::Primary => 0,
            DriveSelect::Secondary => 1,
        };
        self.dma(Command::ReadData, drive_number, chs)
    }

    fn write(&mut self, drive: DriveSelect, chs: ChsGeometry) -> Result<(), ControllerError> {
        self.select_drive(drive);
        let drive_number = match drive {
            DriveSelect::Primary => 0,
            DriveSelect::Secondary => 1,
        };
        self.dma(Command::WriteData, drive_number, chs)
    }

    fn get_dma_buffer(&self) -> &mut [u8] {
        unsafe {
            let buffer_start = self.dma_vaddr.as_u32() as *mut u8;
            let buffer_length = 0x1000;
            core::slice::from_raw_parts_mut(buffer_start, buffer_length)
        }
    }

    fn dma_prepare_load_sectors(&self, count: usize, dma_mode: u8) {
        let dma_channel = DmaChannelRegisters::for_channel(2);
        dma_channel.set_address(self.dma_paddr);
        dma_channel.set_count((count * SECTOR_SIZE) as u32 - 1);
        dma_channel.set_mode(dma_mode);
    }
}

impl AsyncDriver for FloppyDriver {
    fn open(&mut self, path: &str) -> Result<u32, IOError> {
        let index = path.parse::<usize>().unwrap();
        match self.attached.get(index) {
            None => return Err(IOError::NotFound),
            _ => (),
        }
        let drive = match index {
            1 => DriveSelect::Secondary,
            _ => DriveSelect::Primary,
        };
        let handle = OpenHandle {
            drive,
            position: 0,
        };
        Ok(self.open_handle_map.insert(handle) as u32)
    }

    fn read(&mut self, instance: u32, buffer: &mut [u8]) -> Result<u32, IOError> {
        let (drive_select, position) = match self.open_handle_map.get(instance as usize) {
            Some(handle) => (handle.drive, handle.position),
            None => return Err(IOError::FileHandleInvalid), // handle doesn't exist
        };

        let first_sector = position / SECTOR_SIZE;
        let read_offset = position % SECTOR_SIZE;
        let last_sector = (position + buffer.len()) / SECTOR_SIZE;
        let sector_count = last_sector - first_sector + 1;

        self.dma_prepare_load_sectors(sector_count, 0x56);
        let chs = ChsGeometry::from_lba(first_sector);
        self.read(drive_select, chs).unwrap();

        let dma_buffer = self.get_dma_buffer();

        for i in 0..buffer.len() {
            buffer[i] = dma_buffer[read_offset + i];
        }

        let bytes_read = buffer.len();

        self.open_handle_map.get_mut(instance as usize).unwrap().position += bytes_read;

        Ok(bytes_read as u32)
    }

    fn write(&mut self, _instance: u32, _buffer: &[u8]) -> Result<u32, IOError> {
        Err(IOError::UnsupportedOperation)
    }

    fn close(&mut self, handle: u32) -> Result<(), IOError> {
        if self.open_handle_map.remove(handle as usize).is_some() {
            Ok(())
        } else {
            Err(IOError::FileHandleInvalid)
        }
    }

    fn seek(&mut self, instance: u32, offset: SeekMethod) -> Result<u32, IOError> {
        let current_position = match self.open_handle_map.get(instance as usize) {
            Some(handle) => handle.position,
            None => return Err(IOError::FileHandleInvalid),
        };
        let new_position = offset.from_current_position(current_position);
        self.open_handle_map.get_mut(instance as usize).unwrap().position = new_position;
        Ok(new_position as u32)
    }
}

struct AttachedDrive {
    /// How many open handles reference this drive
    open_count: usize,
    /// Size and type of the disk in the drive
    drive_type: DriveType,
}

#[derive(Copy, Clone)]
struct OpenHandle {
    drive: DriveSelect,
    position: usize,
}

const SECTORS_PER_TRACK: usize = 18;
const SECTOR_SIZE: usize = 512; 

#[derive(Debug)]
struct ChsGeometry {
    pub cylinder: usize,
    pub head: usize,
    pub sector: usize,
}

impl ChsGeometry {
    pub fn from_lba(lba: usize) -> Self {
        let sectors_per_cylinder = 2 * SECTORS_PER_TRACK;
        let cylinder = lba / sectors_per_cylinder;
        let cylinder_offset = lba % sectors_per_cylinder;
        let head = cylinder_offset / SECTORS_PER_TRACK;
        let sector = cylinder_offset % SECTORS_PER_TRACK;

        Self {
            cylinder,
            head,
            sector: sector + 1,
        }
    }
}

fn run_driver() -> ! {
    let task_id = get_current_id();
    crate::kprint!("Install Floppy device driver ({:?})\n", task_id);

    let mut fd_count = 0;

    let drives = DriveType::read_cmos();
    let mut driver_impl = FloppyDriver::new();
    install_interrupt_handler(6, floppy_interrupt_handler, None);

    for drive_type in drives {
        crate::kprint!("    {}\n", drive_type);

        if let DriveType::None = drive_type {
            continue;
        }

        driver_impl.set_device(fd_count, drive_type);
        let sub_id = fd_count as u32;
        fd_count += 1;
        let dev_name = alloc::format!("FD{}", fd_count);
        crate::kprint!("Install driver as DEV:\\{}\n", dev_name);
        install_device_driver(dev_name.as_str(), task_id, sub_id).unwrap();
    }

    driver_impl.init().unwrap();

    crate::kprint!("Detected {} Floppy drive(s)\n", fd_count);

    write_file(FileHandle::new(0), &[1]).unwrap();
 
    loop {
        let (message_read, _) = read_message_blocking(None);
        if let Some(packet) = message_read {
            let (sender, message) = packet.open();

            match driver_impl.handle_request(message) {
                Some(response) => send_message(sender, response, 0xffffffff),
                None => continue,
            }
        }

    }
}

static BLOCKED_DRIVER_TASK: AtomicU32 = AtomicU32::new(0);

pub fn floppy_interrupt_handler(_irq: u32) {
    let task_complete = BLOCKED_DRIVER_TASK.swap(0, Ordering::SeqCst);
    if task_complete == 0 {
        return;
    }

    let task_lock = get_task(TaskID::new(task_complete));
    if let Some(lock) = task_lock {
        lock.write().io_complete();
    }
}


pub fn install_drivers() {
    let (pipe_read, pipe_write) = open_pipe().unwrap();
    let driver_task = create_kernel_task(run_driver, Some("FLOPPYDEV"));
    transfer_handle(pipe_write, driver_task).unwrap();

    read_file(pipe_read, &mut [0u8]).unwrap();
}
