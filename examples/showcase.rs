//! Showcase of rich-typography features — styles, fonts, and justification.
//!
//! Mirrors the Python `python -m rich_typography` demo.
//!
//! Run with: `cargo run --example showcase`

use std::sync::Arc;
use std::time::Instant;

use rich_rs::{Column, Renderable, Row, SimpleColor, Table};
use rich_typography_rs::{Console, Font, JustifyMethod, LigatureStyleMethod, Style, Text, Typography};

fn main() {
    let start = Instant::now();

    let font = Arc::new(Font::builtin("condensedsemi").expect("builtin font not found").clone());
    let mut console = Console::new();

    // ── Styles ────────────────────────────────────────────────────────────────
    let styles = Typography::from_markup(
        "Most ansi styles: [bold]bold[/bold], [dim]dim[/dim], \
         [underline]underline[/underline], [strike]strikethrough[/strike], \
         [overline]overline[/overline], [reverse]reverse[/reverse], \
         and even [blink]blink[/blink].",
        Style::default(),
        Arc::clone(&font),
        0,
        true,
        true,
        None,
        None,
        None,
    )
    .expect("markup error");
    console.print(&styles, None, None, None, false, "\n").unwrap();

    // ── Justification intro ───────────────────────────────────────────────────
    let intro = Typography::from_markup(
        "Word wrap text. Justify [green]left[/], [yellow]center[/], \
         [blue]right[/] or [red]full[/].",
        Style::default(),
        Arc::clone(&font),
        0,
        true,
        true,
        None,
        None,
        None,
    )
    .expect("markup error");
    console.print(&intro, None, None, None, false, "\n").unwrap();

    // ── 2×2 justification grid ────────────────────────────────────────────────
    let lorem = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                 Quisque in metus sed sapien ultricies pretium a at justo. \
                 Maecenas luctus velit et auctor maximus.";

    let make = |justify: JustifyMethod, color: SimpleColor| -> Typography {
        let mut t = Typography::new(
            lorem,
            Style::new().with_color(color),
            Arc::clone(&font),
            0,
            true,
            true,
            None,
        );
        t.justify = Some(justify);
        t
    };

    let mut grid = Table::grid().with_padding(0, 3);
    grid.add_column(Column::new().ratio(1));
    grid.add_column(Column::new().ratio(1));

    let row1: Vec<Box<dyn Renderable + Send + Sync>> = vec![
        Box::new(make(JustifyMethod::Left,   SimpleColor::Standard(2))), // green
        Box::new(make(JustifyMethod::Center, SimpleColor::Standard(3))), // yellow
    ];
    let row2: Vec<Box<dyn Renderable + Send + Sync>> = vec![
        Box::new(make(JustifyMethod::Right,  SimpleColor::Standard(4))), // blue
        Box::new(make(JustifyMethod::Full,   SimpleColor::Standard(1))), // red
    ];
    grid.add_row(Row::new(row1));
    grid.add_row(Row::new(row2));
    console.print(&grid, None, None, None, false, "\n").unwrap();

    // ── Fonts ─────────────────────────────────────────────────────────────────
    let font_names = [
        ("condensedsans",  "condensedsans"),
        ("condensedsemi",  "condensedsemi  (default)"),
        ("condensedserif", "condensedserif"),
    ];
    for (key, label) in font_names {
        let f = Arc::new(Font::builtin(key).expect("builtin font not found").clone());
        let t = Typography::new(label, Style::default(), f, 0, true, true, None);
        console.print(&t, None, None, None, false, "\n").unwrap();
    }

    // ── Ligatures ─────────────────────────────────────────────────────────────
    let with_lig = Typography::new(
        "fi fl ff ffi ffl",
        Style::default(),
        Arc::clone(&font),
        0,
        true,
        true,
        Some(LigatureStyleMethod::First),
    );
    let without_lig = Typography::new(
        "fi fl ff ffi ffl",
        Style::default(),
        Arc::clone(&font),
        0,
        true,
        false, // ligatures off
        None,
    );

    let mut lig_table = Table::grid().with_padding(0, 4);
    lig_table.add_column(Column::new());
    lig_table.add_column(Column::new());

    let header_row: Vec<Box<dyn Renderable + Send + Sync>> = vec![
        Box::new(Text::plain("with ligatures")),
        Box::new(Text::plain("without ligatures")),
    ];
    let glyph_row: Vec<Box<dyn Renderable + Send + Sync>> = vec![
        Box::new(with_lig),
        Box::new(without_lig),
    ];
    lig_table.add_row(Row::new(header_row));
    lig_table.add_row(Row::new(glyph_row));
    console.print(&lig_table, None, None, None, false, "\n").unwrap();

    // ── Timing ────────────────────────────────────────────────────────────────
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let timing = Text::from_markup(
        &format!("[dim]rendered in [not dim]{elapsed_ms:.1}ms[/]"),
        false,
    )
    .unwrap();
    console.print(&timing, None, None, None, false, "\n").unwrap();
}
