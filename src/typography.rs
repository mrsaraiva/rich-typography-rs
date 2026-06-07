use std::collections::BTreeMap;
use std::sync::Arc;

use regex::Regex;

use rich_rs::{
    Console, ConsoleOptions, JustifyMethod, Measurement, OverflowMethod, Renderable, Segment,
    Segments, Span, Style, Text,
};
use rich_rs::strip_control_codes;

use crate::font::Font;
use crate::glyph::Glyphs;
use crate::line::LineType;

// ============================================================================
// Constants
// ============================================================================

const NON_OVERLAPPING: &str = " \"'";

/// Style that resets all line decorations to off.
const LINE_STYLE_RESET: Style = Style {
    underline: Some(false),
    underline2: Some(false),
    overline: Some(false),
    strike: Some(false),
    ..rich_rs::NULL_STYLE
};

// ============================================================================
// LigatureStyleMethod
// ============================================================================

/// How to apply styles that fall inside a ligature glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LigatureStyleMethod {
    /// Extend the style from the first character of the ligature.
    First,
    /// Extend the style from the last character of the ligature.
    Last,
}

// ============================================================================
// MutableSpan (private)
// ============================================================================

#[derive(Debug, Clone)]
struct MutableSpan {
    start: usize,
    end: usize,
    style: Option<Style>,
}

impl MutableSpan {
    fn new(start: usize, end: usize, style: Option<Style>) -> Self {
        MutableSpan { start, end, style }
    }

    /// Resolve overlapping spans: later spans take precedence over earlier ones.
    /// Earlier spans have their end adjusted to the start of the later span.
    fn resolve(spans: Vec<MutableSpan>) -> Vec<MutableSpan> {
        let mut last: Option<&MutableSpan> = None;
        let mut result: Vec<MutableSpan> = Vec::new();

        for span in spans.iter().rev() {
            if span.start > span.end {
                continue;
            }
            match last {
                None => {
                    result.push(span.clone());
                    last = result.last();
                }
                Some(prev) if span.start < prev.start => {
                    let mut adjusted = span.clone();
                    adjusted.end = prev.start;
                    result.push(adjusted);
                    last = result.last();
                }
                _ => {}
            }
        }

        result.reverse();
        result
    }
}

// ============================================================================
// Typography
// ============================================================================

/// Large decorative text rendered using Unicode box-drawing glyphs.
///
/// Implements [`rich_rs::Renderable`] and can be used anywhere Rich accepts a
/// renderable (console output, Panel, Table, etc.).
pub struct Typography {
    text: String,
    style: Style,
    spans: Vec<Span>,
    pub justify: Option<JustifyMethod>,
    pub overflow: Option<OverflowMethod>,
    pub no_wrap: Option<bool>,
    pub tab_size: Option<usize>,
    pub font: Arc<Font>,
    pub adjust_spacing: i32,
    pub use_kerning: bool,
    pub use_ligatures: bool,
    pub style_ligatures: Option<LigatureStyleMethod>,
}

impl Typography {
    /// Create a new `Typography` instance.
    pub fn new(
        text: impl Into<String>,
        style: Style,
        font: Arc<Font>,
        adjust_spacing: i32,
        use_kerning: bool,
        use_ligatures: bool,
        style_ligatures: Option<LigatureStyleMethod>,
    ) -> Self {
        let text = strip_control_codes(&text.into());
        Typography {
            text,
            style,
            spans: Vec::new(),
            justify: None,
            overflow: None,
            no_wrap: None,
            tab_size: None,
            font,
            adjust_spacing,
            use_kerning,
            use_ligatures,
            style_ligatures,
        }
    }

    /// Create from a `rich_rs::Text` object.
    pub fn from_text(
        text: &Text,
        font: Arc<Font>,
        adjust_spacing: i32,
        use_kerning: bool,
        use_ligatures: bool,
        style_ligatures: Option<LigatureStyleMethod>,
    ) -> Self {
        let plain = strip_control_codes(text.plain_text());
        let base_style = text.base_style().unwrap_or_default();
        let spans = text.spans().to_vec();
        Typography {
            text: plain,
            style: base_style,
            spans,
            justify: None,
            overflow: None,
            no_wrap: None,
            tab_size: None,
            font,
            adjust_spacing,
            use_kerning,
            use_ligatures,
            style_ligatures,
        }
    }

    /// Create from Rich markup (e.g. `"[bold red]Hello[/] World"`).
    pub fn from_markup(
        markup: &str,
        style: Style,
        font: Arc<Font>,
        adjust_spacing: i32,
        use_kerning: bool,
        use_ligatures: bool,
        style_ligatures: Option<LigatureStyleMethod>,
        justify: Option<JustifyMethod>,
        overflow: Option<OverflowMethod>,
    ) -> Result<Self, rich_rs::ParseError> {
        let text = Text::from_markup(markup, true)?;
        let mut result = Self::from_text(
            &text,
            font,
            adjust_spacing,
            use_kerning,
            use_ligatures,
            style_ligatures,
        );
        if !style.is_null() {
            result.style = result.style.combine(&style);
        }
        result.justify = justify;
        result.overflow = overflow;
        Ok(result)
    }

    /// Convert back to a `rich_rs::Text` object.
    pub fn to_text(&self) -> Text {
        let mut t = Text::styled(self.text.clone(), self.style);
        for span in &self.spans {
            t.spans_mut().push(span.clone());
        }
        t
    }

    /// Return a deep copy of this `Typography`.
    pub fn copy(&self) -> Self {
        Typography {
            text: self.text.clone(),
            style: self.style,
            spans: self.spans.clone(),
            justify: self.justify,
            overflow: self.overflow,
            no_wrap: self.no_wrap,
            tab_size: self.tab_size,
            font: Arc::clone(&self.font),
            adjust_spacing: self.adjust_spacing,
            use_kerning: self.use_kerning,
            use_ligatures: self.use_ligatures,
            style_ligatures: self.style_ligatures,
        }
    }

    // ========================================================================
    // Text accessors
    // ========================================================================

    pub fn plain(&self) -> &str {
        &self.text
    }

    pub fn set_plain(&mut self, new_text: &str) {
        let sanitized = strip_control_codes(new_text);
        let old_len = self.text.chars().count();
        let new_len = sanitized.chars().count();
        self.text = sanitized;
        if old_len > new_len {
            self.spans.retain(|span| span.start < new_len);
            for span in &mut self.spans {
                if span.end > new_len {
                    span.end = new_len;
                }
            }
        }
    }

    // ========================================================================
    // Core algorithms
    // ========================================================================

    /// Compute the inter-glyph spacing between two adjacent chars/ligatures.
    pub fn letter_adjust(&self, left: &str, right: &str) -> i32 {
        let mut value = self.font.letter_spacing() + self.adjust_spacing;
        if self.use_kerning && left != " " && right != " " {
            value -= Glyphs::max_overlap(self.font.get(left), self.font.get(right)) as i32;
        }
        value
    }

    /// Width in terminal cells that `text` would render to with this font.
    pub fn rendered_width(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        let glyphs = self.split_glyphs(text);
        let keys: Vec<&str> = glyphs.iter().map(|(_, k)| k.as_str()).collect();
        if keys.is_empty() {
            return 0;
        }
        let first_width = self.font.get(keys[0]).first().map(|r| r.chars().count()).unwrap_or(0);
        let mut width = first_width;
        for pair in keys.windows(2) {
            let glyph_width = self.font.get(pair[1]).first().map(|r| r.chars().count()).unwrap_or(0);
            let adj = self.letter_adjust(pair[0], pair[1]);
            width = (width as i32 + glyph_width as i32 + adj).max(0) as usize;
        }
        width
    }

    /// Split `text` into `(source_char_index, glyph_key)` pairs, substituting ligatures.
    ///
    /// Returns a `Vec` ordered by character index.
    pub fn split_glyphs(&self, text: &str) -> Vec<(usize, String)> {
        if !self.use_ligatures || self.font.ligature_keys().is_empty() {
            return text.char_indices().map(|(i, c)| (i, c.to_string())).collect();
        }

        let ligatures = self.font.ligature_keys();
        let mut sorted_ligs: Vec<&str> = ligatures.iter().map(|s| *s).collect();
        sorted_ligs.sort_by(|a, b| b.len().cmp(&a.len())); // longest first

        let pattern = sorted_ligs.join("|");
        let re = build_regex(&pattern);

        let mut result: Vec<(usize, String)> = Vec::new();
        let mut last_byte = 0;

        // Convert byte positions to char positions for indexing
        let chars_vec: Vec<(usize, char)> = text.char_indices().collect();
        let byte_to_char_idx: std::collections::HashMap<usize, usize> = chars_vec
            .iter()
            .enumerate()
            .map(|(char_idx, (byte_idx, _))| (*byte_idx, char_idx))
            .collect();

        for m in re.find_iter(text) {
            // Add individual chars between last match and this match
            let before = &text[last_byte..m.start()];
            let before_char_start = byte_to_char_idx.get(&last_byte).copied().unwrap_or(0);
            for (i, c) in before.chars().enumerate() {
                result.push((before_char_start + i, c.to_string()));
            }
            // Add the ligature
            let lig_char_start = byte_to_char_idx.get(&m.start()).copied().unwrap_or(0);
            result.push((lig_char_start, m.as_str().to_string()));
            last_byte = m.end();
        }

        // Add remaining chars after last match
        let remaining = &text[last_byte..];
        let remaining_char_start = if last_byte == 0 {
            0
        } else {
            byte_to_char_idx
                .get(&last_byte)
                .copied()
                .unwrap_or_else(|| chars_vec.len())
        };
        for (i, c) in remaining.chars().enumerate() {
            result.push((remaining_char_start + i, c.to_string()));
        }

        result
    }

    /// Truncate the text in-place to fit within `max_width` rendered cells.
    pub fn truncate(&mut self, max_width: usize, overflow: Option<OverflowMethod>) {
        let effective_overflow = overflow
            .or(self.overflow)
            .unwrap_or(OverflowMethod::Fold);

        let length = self.rendered_width(&self.text.clone());
        if matches!(effective_overflow, OverflowMethod::Ignore) || length <= max_width {
            return;
        }

        if matches!(effective_overflow, OverflowMethod::Ellipsis) {
            let ellipsis = if self.font.contains("…") { "…" } else { "..." };
            let ellipsis_width = self.rendered_width(ellipsis);
            let target = max_width.saturating_sub(ellipsis_width);
            let truncated = self.shrink_to_width(&self.text.clone(), target);
            self.set_plain(&(truncated + ellipsis));
        } else {
            let truncated = self.shrink_to_width(&self.text.clone(), max_width);
            self.set_plain(&truncated);
        }
    }

    fn shrink_to_width(&self, text: &str, max_width: usize) -> String {
        let mut t: String = text.to_string();
        while self.rendered_width(&t) > max_width && !t.is_empty() {
            let mut chars = t.chars();
            chars.next_back();
            t = chars.as_str().to_string();
        }
        t
    }

    /// Word-wrap this Typography into a Vec of per-line Typographies.
    pub fn wrap(
        &self,
        width: usize,
        overflow: Option<OverflowMethod>,
        tab_size: Option<usize>,
        no_wrap: Option<bool>,
    ) -> Vec<Typography> {
        let wrap_overflow = overflow
            .or(self.overflow)
            .unwrap_or(OverflowMethod::Fold);
        let effective_no_wrap =
            no_wrap.unwrap_or(false) || self.no_wrap.unwrap_or(false)
                || matches!(wrap_overflow, OverflowMethod::Ignore);
        let tab_sz = tab_size.or(self.tab_size).unwrap_or(8);

        let text = self.to_text();

        // Split on newlines
        let newline_lines: Vec<Text> = text.split("\n", false, true);

        let mut all_lines: Vec<Text> = Vec::new();
        for mut line in newline_lines {
            // Expand tabs
            if line.plain_text().contains('\t') {
                line = line.expand_tabs(tab_sz);
            }

            if effective_no_wrap {
                all_lines.push(line);
            } else {
                let offsets = self.divide_offsets(
                    line.plain_text(),
                    width,
                    matches!(wrap_overflow, OverflowMethod::Fold),
                );
                let divided = line.divide(offsets);
                all_lines.extend(divided);
            }
        }

        let mut typography_lines: Vec<Typography> = all_lines
            .iter()
            .map(|line| {
                Typography::from_text(
                    line,
                    Arc::clone(&self.font),
                    self.adjust_spacing,
                    self.use_kerning,
                    self.use_ligatures,
                    self.style_ligatures,
                )
            })
            .collect();

        for line in &mut typography_lines {
            line.truncate(width, overflow);
        }

        typography_lines
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    /// Render this (single-line) Typography to segments.
    pub fn render_line(
        &self,
        console: &Console,
        width: usize,
        justify: JustifyMethod,
    ) -> Vec<Segment> {
        let line_height = self.font.line_height();

        // For each newline-separated line in the text
        let mut all_segments: Vec<Segment> = Vec::new();

        for line_text in self.text.lines() {
            let line_text = match justify {
                JustifyMethod::Right | JustifyMethod::Center | JustifyMethod::Full => {
                    line_text.trim_end()
                }
                _ => line_text,
            };

            // Get style fragments aligned to glyph boundaries
            let mut fragments = self.style_fragments(line_text, console);

            // Compute line width and indent
            let line_width = self.rendered_width(line_text);
            let indent = match justify {
                JustifyMethod::Right => (width as i32 - line_width as i32).max(0) as usize,
                JustifyMethod::Center => {
                    ((width as i32 - line_width as i32).max(0) as usize) / 2
                }
                _ => 0,
            };

            // Full justification
            if matches!(justify, JustifyMethod::Full) {
                fragments = self.justify_full(width, line_text, fragments);
            }

            // Initialize row accumulators (one entry per font row)
            let mut row_chars: Vec<String> =
                vec![" ".repeat(indent); line_height];
            let indent_len = indent; // character count of indent
            let mut row_spans: Vec<Vec<MutableSpan>> = (0..line_height)
                .map(|_| vec![MutableSpan::new(0, indent_len, None)])
                .collect();

            let mut last_char: String = String::new();
            let mut last_style: Option<Style> = None;

            for (fragment_text, fragment_style) in &fragments {
                if fragment_text.is_empty() {
                    continue;
                }

                // Collect glyph keys for this fragment
                let segment_chars: Vec<String> = if self.use_ligatures {
                    self.split_glyphs(fragment_text)
                        .into_iter()
                        .map(|(_, k)| k)
                        .collect()
                } else {
                    fragment_text.chars().map(|c| c.to_string()).collect()
                };

                // Render fragment: merge all its glyphs together
                let mut fragment_rows: Vec<String> = Vec::new();
                let mut fragment_char: Option<String> = None;

                for char_key in &segment_chars {
                    let letter = self.font.get(char_key);
                    if fragment_char.is_none() {
                        fragment_rows = letter.clone();
                    } else {
                        let fc = fragment_char.as_ref().unwrap();
                        let mut spacing =
                            self.font.letter_spacing() + self.adjust_spacing;
                        if self.should_overlap(fc, char_key) {
                            spacing -= Glyphs::max_overlap(&fragment_rows, letter) as i32;
                        }
                        fragment_rows = Glyphs::merge(&fragment_rows, letter, spacing);
                    }
                    fragment_char = Some(char_key.clone());
                }

                if fragment_rows.is_empty() {
                    continue;
                }

                // Spacing between accumulated row and new fragment
                let first_frag_char = fragment_text.chars().next().map(|c| c.to_string());
                let mut fragment_spacing = self.font.letter_spacing() + self.adjust_spacing;

                let should_olap = match (&last_char[..], first_frag_char.as_deref()) {
                    (l, Some(r)) if !l.is_empty() => self.should_overlap(l, r),
                    _ => false,
                };
                if should_olap {
                    fragment_spacing -=
                        Glyphs::max_overlap(&row_chars, &fragment_rows) as i32;
                }
                last_char = fragment_char.unwrap_or_default();

                // Determine if background styles need to be split at the boundary
                let has_bg = |s: Option<Style>| {
                    s.and_then(|st| st.bgcolor).is_some()
                };
                let split_styles =
                    (has_bg(*fragment_style) || has_bg(last_style))
                        && fragment_spacing != 0;

                // Compute FG/BG boundary offsets
                let fg_offsets = Glyphs::boundary(&row_chars, &fragment_rows, fragment_spacing);
                let bg_offsets =
                    Glyphs::bg_boundary(&row_chars, &fragment_rows, fragment_spacing);

                for d in 0..line_height {
                    let row_char_len = row_chars[d].chars().count();

                    if split_styles {
                        // End the previous span at min(fg, bg) boundary
                        if let Some(last_span) = row_spans[d].last_mut() {
                            last_span.end =
                                (row_char_len as i32 + fg_offsets[d].min(bg_offsets[d])).max(0)
                                    as usize;
                        }

                        // Row overlaps segment (fg_offset > bg_offset)
                        if fg_offsets[d] > bg_offsets[d] {
                            row_spans[d].push(MutableSpan::new(
                                (row_char_len as i32 + bg_offsets[d]).max(0) as usize,
                                (row_char_len as i32 + fg_offsets[d]).max(0) as usize,
                                Some(overlay_styles(last_style, *fragment_style)),
                            ));
                        }
                        // Fragment overlaps row (fg_offset < bg_offset)
                        else if fg_offsets[d] < bg_offsets[d] {
                            row_spans[d].push(MutableSpan::new(
                                (row_char_len as i32 + fg_offsets[d]).max(0) as usize,
                                (row_char_len as i32 + bg_offsets[d]).max(0) as usize,
                                Some(overlay_styles(*fragment_style, last_style)),
                            ));
                        }
                    } else {
                        // Simple case: end previous span at fg boundary
                        if let Some(last_span) = row_spans[d].last_mut() {
                            last_span.end =
                                (row_char_len as i32 + fg_offsets[d]).max(0) as usize;
                        }
                    }

                    let new_start = (row_char_len as i32
                        + if split_styles {
                            fg_offsets[d].max(bg_offsets[d])
                        } else {
                            fg_offsets[d]
                        })
                    .max(0) as usize;
                    let new_end = row_char_len + fragment_rows[d].chars().count();
                    row_spans[d].push(MutableSpan::new(new_start, new_end, *fragment_style));
                }

                // Merge the fragment into the accumulated row
                row_chars = Glyphs::merge(&row_chars, &fragment_rows, fragment_spacing);
                last_style = *fragment_style;
            }

            // Truncate rows to width
            let row_chars: Vec<String> = row_chars
                .iter()
                .map(|row| row.chars().take(width).collect::<String>())
                .collect();

            // Right-pad for non-default justification
            let row_chars: Vec<String> = if !matches!(justify, JustifyMethod::Default) {
                row_chars
                    .iter()
                    .map(|row| {
                        let len = row.chars().count();
                        if len < width {
                            let mut padded = row.clone();
                            for _ in len..width {
                                padded.push(' ');
                            }
                            padded
                        } else {
                            row.clone()
                        }
                    })
                    .collect()
            } else {
                row_chars
            };

            let final_row_len = row_chars.first().map(|r| r.chars().count()).unwrap_or(0);

            // Append a closing null span to cover any remaining space
            for d in 0..line_height {
                let last_end = row_spans[d].last().map(|s| s.end).unwrap_or(0);
                row_spans[d].push(MutableSpan::new(last_end, final_row_len, None));
            }

            // Resolve overlapping spans
            let row_spans: Vec<Vec<MutableSpan>> = row_spans
                .into_iter()
                .map(MutableSpan::resolve)
                .collect();

            // Emit segments row by row
            for (row_num, (row, spans)) in row_chars.iter().zip(row_spans.iter()).enumerate() {
                let row_chars_vec: Vec<char> = row.chars().collect();
                for span in spans {
                    let start = span.start.min(row_chars_vec.len());
                    let end = span.end.min(row_chars_vec.len());
                    let mut fragment_text: String = row_chars_vec[start..end].iter().collect();
                    let mut style = span.style;

                    if let Some(st) = style {
                        let mut style_override = LINE_STYLE_RESET;

                        for (line_style, attr) in [
                            (self.font.underline(), "underline"),
                            (self.font.underline2(), "underline2"),
                            (self.font.overline(), "overline"),
                            (self.font.strike(), "strike"),
                        ] {
                            if !line_style_active(&st, attr) || line_style.index != row_num {
                                continue;
                            }
                            if line_style.line == LineType::Custom {
                                if let Some(custom_char) = line_style.char {
                                    fragment_text = fragment_text
                                        .replace(' ', &custom_char.to_string());
                                }
                            } else {
                                let line_type = &line_style.line;
                                style_override = style_override.combine(
                                    &line_type_to_style(line_type),
                                );
                            }
                        }

                        style = Some(st.combine(&style_override));
                    }

                    all_segments.push(if let Some(s) = style {
                        Segment::styled(fragment_text, s)
                    } else {
                        Segment::new(fragment_text)
                    });
                }
                all_segments.push(Segment::line());
            }
        }

        all_segments
    }

    // ========================================================================
    // Private helpers
    // ========================================================================

    /// Returns whether the given char/ligature can overlap with its neighbor.
    fn should_overlap(&self, a: &str, b: &str) -> bool {
        if a.is_empty() || b.is_empty() {
            return false;
        }
        if !self.use_kerning {
            return false;
        }
        let a_bad = a.chars().any(|c| NON_OVERLAPPING.contains(c));
        let b_bad = b.chars().any(|c| NON_OVERLAPPING.contains(c));
        !a_bad && !b_bad
    }

    /// Character positions that are glyph start positions (excluding mid-ligature indices).
    fn glyph_borders(&self, text: &str) -> Vec<usize> {
        let result: Vec<usize> = (0..text.chars().count()).collect();
        if !self.use_ligatures || self.font.ligature_keys().is_empty() {
            return result;
        }

        let ligatures = self.font.ligature_keys();
        let mut sorted_ligs: Vec<&str> = ligatures.iter().map(|s| *s).collect();
        sorted_ligs.sort_by(|a, b| b.len().cmp(&a.len()));

        let pattern = sorted_ligs.join("|");
        let re = build_regex(&pattern);

        let mut to_remove: Vec<usize> = Vec::new();
        let chars_vec: Vec<(usize, char)> = text.char_indices().collect();
        let byte_to_char: std::collections::HashMap<usize, usize> = chars_vec
            .iter()
            .enumerate()
            .map(|(ci, (bi, _))| (*bi, ci))
            .collect();

        for m in re.find_iter(text) {
            let start_char = *byte_to_char.get(&m.start()).unwrap_or(&0);
            let char_count = m.as_str().chars().count();
            for d in 1..char_count {
                to_remove.push(start_char + d);
            }
        }

        result.into_iter().filter(|i| !to_remove.contains(i)).collect()
    }

    /// Style transition points in `text`, sorted by character position.
    ///
    /// Returns `Vec<(char_pos, combined_style)>`.
    fn style_borders(&self, text_len: usize) -> Vec<(usize, Option<Style>)> {
        // styles[0] = base style; styles[idx] = span styles
        let base_style = if self.style.is_null() {
            None
        } else {
            Some(self.style)
        };
        let mut styles: Vec<Option<Style>> = vec![base_style];

        // borders: position -> Vec<(direction: +1=open / -1=close, style_index)>
        let mut borders: BTreeMap<usize, Vec<(i32, usize)>> = BTreeMap::new();
        borders.entry(0).or_default().push((1, 0));
        borders.entry(text_len).or_default().push((-1, 0));

        for (idx, span) in self.spans.iter().enumerate() {
            let span_idx = idx + 1;
            borders.entry(span.start).or_default().push((1, span_idx));
            borders.entry(span.end).or_default().push((-1, span_idx));
            let span_style = if span.style.is_null() {
                None
            } else {
                Some(span.style)
            };
            styles.push(span_style);
        }

        let mut stack: Vec<usize> = Vec::new();
        let mut result: Vec<(usize, Option<Style>)> = Vec::new();

        for (&pos, events) in &borders {
            let mut sorted_events = events.clone();
            sorted_events.sort(); // -1 before +1, removes before adds
            for (direction, idx) in sorted_events {
                if direction > 0 {
                    stack.push(idx);
                } else {
                    stack.retain(|&x| x != idx);
                }
            }

            if pos < text_len {
                let combined = combine_styles(stack.iter().filter_map(|&d| styles[d]));
                result.push((pos, combined));
            }
        }

        if result.is_empty() || result[0].0 != 0 {
            result.insert(0, (0, None));
        }

        result
    }

    /// Combine glyph and style borders into `(text_fragment, style)` pairs.
    fn style_fragments(
        &self,
        text: &str,
        _console: &Console,
    ) -> Vec<(String, Option<Style>)> {
        let text_len = text.chars().count();
        let glyph_border_set: std::collections::HashSet<usize> =
            self.glyph_borders(text).into_iter().collect();

        let styles = self.style_borders(text_len);

        let corrected: std::collections::BTreeMap<usize, Option<Style>> = if self.use_ligatures {
            let glyph_borders_sorted: Vec<usize> = {
                let mut v: Vec<usize> = glyph_border_set.iter().copied().collect();
                v.sort();
                v
            };
            let mut c = std::collections::BTreeMap::new();
            for (pos, style) in &styles {
                if glyph_border_set.contains(pos) {
                    c.insert(*pos, *style);
                } else {
                    // Find nearest glyph border
                    let right_idx = glyph_borders_sorted.partition_point(|&x| x <= *pos);
                    let left_idx = right_idx.saturating_sub(1);
                    if self.style_ligatures == Some(LigatureStyleMethod::Last) {
                        if left_idx < glyph_borders_sorted.len() {
                            let neighbor = glyph_borders_sorted[left_idx];
                            c.insert(neighbor, *style); // last border to snap here wins
                        }
                    } else {
                        // Default: snap to next (right) glyph border
                        if right_idx < glyph_borders_sorted.len() {
                            let neighbor = glyph_borders_sorted[right_idx];
                            c.insert(neighbor, *style); // last border to snap here wins
                        }
                    }
                }
            }
            c
        } else {
            styles.into_iter().collect()
        };

        let text_chars: Vec<char> = text.chars().collect();
        let mut result: Vec<(String, Option<Style>)> = Vec::new();

        let positions: Vec<usize> = corrected.keys().copied().collect();
        for (i, &start) in positions.iter().enumerate() {
            let end = positions.get(i + 1).copied().unwrap_or(text_len);
            let fragment: String = text_chars[start..end.min(text_chars.len())]
                .iter()
                .collect();
            result.push((fragment, corrected[&start]));
        }

        result
    }

    /// Full justification: expand spaces to fill `width`.
    fn justify_full(
        &self,
        width: usize,
        line: &str,
        spans: Vec<(String, Option<Style>)>,
    ) -> Vec<(String, Option<Style>)> {
        let line = line.trim_end();
        let line_width = self.rendered_width(line);
        let words: Vec<&str> = line.split(' ').collect();
        let num_words = words.len();
        if num_words <= 1 {
            return spans;
        }

        let space_width = self.font.space_width();
        let num_spaces = num_words - 1;
        let words_rendered: usize = words.iter().map(|w| self.rendered_width(w)).sum();
        let _ = words_rendered; // used implicitly via line_width
        let _ = line_width;

        // Build space counts: start with 1 space per gap, add extras from the right
        let mut spaces: Vec<usize> = vec![1; num_spaces];
        let mut total_spaces = num_spaces;
        let mut index = 0;
        while {
            let current_width: usize = words.iter().map(|w| self.rendered_width(w)).sum::<usize>()
                + total_spaces * space_width;
            current_width < width
        } {
            spaces[num_spaces - index - 1] += 1;
            total_spaces += 1;
            index = (index + 1) % num_spaces;
        }

        // Build the adjusted line
        let adjusted_line: String = words
            .iter()
            .zip(spaces.iter().chain(std::iter::once(&0usize)))
            .map(|(word, &space_count)| {
                let mut s = word.to_string();
                s.push_str(&" ".repeat(space_count));
                s
            })
            .collect();

        // Re-map span positions from original to adjusted line
        let mut result: Vec<(String, Option<Style>)> = Vec::new();
        let mut pos = 0;
        let original_chars: Vec<char> = line.chars().collect();
        let adjusted_chars: Vec<char> = adjusted_line.chars().collect();

        for (txt, style) in &spans {
            let end = pos + txt.chars().count();
            let spaces_before_pos =
                original_chars[..pos.min(original_chars.len())].iter().filter(|&&c| c == ' ').count();
            let spaces_before_end =
                original_chars[..end.min(original_chars.len())].iter().filter(|&&c| c == ' ').count();

            let extra_before: usize = spaces[..spaces_before_pos.min(spaces.len())]
                .iter()
                .sum::<usize>()
                .saturating_sub(spaces_before_pos);
            let extra_before_end: usize = spaces[..spaces_before_end.min(spaces.len())]
                .iter()
                .sum::<usize>()
                .saturating_sub(spaces_before_end);

            let adj_pos = pos + extra_before;
            let adj_end = end + extra_before_end;

            let fragment: String = adjusted_chars
                [adj_pos.min(adjusted_chars.len())..adj_end.min(adjusted_chars.len())]
                .iter()
                .collect();
            result.push((fragment, *style));
            pos = end;
        }

        result
    }

    /// Break `text` into character-offset segments for word-wrapping.
    fn divide_offsets(&self, text: &str, width: usize, fold: bool) -> Vec<usize> {
        let mut offsets: Vec<usize> = Vec::new();
        let space_length = self.font.space_width();
        let mut offset: usize = 0;
        let mut length: usize = 0;

        for word in text.split(' ') {
            let remaining = (width as i32 - length as i32 - space_length as i32).max(0) as usize;
            let word_length = self.rendered_width(word);

            if word_length > remaining {
                if fold
                    && word_length > width
                    && !word.is_empty()
                    && self.rendered_width(&word.chars().next().unwrap().to_string()) <= remaining
                {
                    // Fold mid-word
                    let mut fold_offset = 1;
                    while fold_offset < word.chars().count() {
                        let part: String = word.chars().take(fold_offset + 1).collect();
                        if self.rendered_width(&part) > remaining {
                            break;
                        }
                        fold_offset += 1;
                    }
                    let part: String = word.chars().take(fold_offset).collect();
                    let part_length = self.rendered_width(&part);

                    if offset > 0 {
                        offset += 1;
                    }
                    offset += fold_offset;
                    if length > 0 {
                        length += space_length;
                    }
                    length += part_length;

                    // Chop remaining of word
                    let rest: String = word.chars().skip(fold_offset).collect();
                    for chop in self.chop_cells(&rest, width) {
                        offsets.push(offset);
                        offset += chop.chars().count();
                        length = self.rendered_width(&chop);
                    }
                } else {
                    // Word doesn't fit: break before it
                    length = word_length;
                    if offset > 0 || word.is_empty() {
                        offset += 1;
                    }
                    offsets.push(offset);
                    offset += word.chars().count();
                }
            } else {
                // Word fits on current line
                if length > 0 || word.is_empty() {
                    length += space_length;
                }
                length += word_length;
                if offset > 0 || word.is_empty() {
                    offset += 1;
                }
                offset += word.chars().count();
            }
        }

        offsets.push(offset);
        offsets
    }

    /// Split `text` into chunks that each fit within `width` rendered cells.
    fn chop_cells(&self, text: &str, width: usize) -> Vec<String> {
        let mut result = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut lst = 0;
        let mut curr = 1;

        while curr < chars.len() {
            while curr < chars.len() {
                let part: String = chars[lst..curr].iter().collect();
                if self.rendered_width(&part) > width {
                    break;
                }
                curr += 1;
            }
            let part: String = chars[lst..curr].iter().collect();
            result.push(part);
            lst = curr;
        }

        if lst < chars.len() {
            let part: String = chars[lst..].iter().collect();
            result.push(part);
        }

        result
    }
}

// ============================================================================
// Free helper functions
// ============================================================================

/// Combine an iterator of `Style` values left-to-right (later = higher priority).
fn combine_styles(styles: impl Iterator<Item = Style>) -> Option<Style> {
    let mut iter = styles.peekable();
    if iter.peek().is_none() {
        return None;
    }
    Some(iter.fold(Style::new(), |acc, s| acc.combine(&s)))
}

/// Blend two styles for the overlap zone: FG attributes from `fg`, BG from `bg`.
fn overlay_styles(fg: Option<Style>, bg: Option<Style>) -> Style {
    let fg = fg.unwrap_or_default();
    let bg = bg.unwrap_or_default();
    Style {
        color: fg.color,
        blink: fg.blink,
        strike: fg.strike,
        bgcolor: bg.bgcolor,
        underline: bg.underline,
        underline2: bg.underline2,
        overline: bg.overline,
        ..Style::new()
    }
}

/// Return true if the style has a given line decoration enabled.
fn line_style_active(style: &Style, attr: &str) -> bool {
    match attr {
        "underline" => style.underline == Some(true),
        "underline2" => style.underline2 == Some(true),
        "overline" => style.overline == Some(true),
        "strike" => style.strike == Some(true),
        _ => false,
    }
}

/// Convert a `LineType` to the corresponding `Style` that enables it.
fn line_type_to_style(line_type: &LineType) -> Style {
    match line_type {
        LineType::Underline => Style::new().with_underline(true),
        LineType::Underline2 => Style::new().with_underline2(true),
        LineType::Overline => Style::new().with_overline(true),
        LineType::Strike => Style::new().with_strike(true),
        LineType::Custom => Style::new(),
    }
}

/// Build or retrieve a compiled regex from a pattern string.
fn build_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("invalid ligature regex")
}

// ============================================================================
// Renderable impl
// ============================================================================

impl Renderable for Typography {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let justify = options
            .justify
            .or(self.justify)
            .unwrap_or(JustifyMethod::Default);
        let overflow = options.overflow.or(self.overflow);
        let tab_size = self.tab_size.unwrap_or(options.tab_size);
        let no_wrap = self.no_wrap.map(|v| v || options.no_wrap);

        let lines = self.wrap(
            options.max_width,
            overflow,
            Some(tab_size),
            no_wrap,
        );

        let mut segs = Segments::default();
        for line in &lines {
            let line_segs = line.render_line(console, options.max_width, justify);
            for seg in line_segs {
                segs.push(seg);
            }
        }
        segs
    }

    fn measure(&self, _console: &Console, _options: &ConsoleOptions) -> Measurement {
        // Minimum = widest single glyph; maximum = full rendered width
        let all_glyph_keys: std::collections::HashSet<String> = {
            let mut set: std::collections::HashSet<String> =
                self.text.chars().map(|c| c.to_string()).collect();
            for (_, key) in self.split_glyphs(&self.text) {
                set.insert(key);
            }
            set
        };
        let minimum = all_glyph_keys
            .iter()
            .map(|k| self.rendered_width(k))
            .max()
            .unwrap_or(0);
        let maximum = self.rendered_width(&self.text);
        Measurement::new(minimum, maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Font;

    fn semi_font() -> Arc<Font> {
        Arc::new(Font::builtin("condensedsemi").expect("condensedsemi not found").clone())
    }

    #[test]
    fn test_builtin_fonts_load() {
        for name in &["condensedsemi", "condensedsans", "condensedserif",
                      "extended.condensedsans", "extended.sans"] {
            assert!(Font::builtin(name).is_some(), "font {name} missing");
        }
    }

    #[test]
    fn test_rendered_width_nonempty() {
        let font = semi_font();
        let t = Typography::new("Hi", Style::default(), font, 0, true, true, None);
        assert!(t.rendered_width("H") > 0);
        assert!(t.rendered_width("Hi") >= t.rendered_width("H"));
    }

    #[test]
    fn test_split_glyphs_ligature() {
        let font = semi_font();
        let t = Typography::new("fi", Style::default(), font, 0, true, true, None);
        let glyphs = t.split_glyphs("fi");
        // "fi" should be a single ligature glyph if the font supports it,
        // or two separate glyphs otherwise.  Either way, positions must be ≥ 0
        // and keys must be non-empty.
        assert!(!glyphs.is_empty());
        for (_, key) in &glyphs {
            assert!(!key.is_empty());
        }
    }

    #[test]
    fn test_render_produces_segments() {
        let font = semi_font();
        let t = Typography::new("Hi", Style::default(), font, 0, true, true, None);
        let console = Console::new();
        let options = console.options();
        let segs = t.render(&console, &options);
        // Should produce at least one segment
        assert!(segs.len() > 0);
    }

    #[test]
    fn test_measure() {
        let font = semi_font();
        let t = Typography::new("AB", Style::default(), font, 0, true, true, None);
        let console = Console::new();
        let options = console.options();
        let m = t.measure(&console, &options);
        assert!(m.minimum > 0);
        assert!(m.maximum >= m.minimum);
    }

    #[test]
    fn test_wrap_respects_width() {
        let font = semi_font();
        let t = Typography::new("Hello World", Style::default(), font, 0, true, true, None);
        // Wrap at a very narrow width — should split into multiple lines
        let lines = t.wrap(5, None, None, None);
        assert!(lines.len() >= 2, "expected at least 2 lines, got {}", lines.len());
    }
}
