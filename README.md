# rich-typography-rs

[![Crates.io](https://img.shields.io/crates/v/rich-typography-rs.svg)](https://crates.io/crates/rich-typography-rs)
[![Documentation](https://docs.rs/rich-typography-rs/badge.svg)](https://docs.rs/rich-typography-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Large decorative text rendering for the terminal using Unicode box-drawing glyphs, a Rust
port of Python's [rich-typography](https://github.com/mtkalms/rich-typography), built on
[rich-rs](https://github.com/mrsaraiva/rich-rs).

> **Attribution.** rich-typography-rs is a derivative work: a Rust port of
> [rich-typography](https://github.com/mtkalms/rich-typography) by mtkalms. All credit for
> the original design and the bundled fonts goes to the upstream author.

## Compatibility

Works on Linux, macOS, and Windows.

**Minimum Supported Rust Version:** 1.85+ (inherited from rich-rs)

## Installing

```toml
[dependencies]
rich-typography-rs = "1.0"
```

## Quick Start

```rust,no_run
use std::sync::Arc;
use rich_typography_rs::{Font, Typography};

let font = Arc::new(Font::builtin("condensedsemi").unwrap().clone());
let t = Typography::new("Hello!", Default::default(), font, 0, true, true, None);

let mut console = rich_rs::Console::new();
console.print(&t, None, None, None, false, "\n").unwrap();
```

## Examples

```sh
cargo run --example hello
cargo run --example showcase
```

- `hello`: minimal "render a word" example
- `showcase`: bundled fonts and styling options

## License

Licensed under the [MIT License](LICENSE). Bundled fonts retain their upstream licensing.
