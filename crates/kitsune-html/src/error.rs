use thiserror::Error;
pub type HtmlResult<T> = Result<T, HtmlError>;

#[derive(Debug, Error)]
pub enum HtmlError {
    #[error("HTML parse error: {0}")]
    ParseError(String),
    #[error("Invalid HTML: {0}")]
    InvalidHtml(String),
    #[error("DOM error: {0}")]
    DomError(String),
}
