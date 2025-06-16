pub mod com;
pub mod elf;
pub mod environment;
pub mod error;
pub mod parse;
pub mod relocation;
pub mod request;
pub mod resident;

use crate::log::TaggedLogger;
use crate::task::id::TaskID;

use self::environment::ExecutionEnvironment;
use self::error::LoaderError;

const LOGGER: TaggedLogger = TaggedLogger::new("LOADER", 33);

pub fn load_executable(address_space: TaskID, path_str: &str) -> Result<(), LoaderError> {
    self::resident::REQUEST_QUEUE.add_request(address_space, path_str);
    Ok(())
}
