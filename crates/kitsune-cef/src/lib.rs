pub mod error;

pub use error::CefError;

use std::collections::HashMap;
use std::num::NonZeroIsize;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use raw_window_handle::{HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use wry::{PageLoadEvent, Rect, WebView, WebViewBuilder, WebViewBuilderExtWindows};

/// A rectangle with a position and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CefRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub enum CefEvent {
    PageLoadStarted(String),
    PageLoadFinished(String),
    TitleChanged(String),
    IpcMessage(String),
    /// A page tried to open a new window; the URL to navigate to instead.
    NewWindowNav(String),
    /// A file download has started. `save_path` is where it will be written.
    DownloadStarted { url: String, filename: String, save_path: String },
    /// A download finished (or failed).
    DownloadCompleted { url: String, save_path: Option<String>, success: bool },
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
    /// The parent HWND that owns this WebView. Used to return keyboard focus
    /// to the egui event loop when the user interacts with native UI panels.
    parent_hwnd: isize,
}

impl CefBrowser {
    /// Create a new browser, child of `parent_hwnd`, filling the provided bounds.
    pub fn new(
        parent_hwnd: isize,
        url: &str,
        bounds: CefRect,
        event_tx: Option<Sender<CefEvent>>,
    ) -> Result<Self, CefError> {
        let parent = ParentWindow::new(parent_hwnd)?;
        let ipc_tx = event_tx.clone();
        let load_tx = event_tx.clone();
        let new_win_tx = event_tx.clone();
        let dl_start_tx = event_tx.clone();
        let dl_done_tx = event_tx;
        let webview = WebViewBuilder::new_as_child(&parent)
            .with_bounds(bounds.into())
            .with_url(url)
            .with_background_color((8, 8, 10, 255))  // matches BG_VOID so any sub-pixel gap is invisible
            .with_devtools(cfg!(debug_assertions))
            .with_browser_accelerator_keys(true)
            // Redirect window.open() and target="_blank" links back to the same tab so the
            // agent never loses control to an unreachable popup window.
            .with_initialization_script(NEW_WINDOW_REDIRECT_JS)
            .with_ipc_handler(move |request| {
                if let Some(tx) = &ipc_tx {
                    let _ = tx.send(CefEvent::IpcMessage(request.body().to_string()));
                }
            })
            .with_on_page_load_handler(move |event, url| {
                if let Some(tx) = &load_tx {
                    let _ = match event {
                        PageLoadEvent::Started => tx.send(CefEvent::PageLoadStarted(url)),
                        PageLoadEvent::Finished => tx.send(CefEvent::PageLoadFinished(url)),
                    };
                }
            })
            // Belt-and-suspenders: block any native new-window request that slips through
            // the JS override and redirect it as a same-tab navigation.
            .with_new_window_req_handler(move |url: String| {
                if let Some(tx) = &new_win_tx {
                    let _ = tx.send(CefEvent::NewWindowNav(url));
                }
                false // block the native popup
            })
            // ── Download handling ─────────────────────────────────────────────
            // Intercept the download start to capture the filename + save path,
            // then let WebView2 proceed with its default destination.
            .with_download_started_handler(move |url: String, path: &mut PathBuf| {
                // Derive a clean filename from the URL, stripping query strings.
                let raw_name = url
                    .rsplit('/')
                    .next()
                    .unwrap_or("download")
                    .split('?')
                    .next()
                    .unwrap_or("download");
                let filename = if raw_name.is_empty() {
                    "download".to_string()
                } else {
                    raw_name.to_string()
                };

                // Override the save path to the user's Downloads folder.
                let dl_dir = dirs::download_dir()
                    .unwrap_or_else(|| PathBuf::from("."));
                *path = dl_dir.join(&filename);
                let save_path = path.to_string_lossy().to_string();

                if let Some(tx) = &dl_start_tx {
                    let _ = tx.send(CefEvent::DownloadStarted { url, filename, save_path });
                }
                true // allow download
            })
            .with_download_completed_handler(move |url: String, path: Option<PathBuf>, success: bool| {
                let save_path = path.map(|p| p.to_string_lossy().to_string());
                if let Some(tx) = &dl_done_tx {
                    let _ = tx.send(CefEvent::DownloadCompleted { url, save_path, success });
                }
            })
            .build()
            .map_err(|e| CefError::Backend(format!("failed to create embedded webview: {e}")))?;

        Ok(Self { webview, parent_hwnd })
    }

    /// Navigate to URL.
    pub fn load_url(&self, url: &str) {
        let _ = self.webview.load_url(url);
    }

    pub fn navigate(&self, url: &str) {
        self.load_url(url);
    }

    pub fn url(&self) -> Result<String, CefError> {
        self.webview
            .url()
            .map_err(|e| CefError::Backend(format!("failed to query webview url: {e}")))
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

    pub fn execute_js_with_callback(
        &self,
        script: &str,
        callback: impl Fn(String) + Send + 'static,
    ) -> Result<(), CefError> {
        self.webview
            .evaluate_script_with_callback(script, callback)
            .map_err(|e| CefError::Backend(format!("failed to evaluate script with callback: {e}")))
    }

    /// Resize/reposition to match the current center panel.
    pub fn set_bounds(&self, rect: CefRect) {
        let _ = self.webview.set_bounds(rect.into());
    }

    /// Give keyboard focus to the WebView (when user clicks the page area).
    pub fn focus(&self) {
        let _ = self.webview.focus();
    }

    /// Return keyboard focus to the parent window (egui event loop).
    /// This is CRITICAL — without this, typing in egui TextEdits does nothing
    /// because the WebView2 child window intercepts all keyboard events.
    #[cfg(target_os = "windows")]
    pub fn unfocus(&self) {
        // SAFETY: parent_hwnd is a valid window handle owned by eframe.
        // SetFocus transfers keyboard input to the specified window.
        unsafe {
            SetFocus(self.parent_hwnd);
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn unfocus(&self) {
        // No-op on non-Windows platforms for now
    }

    /// Inject ResourceRequestHandler (privacy middleware).
    pub fn set_request_handler(&self, _handler: Box<dyn RequestHandler + Send + Sync>) {
        // TODO: Bridge request inspection through WebView2 events.
    }
}

// Win32 FFI for SetFocus
#[cfg(target_os = "windows")]
extern "system" {
    fn SetFocus(hWnd: isize) -> isize;
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

/// Injected into every page before any site scripts run.
/// Overrides `window.open` and strips `target="_blank"` on link clicks so that
/// the agent never loses control to a popup window it cannot observe.
const NEW_WINDOW_REDIRECT_JS: &str = r#"(function () {
    // Replace window.open with a same-tab redirect.
    window.open = function (url) {
        if (url && url !== 'about:blank') {
            window.location.href = url;
        }
        return { closed: false, focus: function(){}, close: function(){} };
    };
    // Intercept every click: if it hits an anchor with a non-self target,
    // strip the target so the browser navigates in-place.
    document.addEventListener('click', function (e) {
        var el = e.target;
        while (el && el.tagName !== 'A') { el = el.parentElement; }
        if (el && el.tagName === 'A') {
            var t = (el.getAttribute('target') || '').toLowerCase();
            if (t === '_blank' || t === '_new' || t === '_tab') {
                el.removeAttribute('target');
            }
        }
    }, true);
})();"#;

impl From<CefRect> for Rect {
    fn from(value: CefRect) -> Self {
        Self {
            x: value.x as i32,
            y: value.y as i32,
            width: value.width as u32,
            height: value.height as u32,
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
        assert!(matches!(
            ParentWindow::new(0),
            Err(CefError::BrowserCreation)
        ));
    }
}
