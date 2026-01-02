use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Parse error: {0}")]
    Parse(String),
}
