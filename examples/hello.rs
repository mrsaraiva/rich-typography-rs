//! Basic hello-world example from the rich-typography README.
//!
//! Run with: `cargo run --example hello`

use std::sync::Arc;

use rich_typography_rs::{Console, Font, JustifyMethod, Style, Typography};

fn main() {
    let font = Arc::new(Font::builtin("condensedsemi").expect("builtin font not found").clone());

    let t = Typography::from_markup(
        "Hello from [purple]rich-typography[/purple]",
        Style::default(),
        font,
        0,
        true,
        true,
        None,
        Some(JustifyMethod::Center),
        None,
    )
    .expect("markup parse error");

    let mut console = Console::new();
    console.print(&t, None, None, None, false, "\n").unwrap();
}
