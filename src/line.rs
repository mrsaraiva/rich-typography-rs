/// Type of decorative line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineType {
    Underline,
    Underline2,
    Strike,
    Overline,
    Custom,
}

impl LineType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "underline" => Some(LineType::Underline),
            "underline2" => Some(LineType::Underline2),
            "strike" => Some(LineType::Strike),
            "overline" => Some(LineType::Overline),
            "custom" => Some(LineType::Custom),
            _ => None,
        }
    }
}

/// Style configuration for underline, overline, or strikethrough decorations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineStyle {
    /// Which font row this decoration appears on (0-indexed).
    pub index: usize,
    /// The type of line decoration.
    pub line: LineType,
    /// Custom character (only used when `line == LineType::Custom`).
    pub char: Option<char>,
}

impl LineStyle {
    pub fn new(index: usize, line: LineType, char: Option<char>) -> Self {
        LineStyle { index, line, char }
    }
}

/// Override type used when constructing a `Font`: either a row index or a full `LineStyle`.
#[derive(Debug, Clone)]
pub enum LineStyleOverride {
    /// Override only the row index, keeping the line type and char from the default.
    Index(usize),
    /// Override with a complete `LineStyle`, merging right-side-wins.
    Style(LineStyle),
}

impl From<usize> for LineStyleOverride {
    fn from(i: usize) -> Self {
        LineStyleOverride::Index(i)
    }
}

impl From<LineStyle> for LineStyleOverride {
    fn from(s: LineStyle) -> Self {
        LineStyleOverride::Style(s)
    }
}

impl LineStyle {
    /// Apply an optional override on top of this LineStyle.
    ///
    /// Mirrors Python's `LineStyle.__or__`:
    /// - `None` → return self unchanged
    /// - `Index(i)` → replace index only
    /// - `Style(other)` → merge (right-side wins on non-zero/non-None fields)
    pub fn with_override(self, other: Option<LineStyleOverride>) -> Self {
        match other {
            None => self,
            Some(LineStyleOverride::Index(i)) => LineStyle { index: i, ..self },
            Some(LineStyleOverride::Style(s)) => LineStyle {
                index: if s.index != 0 { s.index } else { self.index },
                line: s.line,
                char: s.char.or(self.char),
            },
        }
    }
}
