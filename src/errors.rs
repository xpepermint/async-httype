#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    StreamNotReadable,
    StreamNotWritable,
    SizeLimitExceeded(usize),
    InvalidData,
    InvalidHeader(String),
    MissingHeader(String),
}
