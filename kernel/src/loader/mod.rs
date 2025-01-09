pub mod environment;
pub mod parse;
pub mod relocation;
pub mod task;

use self::environment::ExecutionEnvironment;

#[derive(Debug)]
pub enum LoaderError {
    FileNotFound,
    InternalError,
}

pub fn load_executable(_path_str: &str) -> Result<ExecutionEnvironment, LoaderError> {
    Err(LoaderError::InternalError)
}
