// ARCHITECTURE: kitsune-html provides the HTML5 parser for KitsuneEngine.
// It wraps html5ever to provide a stable, safe interface for parsing HTML
// into a DOM tree that the layout and rendering engines can consume.

pub mod parser;
pub mod dom;
pub mod error;

pub use error::{HtmlError, HtmlResult};
pub use parser::*;
pub use dom::*;
