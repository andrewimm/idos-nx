#[derive(Debug)]
pub enum LoaderError {
    FileNotFound,
    UnsupportedFileFormat,
    SectionOutOfBounds,
    InternalError,
}
