pub mod devfs;
pub mod driver;
pub mod fatfs;
pub mod sysfs;
pub mod taskfs;
#[cfg(test)]
pub mod testing;

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use idos_api::io::driver::DriverMappingToken;
use idos_api::io::error::{IoError, IoResult};
use idos_api::io::file::FileStatus;
use spin::{Once, RwLock};

use crate::files::path::Path;
use crate::memory::address::{PhysicalAddress, VirtualAddress};
use crate::memory::shared::{release_buffer, share_buffer};
use crate::task::actions::memory::{map_memory, unmap_memory};
use crate::task::id::TaskID;
use crate::task::switching::get_current_id;

use self::devfs::DevFileSystem;
use self::driver::{AsyncIOCallback, DriverID, DriverType, InstalledDriver};

use super::async_io::AsyncOpID;
use super::driver::comms::DriverIoAction;
use super::driver::pending::send_async_request;

static INSTALLED_DRIVERS: RwLock<BTreeMap<u32, (String, DriverType)>> =
    RwLock::new(BTreeMap::new());
static NEXT_DRIVER_ID: AtomicU32 = AtomicU32::new(1);
static DEV_FS: Once<DriverType> = Once::new();

pub fn get_dev_fs() -> &'static DriverType {
    DEV_FS.call_once(|| DriverType::KernelFilesystem(Box::new(DevFileSystem::new())))
}

pub fn get_driver_id_by_name(name: &str) -> Option<DriverID> {
    let drivers = INSTALLED_DRIVERS.read();
    for (id, (drive_name, _)) in drivers.iter() {
        if drive_name.as_str() == name {
            return Some(DriverID::new(*id));
        }
    }
    None
}

pub fn get_all_drive_names() -> Vec<String> {
    let drivers = INSTALLED_DRIVERS.read();
    drivers
        .iter()
        .filter_map(|(_, (name, driver))| match driver {
            DriverType::KernelFilesystem(_) | DriverType::TaskFilesystem(_) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

pub fn get_all_dev_names() -> Vec<String> {
    let drivers = INSTALLED_DRIVERS.read();
    drivers
        .iter()
        .filter_map(|(_, (name, driver))| match driver {
            DriverType::KernelDevice(_) | DriverType::TaskDevice(_, _) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

pub fn install_kernel_fs(name: &str, driver: InstalledDriver) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS
        .write()
        .insert(id, (name.to_string(), DriverType::KernelFilesystem(driver)));
    DriverID::new(id)
}

pub fn install_kernel_dev(name: &str, driver: InstalledDriver) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS
        .write()
        .insert(id, (name.to_string(), DriverType::KernelDevice(driver)));
    DriverID::new(id)
}

pub fn install_task_fs(name: &str, task: TaskID) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS
        .write()
        .insert(id, (name.to_string(), DriverType::TaskFilesystem(task)));
    DriverID::new(id)
}

pub fn install_task_dev(name: &str, task: TaskID, sub_driver: u32) -> DriverID {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    INSTALLED_DRIVERS.write().insert(
        id,
        (name.to_string(), DriverType::TaskDevice(task, sub_driver)),
    );
    DriverID::new(id)
}

pub fn with_driver<F>(driver_id: DriverID, f: F) -> Option<IoResult>
where
    F: FnOnce(&DriverType) -> Option<IoResult>,
{
    if driver_id.is_dev() {
        return f(get_dev_fs());
    }

    let drivers = INSTALLED_DRIVERS.read();
    let (_, driver) = match drivers.get(&driver_id) {
        Some(d) => d,
        None => return Some(Err(IoError::NotFound)),
    };
    f(driver)
}

/// Run the open() operation on an installed driver
pub fn driver_open(
    driver_id: DriverID,
    path: Path,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    with_driver(driver_id, |driver| {
        match driver {
            DriverType::KernelFilesystem(fs) => {
                return fs.open(Some(path), io_callback);
            }
            DriverType::KernelDevice(dev) => {
                return dev.open(None, io_callback);
            }
            DriverType::TaskDevice(dev, sub) => {
                let action = DriverIoAction::OpenRaw { driver_id: *sub };
                send_async_request(*dev, io_callback, action);
                return None;
            }
            DriverType::TaskFilesystem(task) => {
                // Unlike other ops that wait for a buffer to be filled, the
                // path string passed to the original call is never used again.
                // This means the original memory can easily be dropped by the
                // time the driver tries to consume the path. In order to
                // ensure a version of the string is still available, we create
                // a copy that will be dropped when the op completes.
                //
                // This copy could live in the kernel heap, but that would
                // require exposing an entire page of kernel heap to the
                // driver. Tasks can be corrupted by misbehaving drivers, but
                // the kernel should remain protected. We create a new frame
                // of memory to contain this string copy, and then share it
                // with the driver task.

                let path_len = path.as_str().len();
                let action = if path_len == 0 {
                    // can't share memory for an empty slice, just hardcode it
                    DriverIoAction::Open {
                        path_str_vaddr: VirtualAddress::new(0),
                        path_str_len: 0,
                    }
                } else {
                    // create a new frame of memory
                    let page_start =
                        map_memory(None, 0x1000, crate::task::memory::MemoryBacking::FreeMemory)
                            .unwrap();
                    let path_slice = unsafe {
                        core::slice::from_raw_parts_mut(page_start.as_ptr_mut::<u8>(), path_len)
                    };
                    path_slice.copy_from_slice(path.as_str().as_bytes());
                    let shared_vaddr = share_buffer(*task, page_start, path_len);
                    release_buffer(page_start, path_len);
                    DriverIoAction::Open {
                        path_str_vaddr: shared_vaddr,
                        path_str_len: path_len,
                    }
                };

                send_async_request(*task, io_callback, action);
                None
            }
        }
    })
}

/// Run a path-based operation (mkdir/unlink/rmdir) on a driver.
/// These operations don't require an open file instance.
fn driver_path_op(
    driver_id: DriverID,
    path: Path,
    io_callback: AsyncIOCallback,
    kernel_fn: fn(&dyn super::driver::kernel_driver::KernelDriver, &str, AsyncIOCallback) -> Option<IoResult>,
    make_action: fn(VirtualAddress, usize) -> DriverIoAction,
) -> Option<IoResult> {
    let path_len = path.as_str().len();
    if path_len == 0 {
        return Some(Err(IoError::InvalidArgument));
    }

    with_driver(driver_id, |driver| {
        match driver {
            DriverType::KernelFilesystem(fs) => {
                return kernel_fn(&**fs, path.as_str(), io_callback);
            }
            DriverType::KernelDevice(_) | DriverType::TaskDevice(_, _) => {
                return Some(Err(IoError::UnsupportedOperation));
            }
            DriverType::TaskFilesystem(task) => {
                let page_start =
                    map_memory(None, 0x1000, crate::task::memory::MemoryBacking::FreeMemory)
                        .unwrap();
                let path_slice = unsafe {
                    core::slice::from_raw_parts_mut(page_start.as_ptr_mut::<u8>(), path_len)
                };
                path_slice.copy_from_slice(path.as_str().as_bytes());
                let shared_vaddr = share_buffer(*task, page_start, path_len);
                release_buffer(page_start, path_len);
                let action = make_action(shared_vaddr, path_len);

                send_async_request(*task, io_callback, action);
                None
            }
        }
    })
}

pub fn driver_mkdir(
    driver_id: DriverID,
    path: Path,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    driver_path_op(
        driver_id,
        path,
        io_callback,
        |d, p, cb| d.mkdir(p, cb),
        |vaddr, len| DriverIoAction::Mkdir {
            path_str_vaddr: vaddr,
            path_str_len: len,
        },
    )
}

pub fn driver_unlink(
    driver_id: DriverID,
    path: Path,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    driver_path_op(
        driver_id,
        path,
        io_callback,
        |d, p, cb| d.unlink(p, cb),
        |vaddr, len| DriverIoAction::Unlink {
            path_str_vaddr: vaddr,
            path_str_len: len,
        },
    )
}

pub fn driver_rmdir(
    driver_id: DriverID,
    path: Path,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    driver_path_op(
        driver_id,
        path,
        io_callback,
        |d, p, cb| d.rmdir(p, cb),
        |vaddr, len| DriverIoAction::Rmdir {
            path_str_vaddr: vaddr,
            path_str_len: len,
        },
    )
}

pub fn driver_close(id: DriverID, instance: u32, io_callback: AsyncIOCallback) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            d.close(instance, io_callback)
        }

        DriverType::TaskFilesystem(task_id) | DriverType::TaskDevice(task_id, _) => {
            let action = DriverIoAction::Close { instance };
            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_read(
    id: DriverID,
    instance: u32,
    buffer: &mut [u8],
    offset: u32,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            d.read(instance, buffer, offset, io_callback)
        }

        DriverType::TaskFilesystem(task_id) | DriverType::TaskDevice(task_id, _) => {
            let range_start = VirtualAddress::new(buffer.as_ptr() as u32);
            let shared_vaddr = share_buffer(*task_id, range_start, buffer.len());

            let action = DriverIoAction::Read {
                instance,
                buffer_ptr_vaddr: shared_vaddr,
                buffer_len: buffer.len(),
                starting_offset: offset,
            };

            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_write(
    id: DriverID,
    instance: u32,
    buffer: &[u8],
    offset: u32,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            d.write(instance, buffer, offset, io_callback)
        }

        DriverType::TaskFilesystem(task_id) | DriverType::TaskDevice(task_id, _) => {
            let range_start = VirtualAddress::new(buffer.as_ptr() as u32);
            let shared_vaddr = share_buffer(*task_id, range_start, buffer.len());

            let action = DriverIoAction::Write {
                instance,
                buffer_ptr_vaddr: shared_vaddr,
                buffer_len: buffer.len(),
                starting_offset: offset,
            };

            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_stat(
    id: DriverID,
    instance: u32,
    file_status: &mut FileStatus,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            d.stat(instance, file_status, io_callback)
        }

        DriverType::TaskFilesystem(task_id) | DriverType::TaskDevice(task_id, _) => {
            let range_start = VirtualAddress::new(file_status as *mut FileStatus as u32);
            let shared_vaddr =
                share_buffer(*task_id, range_start, core::mem::size_of::<FileStatus>());

            let action = DriverIoAction::Stat {
                instance,
                stat_ptr_vaddr: shared_vaddr,
                stat_len: core::mem::size_of::<FileStatus>(),
            };

            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_share(
    id: DriverID,
    instance: u32,
    transfer_to: TaskID,
    is_move: bool,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            d.share(instance, transfer_to, is_move, io_callback)
        }

        DriverType::TaskFilesystem(task_id) | DriverType::TaskDevice(task_id, _) => {
            let action = DriverIoAction::Share {
                instance,
                dest_task_id: transfer_to,
                is_move,
            };
            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_ioctl(
    id: DriverID,
    instance: u32,
    ioctl: u32,
    arg: u32,
    arg_len: usize,
    io_callback: AsyncIOCallback,
) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            d.ioctl(instance, ioctl, arg, arg_len, io_callback)
        }

        DriverType::TaskFilesystem(task_id) | DriverType::TaskDevice(task_id, _) => {
            let action = if arg_len > 0 {
                // validate that the pointer can be used safely
                // TODO: maybe fail if arg is in kernel space? Could cause some gnarly problems for uspace programs to do that

                let struct_start = VirtualAddress::new(arg);
                let shared_vaddr = share_buffer(*task_id, struct_start, arg_len);
                DriverIoAction::IoctlStruct {
                    instance,
                    ioctl,
                    arg_ptr_vaddr: shared_vaddr,
                    arg_len,
                }
            } else {
                DriverIoAction::Ioctl {
                    instance,
                    ioctl,
                    arg,
                }
            };
            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_create_mapping(id: DriverID, path: Path) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            match d.create_mapping(path.as_str()) {
                Some(Ok(token)) => Some(Ok(*token)),
                Some(Err(e)) => Some(Err(e)),
                None => None,
            }
        }
        DriverType::TaskDevice(task_id, sub_id) => {
            // let's avoid allocating memory to pass the sub_id
            // Without implementing a separate enum value, we can pass it
            // as the path ptr with a zero-length string.
            // The receiving code can check the length and interpret the pointer
            // appropriately.
            let action = DriverIoAction::CreateFileMapping {
                path_str_vaddr: VirtualAddress::new(*sub_id),
                path_str_len: 0,
            };
            let io_callback: AsyncIOCallback = (get_current_id(), 0, AsyncOpID::new(0));
            send_async_request(*task_id, io_callback, action);
            None
        }
        DriverType::TaskFilesystem(task_id) => {
            // like driver_open, we need to copy the string to a new frame for
            // security reasons
            let path_len = path.as_str().len();
            let action = if path_len == 0 {
                // can't share memory for an empty slice, just hardcode it using
                // a sentinel for the address.
                DriverIoAction::CreateFileMapping {
                    path_str_vaddr: VirtualAddress::new(0xffff_ffff),
                    path_str_len: 0,
                }
            } else {
                // create a new frame of memory
                let page_start =
                    map_memory(None, 0x1000, crate::task::memory::MemoryBacking::FreeMemory)
                        .unwrap();
                let path_slice = unsafe {
                    core::slice::from_raw_parts_mut(page_start.as_ptr_mut::<u8>(), path_len)
                };
                path_slice.copy_from_slice(path.as_str().as_bytes());
                let shared_vaddr = share_buffer(*task_id, page_start, path_len);
                unmap_memory(page_start, 0x1000).unwrap();
                DriverIoAction::CreateFileMapping {
                    path_str_vaddr: shared_vaddr,
                    path_str_len: path_len,
                }
            };
            let io_callback: AsyncIOCallback = (get_current_id(), 0, AsyncOpID::new(0));
            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_page_in_file(
    id: DriverID,
    token: DriverMappingToken,
    offset_in_file: u32,
    frame_paddr: PhysicalAddress,
) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => {
            d.page_in_mapping(token, offset_in_file, frame_paddr.as_u32())
        }
        DriverType::TaskDevice(task_id, _) | DriverType::TaskFilesystem(task_id) => {
            let action = DriverIoAction::PageInFileMapping {
                mapping_token: *token,
                offset_in_file,
                frame_paddr,
            };
            let io_callback: AsyncIOCallback = (get_current_id(), 0, AsyncOpID::new(0));
            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}

pub fn driver_remove_mapping(id: DriverID, token: DriverMappingToken) -> Option<IoResult> {
    with_driver(id, |driver| match driver {
        DriverType::KernelFilesystem(d) | DriverType::KernelDevice(d) => d.remove_mapping(token),
        DriverType::TaskDevice(task_id, _) | DriverType::TaskFilesystem(task_id) => {
            let action = DriverIoAction::RemoveFileMapping {
                mapping_token: *token,
            };
            let io_callback: AsyncIOCallback = (get_current_id(), 0, AsyncOpID::new(0));
            send_async_request(*task_id, io_callback, action);
            None
        }
    })
}
