pub mod bin;
pub mod com;
pub mod elf;
pub mod environment;
pub mod mz;
pub mod parse;
pub mod relocation;

use crate::files::path::Path;
use crate::filesystem::get_driver_by_id;
use crate::task::actions::io::prepare_open_file;
use crate::task::files::OpenFile;
use self::environment::ExecutionEnvironment;

#[derive(Debug)]
pub enum LoaderError {
    FileNotFound,
    InternalError,
}

pub fn load_executable(path_str: &str) -> Result<(OpenFile, ExecutionEnvironment), LoaderError> {
    let exec_file = prepare_open_file(path_str).map_err(|_| LoaderError::FileNotFound)?;
    
    let mut magic_number: [u8; 4] = [0; 4];
    let driver = get_driver_by_id(exec_file.drive)
        .map_err(|_| LoaderError::FileNotFound)?
        .read(exec_file.driver_handle, &mut magic_number)
        .map_err(|_| LoaderError::InternalError)?;

    let is_elf = magic_number == [0x7f, 0x45, 0x4c, 0x46];
    let is_mz = magic_number[0..2] == [b'M', b'Z'] || magic_number[0..2] == [b'Z', b'M'];

    let env = if is_elf {
        self::elf::build_environment(exec_file.drive, exec_file.driver_handle)?
    } else if is_mz {
        self::mz::build_environment(exec_file.drive, exec_file.driver_handle)?
    } else {
        let extension = Path::get_extension(path_str);
        match extension {
            Some("COM") => self::com::build_environment(exec_file.drive, exec_file.driver_handle)?,
            _ => self::bin::build_environment(exec_file.drive, exec_file.driver_handle)?,
        }
    };
    
    Ok((exec_file, env))
}
