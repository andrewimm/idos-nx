pub mod bin;
pub mod com;
pub mod environment;

use crate::files::path::Path;
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
    
    // TODO: read beginning of file to detect ELF or MZ executable

    let extension = Path::get_extension(path_str);
    let env = match extension {
        Some("COM") => self::com::build_environment(exec_file.drive, exec_file.driver_handle)?,
        _ => self::bin::build_environment(exec_file.drive, exec_file.driver_handle)?,
    };
    
    Ok((exec_file, env))
}
