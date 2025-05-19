pub mod elf;
pub mod environment;
pub mod error;
pub mod parse;
pub mod relocation;
pub mod request;
pub mod resident;

use crate::task::id::TaskID;

use self::environment::ExecutionEnvironment;
use self::error::LoaderError;

pub fn load_executable(
    address_space: TaskID,
    path_str: &str,
) -> Result<ExecutionEnvironment, LoaderError> {
    self::resident::REQUEST_QUEUE.add_request(address_space, path_str);
    Err(LoaderError::InternalError)
}
