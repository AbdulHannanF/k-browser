//! DOM observation script — produces the structured page snapshot the LLM sees.
//!
//! The script tags every interactive element with a `data-kitsune-id` attribute
//! starting from 0 and returns a JSON blob describing the page. The runtime
//! references elements by `id` thereafter, so the LLM never has to guess at
//! CSS selectors for clicks/fills.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObservedElement {
    pub id: usize,
    pub tag: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub autocomplete: String,
    /// For `<a>` elements: the href value (helps the agent understand where a link goes).
    #[serde(default)]
    pub href: String,
    /// Accessibility label (`aria-label` or `title`) when the visible text is absent or ambiguous.
    #[serde(default)]
    pub aria_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObservedPage {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub elements: Vec<ObservedElement>,
    #[serde(default)]
    pub text_preview: String,
}

/// Returns a self-contained JS snippet that:
/// 1. Tags every interactive element with `data-kitsune-id` starting from 0.
/// 2. Calls `window.__kitsune_ipc(JSON.stringify({ url, title, elements, text_preview }))`.
///
/// Runtime callers wrap this with `EvalJsWithCallback` and parse the resulting
/// JSON into [`ObservedPage`].
pub fn observation_script() -> String {
    r#"
(function() {
    try {
        const sel = 'a, button, input, select, textarea';
        const nodes = Array.from(document.querySelectorAll(sel));
        const elements = [];
        let i = 0;
        for (const el of nodes) {
            // Skip hidden / disabled elements; they're noise for the agent.
            const style = window.getComputedStyle(el);
            if (style && (style.display === 'none' || style.visibility === 'hidden')) continue;
            if (el.disabled) continue;
            el.setAttribute('data-kitsune-id', String(i));
            const txt = (el.innerText || el.value || '').toString().slice(0, 100).trim();
            elements.push({
                id: i,
                tag: (el.tagName || '').toLowerCase(),
                type: (el.getAttribute('type') || '').toLowerCase(),
                name: el.getAttribute('name') || '',
                placeholder: el.getAttribute('placeholder') || '',
                text: txt,
                autocomplete: (el.getAttribute('autocomplete') || '').toLowerCase(),
                href: (el.tagName === 'A' ? (el.getAttribute('href') || '') : ''),
                aria_label: (el.getAttribute('aria-label') || el.getAttribute('title') || '').slice(0, 80)
            });
            i += 1;
            if (i >= 80) break; // hard cap so we never blow up the prompt
        }
        const body = document.body ? document.body.innerText || '' : '';
        const payload = {
            url: location.href,
            title: document.title || '',
            elements: elements,
            text_preview: body.slice(0, 1500)
        };
        return JSON.stringify(payload);
    } catch (e) {
        return JSON.stringify({ error: String(e) });
    }
})();
"#
    .to_string()
}
