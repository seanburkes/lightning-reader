use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDF parse error: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("PDF requires a password or is encrypted")]
    Encrypted,
    #[error("PDF is empty")]
    Empty,
    #[error("Requested page {0} is out of bounds")]
    InvalidPage(usize),
    #[error("PDF parse error (pdf-rs): {0}")]
    PdfRs(String),
}
