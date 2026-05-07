/// Tab management for KitsuneEngine.
use crate::navigation::NavigationHistory;
use serde::{Deserialize, Serialize};

/// A browser tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    /// Tab ID.
    pub id: usize,
    /// Tab title.
    pub title: String,
    /// Current URL.
    pub url: Option<String>,
    /// Loading state.
    pub state: TabState,
    /// Whether the tab is actively fetching or parsing content (spinning dot state).
    pub is_loading: bool,
    /// Whether this is the active tab.
    pub active: bool,
    /// Color block substitute for favicon rendering.
    pub favicon_color: Option<[u8; 3]>,
    /// Fingerprint exposure score for this tab's origin.
    pub fingerprint_score: f32,
    /// Navigation history.
    #[serde(skip)]
    pub history: NavigationHistory,
}

/// Tab loading state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabState {
    /// Blank/new tab.
    Blank,
    /// Loading page.
    Loading,
    /// Page loaded.
    Loaded,
    /// Error loading page.
    Error,
}

impl Tab {
    /// Create a new blank tab.
    pub fn new(id: usize, title: String) -> Self {
        Self {
            id,
            title,
            url: None,
            state: TabState::Blank,
            is_loading: false,
            active: true,
            favicon_color: None,
            fingerprint_score: 0.0,
            history: NavigationHistory::new(),
        }
    }

    /// Navigate to a URL.
    pub fn navigate(&mut self, url: &str) {
        self.url = Some(url.to_string());
        self.state = TabState::Loading;
        self.is_loading = true;

        let hash = url.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        self.favicon_color = Some([
            ((hash >> 16) & 0xFF) as u8,
            ((hash >> 8) & 0xFF) as u8,
            (hash & 0xFF) as u8,
        ]);
    }

    /// Mark as loaded.
    pub fn loaded(&mut self, title: String) {
        self.title = title;
        self.state = TabState::Loaded;
        self.is_loading = false;
    }

    /// Mark as error.
    pub fn error(&mut self) {
        self.state = TabState::Error;
        self.is_loading = false;
    }
}
