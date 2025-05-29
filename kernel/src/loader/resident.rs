//! The Loader Task is a resident that receives requests to attach executable
//! programs to new tasks.
//! Other tasks can send IPC messages in a specific format, telling the loader
//! which task to modify and which program to load. Most of the details of this
//! will be handled by the stdlib.

use idos_api::io::error::IOError;
use spin::Once;

use crate::io::handle::Handle;
use crate::loader::environment::ExecutionEnvironment;
use crate::loader::error::LoaderError;
use crate::task::actions::handle::{create_file_handle, create_kernel_task};
use crate::task::actions::io::{close_sync, open_sync, read_struct_sync, read_sync};
use crate::task::id::TaskID;
use crate::task::messaging::Message;
use crate::task::switching::get_task;

use super::request::RequestQueue;

pub static REQUEST_QUEUE: RequestQueue = RequestQueue::new();

fn loader_resident() -> ! {
    crate::kprintln!("Loader task ready to receive");
    loop {
        let incoming_request = REQUEST_QUEUE.wait_on_request();
        crate::kprintln!("Loader Request - Load \"{}\"", incoming_request.path);

        let (file_handle, mut env) = match load_file(&incoming_request.path) {
            Ok(handle) => handle,
            Err(e) => continue,
        };

        env.map_memory(incoming_request.task);
        env.fill_sections(file_handle);
        if env.require_vm {
            // if the environment requires a VM, we need to load DOSLAYER.ELF
            // and create a secondary environment that we also load here
            crate::kprintln!("DOS VM: Open DOSLAYER");

            let (compat_handle, mut compat_env) = match load_file("C:\\DOSLAYER.ELF") {
                Ok(handle) => handle,
                Err(e) => {
                    crate::kprintln!("Could not find compat layer");
                    continue;
                }
            };

            compat_env.map_memory(incoming_request.task);
            compat_env.fill_sections(compat_handle);
            compat_env.fill_stack(incoming_request.task);
            compat_env.set_registers(incoming_request.task);

            let _ = close_sync(compat_handle);
        } else {
            env.fill_stack(incoming_request.task);
            env.set_registers(incoming_request.task);
        }

        {
            let task_lock = get_task(incoming_request.task).unwrap();
            let mut task = task_lock.write();
            task.set_filename(&incoming_request.path);
            task.make_runnable();
        }

        let _ = close_sync(file_handle);
    }
}

fn load_file(path: &str) -> Result<(Handle, ExecutionEnvironment), LoaderError> {
    let exec_handle = create_file_handle();
    let _ = open_sync(exec_handle, path).map_err(|e| match e {
        IOError::NotFound => LoaderError::FileNotFound,
        _ => LoaderError::InternalError,
    })?;

    let mut magic_number: [u8; 4] = [0; 4];
    let _ = read_sync(exec_handle, &mut magic_number, 0);
    let is_elf = magic_number == [0x7f, 0x45, 0x4c, 0x46];
    let is_mz = magic_number[..2] == [b'M', b'Z'] || magic_number[..2] == [b'Z', b'M'];

    let env = if is_elf {
        super::elf::build_environment(exec_handle)?
    } else if is_mz {
        return Err(LoaderError::UnsupportedFileFormat);
    } else if path.to_uppercase().ends_with(".COM") {
        super::com::build_environment(exec_handle)?
    } else {
        return Err(LoaderError::UnsupportedFileFormat);
    };

    crate::kprintln!("Loader Request - Executable loaded");
    for segment in &env.segments {
        crate::kprintln!(
            "Segment: {} {:?} - {:?} ({:#x})",
            if segment.can_write() { "W" } else { "R" },
            segment.get_starting_address(),
            segment.get_starting_address() + segment.size_in_bytes(),
            segment.size_in_bytes()
        );
    }

    Ok((exec_handle, env))
}

struct Loader {}

impl Loader {}

pub static LOADER_ID: Once<TaskID> = Once::new();

pub fn get_loader_id() -> TaskID {
    LOADER_ID
        .call_once(|| {
            let (_, task_id) = create_kernel_task(loader_resident, Some("LOADER"));
            // TODO: Register the task, or better yet execute it from within the registry

            task_id
        })
        .clone()
}
