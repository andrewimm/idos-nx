use crate::log::TaggedLogger;

pub mod address;
pub mod heap;
pub mod physical;
pub mod shared;
pub mod virt;

const LOGGER: TaggedLogger = TaggedLogger::new("MEMORY", 31);
