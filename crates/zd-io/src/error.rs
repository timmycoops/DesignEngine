use std::fmt;

#[derive(Debug)]
pub enum IoError {
    Io(std::io::Error),
    /// The file is readable but its content is wrong; `line` is 1-based.
    Format { line: usize, message: String },
    Unsupported(String),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::Io(e) => write!(f, "{e}"),
            IoError::Format { line, message } => write!(f, "line {line}: {message}"),
            IoError::Unsupported(message) => write!(f, "unsupported: {message}"),
        }
    }
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IoError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        IoError::Io(e)
    }
}
