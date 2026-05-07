//! Navigation history management for browser tabs.

use url::Url;

/// A single entry in the navigation history.
#[derive(Debug, Clone)]
pub struct NavigationEntry {
    pub url: Url,
    pub title: String,
    pub scroll_offset: (f32, f32),
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Navigation history for a single tab.
#[derive(Debug, Clone)]
pub struct NavigationHistory {
    entries: Vec<NavigationEntry>,
    current: usize,
}

impl NavigationHistory {
    /// Create a new, empty navigation history.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            current: 0,
        }
    }

    /// Push a new entry to the history.
    pub fn push(&mut self, url: Url, title: String) {
        // If we are not at the end of the history, truncate it.
        if !self.entries.is_empty() && self.current < self.entries.len() - 1 {
            self.entries.truncate(self.current + 1);
        }
        self.entries.push(NavigationEntry {
            url,
            title,
            scroll_offset: (0.0, 0.0),
            timestamp: chrono::Utc::now(),
        });
        self.current = self.entries.len() - 1;
    }

    /// Go back in history.
    pub fn back(&mut self) -> Option<&NavigationEntry> {
        if self.can_go_back() {
            self.current -= 1;
            self.current()
        } else {
            None
        }
    }

    /// Go forward in history.
    pub fn forward(&mut self) -> Option<&NavigationEntry> {
        if self.can_go_forward() {
            self.current += 1;
            self.current()
        } else {
            None
        }
    }

    /// Get the current history entry.
    pub fn current(&self) -> Option<&NavigationEntry> {
        self.entries.get(self.current)
    }

    /// Check if we can go back.
    pub fn can_go_back(&self) -> bool {
        self.current > 0
    }

    /// Check if we can go forward.
    pub fn can_go_forward(&self) -> bool {
        !self.entries.is_empty() && self.current < self.entries.len() - 1
    }

    /// Push a new state to the history without reloading.
    pub fn push_state(&mut self, url: Url, title: String) {
        // This is similar to push, but it doesn't trigger a page load.
        self.push(url, title);
    }
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::NavigationHistory;

    #[test]
    fn empty_history_cannot_go_forward() {
        let history = NavigationHistory::new();
        assert!(!history.can_go_forward());
    }
}
