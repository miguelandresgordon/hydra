use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VdfError {
    #[error("unexpected end of data at byte {offset}")]
    UnexpectedEof { offset: usize },
    #[error("unknown value type 0x{tag:02x} at byte {offset}")]
    UnknownType { offset: usize, tag: u8 },
    #[error("invalid UTF-8 at byte {offset}")]
    InvalidUtf8 { offset: usize },
    #[error("trailing bytes after root map at byte {offset}")]
    TrailingBytes { offset: usize },
    #[error("nesting deeper than {max} levels at byte {offset}")]
    TooDeep { offset: usize, max: usize },
    #[error("string contains a NUL byte and cannot be written (key or value {context:?})")]
    EmbeddedNul { context: String },
    #[error("text VDF syntax error at byte {offset}: {message}")]
    Syntax { offset: usize, message: &'static str },
    #[error("shortcuts root map not found")]
    NoShortcuts,
}
