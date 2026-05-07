use thiserror::Error;

#[derive(Error, Debug)]
pub enum CefError {
    #[error("CEF initialization failed")]
    Initialization,
    #[error("Failed to create browser")]
    BrowserCreation,
    #[error("{0}")]
    Backend(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}
