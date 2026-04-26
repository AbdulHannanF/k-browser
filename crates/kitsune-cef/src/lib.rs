pub mod error;

pub use error::CefError;

use std::collections::HashMap;
use std::num::NonZeroIsize;

use raw_window_handle::{HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use wry::{Rect, WebView, WebViewBuilder};

#[cfg(target_os = "windows")]
use wry::WebViewBuilderExtWindows;

/// A rectangle with a position and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CefRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Initialize the embedded browser backend.
///
/// The legacy crate name remains for compatibility, but the implementation now
/// uses the platform WebView2 host through `wry`.
pub fn initialize() -> Result<(), CefError> {
    #[cfg(target_os = "windows")]
    {
        let _ = wry::webview_version()
            .map_err(|e| CefError::Backend(format!("WebView2 runtime unavailable: {e}")))?;
    }

    Ok(())
}

/// A handle to a browser instance embedded as a child window.
pub struct CefBrowser {
    webview: WebView,
}

impl CefBrowser {
    /// Create a new browser, child of `parent_hwnd`, filling the provided bounds.
    pub fn new(parent_hwnd: isize, url: &str, bounds: CefRect) -> Result<Self, CefError> {
        let parent = ParentWindow::new(parent_hwnd)?;
        let webview = WebViewBuilder::new_as_child(&parent)
            .with_bounds(bounds.into())
            .with_url(url)
            .with_background_color((19, 20, 24, 255))
            .with_devtools(cfg!(debug_assertions))
            .with_browser_accelerator_keys(true)
            .build()
            .map_err(|e| CefError::Backend(format!("failed to create embedded webview: {e}")))?;

        Ok(Self { webview })
    }

    /// Navigate to URL.
    pub fn load_url(&self, url: &str) {
        let _ = self.webview.load_url(url);
    }

    pub fn navigate(&self, url: &str) {
        self.load_url(url);
    }

    /// Go back in history.
    pub fn go_back(&self) {
        let _ = self.webview.evaluate_script("history.back();");
    }

    /// Go forward in history.
    pub fn go_forward(&self) {
        let _ = self.webview.evaluate_script("history.forward();");
    }

    /// Reload current page.
    pub fn reload(&self) {
        let _ = self.webview.evaluate_script("window.location.reload();");
    }

    /// Stop loading.
    pub fn stop_load(&self) {
        let _ = self.webview.evaluate_script("window.stop();");
    }

    /// Execute JavaScript in the main page context. Fire and forget.
    pub fn execute_js(&self, script: &str) {
        let _ = self.webview.evaluate_script(script);
    }

    /// Resize/reposition to match the current center panel.
    pub fn set_bounds(&self, rect: CefRect) {
        let _ = self.webview.set_bounds(rect.into());
    }

    /// Inject ResourceRequestHandler (privacy middleware).
    pub fn set_request_handler(&self, _handler: Box<dyn RequestHandler + Send + Sync>) {
        // TODO: Bridge request inspection through WebView2 events.
    }
}

/// Called by the browser backend for every outbound request.
pub trait RequestHandler: Send + Sync {
    fn on_before_request(&self, url: &str, method: &str, headers: &mut Headers) -> RequestAction;
}

pub type Headers = HashMap<String, String>;

pub enum RequestAction {
    Allow,
    Block,
    Redirect(String),
}

impl From<CefRect> for Rect {
    fn from(value: CefRect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

struct ParentWindow {
    hwnd: NonZeroIsize,
}

impl ParentWindow {
    fn new(hwnd: isize) -> Result<Self, CefError> {
        let hwnd = NonZeroIsize::new(hwnd).ok_or(CefError::BrowserCreation)?;
        Ok(Self { hwnd })
    }
}

impl HasWindowHandle for ParentWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let mut handle = Win32WindowHandle::new(self.hwnd);
        handle.hinstance = None;
        let raw = RawWindowHandle::Win32(handle);

        // SAFETY: The raw handle points to a live native window owned by eframe for the
        // lifetime of the embedded webview.
        unsafe { Ok(WindowHandle::borrow_raw(raw)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_conversion_preserves_coordinates() {
        let rect = CefRect {
            x: 12,
            y: 24,
            width: 640,
            height: 480,
        };

        let converted: Rect = rect.into();
        assert_eq!(converted.x, 12);
        assert_eq!(converted.y, 24);
        assert_eq!(converted.width, 640);
        assert_eq!(converted.height, 480);
    }

    #[test]
    fn parent_window_rejects_zero_handle() {
        assert!(matches!(ParentWindow::new(0), Err(CefError::BrowserCreation)));
    }
}
