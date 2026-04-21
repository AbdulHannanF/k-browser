pub mod app;
pub mod client;
pub mod error;
pub mod js;
pub mod request;
pub mod scheme;

pub use error::CefError;

/// A rectangle with a position and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CefRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A handle to a CEF browser instance.
pub struct CefBrowser { /* opaque */ }

impl CefBrowser {
    /// Create a new browser, child of parent_hwnd, filling bounds rect.
    pub fn new(_parent_hwnd: isize, _url: &str, _bounds: CefRect) -> Result<Self, CefError> {
        // TODO: Implement CEF browser creation
        Ok(Self {})
    }

    /// Navigate to URL.
    pub fn load_url(&self, _url: &str) {
        // TODO: Implement URL loading
    }

    /// Go back in history.
    pub fn go_back(&self) {
        // TODO: Implement go back
    }

    /// Go forward in history.
    pub fn go_forward(&self) {
        // TODO: Implement go forward
    }

    /// Reload current page.
    pub fn reload(&self) {
        // TODO: Implement reload
    }

    /// Stop loading.
    pub fn stop_load(&self) {
        // TODO: Implement stop load
    }

    /// Execute JavaScript in the main frame. Fire and forget.
    pub fn execute_js(&self, _script: &str) {
        // TODO: Implement JS execution
    }

    /// Resize/reposition to match egui central panel.
    pub fn set_bounds(&self, _rect: CefRect) {
        // TODO: Implement set bounds
    }

    /// Inject ResourceRequestHandler (privacy middleware).
    pub fn set_request_handler(&self, _handler: Box<dyn RequestHandler + Send + Sync>) {
        // TODO: Implement request handler injection
    }
}

/// Called by CEF for every outbound request.
pub trait RequestHandler: Send + Sync {
    fn on_before_request(&self, url: &str, method: &str, headers: &mut Headers) -> RequestAction;
}

pub type Headers = std::collections::HashMap<String, String>;

pub enum RequestAction {
    Allow,
    Block,
    Redirect(String),
}
