//! Large decorative text rendering using Unicode box-drawing glyphs.
//!
//! A Rust port of Python's [rich-typography](https://github.com/mtkalms/rich-typography)
//! by mtkalms, built on top of [rich-rs](https://github.com/mrsaraiva/rich-rs).
//!
//! # Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use rich_typography_rs::{Font, Typography};
//!
//! let font = Arc::new(Font::builtin("condensedsemi").unwrap().clone());
//! let t = Typography::new("Hello!", Default::default(), font, 0, true, true, None);
//!
//! let mut console = rich_rs::Console::new();
//! console.print(&t, None, None, None, false, "\n").unwrap();
//! ```

mod glyph;
mod line;
mod font;
mod typography;

pub use glyph::{Glyph, Glyphs};
pub use line::{LineStyle, LineStyleOverride, LineType};
pub use font::{Font, FontError};
pub use typography::{LigatureStyleMethod, Typography};

// Re-export commonly used rich-rs types so users don't need to depend on it directly.
pub use rich_rs::{
    Console, ConsoleOptions, JustifyMethod, Measurement, OverflowMethod, Renderable, Segment,
    Segments, Span, Style, Text,
};
