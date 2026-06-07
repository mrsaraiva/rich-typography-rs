use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use once_cell::sync::Lazy;

use crate::glyph::{Glyph, Glyphs};
use crate::line::{LineStyle, LineStyleOverride, LineType};

// ============================================================================
// Built-in character sets (mirrors Python's `string` module constants)
// ============================================================================

const ASCII_LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const ASCII_UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const WHITESPACE: &str = " \t\n\r\x0b\x0c";
const ASCII_LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const PRINTABLE: &str =
    "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c";
const HEX_DIGITS: &str = "0123456789abcdefABCDEF";
const OCT_DIGITS: &str = "01234567";
// Python's string.punctuation (32 chars): !"#$%&'()*+,-./:;<=>?@[\]^_`{|}~
const PUNCTUATION: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

fn string_module_chars(name: &str) -> Option<&'static str> {
    match name {
        "ascii_lowercase" => Some(ASCII_LOWERCASE),
        "ascii_uppercase" => Some(ASCII_UPPERCASE),
        "ascii_letters" => Some(ASCII_LETTERS),
        "digits" => Some(DIGITS),
        "hexdigits" => Some(HEX_DIGITS),
        "octdigits" => Some(OCT_DIGITS),
        "punctuation" => Some(PUNCTUATION),
        "whitespace" => Some(WHITESPACE),
        "printable" => Some(PRINTABLE),
        _ => None,
    }
}

/// Split an ASCII string into a Vec of single-character `&str` slices.
fn ascii_to_char_slices(s: &'static str) -> Vec<&'static str> {
    s.char_indices()
        .map(|(i, c)| &s[i..i + c.len_utf8()])
        .collect()
}

// ============================================================================
// Font errors
// ============================================================================

/// Error type for font loading and parsing.
#[derive(Debug)]
pub enum FontError {
    NotFound(String),
    MissingHeader,
    ParseError(String),
    Io(std::io::Error),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::NotFound(p) => write!(f, "Font file not found: {}", p),
            FontError::MissingHeader => write!(f, "Font file missing [header] section"),
            FontError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            FontError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for FontError {}

impl From<std::io::Error> for FontError {
    fn from(e: std::io::Error) -> Self {
        FontError::Io(e)
    }
}

// ============================================================================
// .toff INI parser
// ============================================================================

type SectionMap = HashMap<String, HashMap<String, String>>;

/// Minimal INI parser compatible with Python's `configparser`:
/// - Sections: `[section_name]` (converted to lowercase)
/// - Key-value: `key: value` (colon separator, key converted to lowercase)
/// - Multi-line values: continuation lines starting with whitespace
/// - Comments: lines starting with `;` or `#`
fn parse_ini(src: &str) -> SectionMap {
    let mut sections: SectionMap = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut current_key: Option<String> = None;
    let mut current_value_lines: Vec<String> = Vec::new();

    fn flush(
        sections: &mut SectionMap,
        section: &Option<String>,
        key: &mut Option<String>,
        value_lines: &mut Vec<String>,
    ) {
        if let (Some(sec), Some(k)) = (section, key.take()) {
            let value = value_lines.join("\n");
            sections.entry(sec.clone()).or_default().insert(k, value);
        }
        value_lines.clear();
    }

    for line in src.lines() {
        // Section header — trim trailing whitespace before checking for ']'
        let line_trimmed = line.trim_end();
        if line_trimmed.starts_with('[') && line_trimmed.ends_with(']') && line_trimmed.len() >= 2 {
            flush(
                &mut sections,
                &current_section,
                &mut current_key,
                &mut current_value_lines,
            );
            current_section =
                Some(line_trimmed[1..line_trimmed.len() - 1].trim().to_lowercase());
            continue;
        }

        // Continuation line: starts with whitespace
        if (line.starts_with(' ') || line.starts_with('\t')) && current_key.is_some() {
            let stripped: String = line.chars().skip_while(|&c| c == ' ' || c == '\t').collect();
            if !stripped.is_empty() {
                current_value_lines.push(stripped);
            }
            continue;
        }

        // Skip blank lines and comments
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }

        // Key-value pair
        if let Some(colon_pos) = line.find(':') {
            flush(
                &mut sections,
                &current_section,
                &mut current_key,
                &mut current_value_lines,
            );
            let key = line[..colon_pos].trim().to_lowercase().to_string();
            let val = line[colon_pos + 1..].trim().to_string();
            current_key = Some(key);
            if !val.is_empty() {
                current_value_lines.push(val);
            }
        }
    }

    flush(
        &mut sections,
        &current_section,
        &mut current_key,
        &mut current_value_lines,
    );

    sections
}

/// Split a multi-line glyph value into glyph rows.
///
/// Each continuation line starts with `│ ` (box char U+2502 + space = 2 chars stripped).
/// Lines are right-padded to equal character-count length.
fn split_glyph_text(text: &str) -> Vec<String> {
    let lines: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            // Strip the first 2 Unicode characters: │ + space
            line.chars().skip(2).collect::<String>()
        })
        .collect();

    if lines.is_empty() {
        return vec![];
    }

    let max_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);

    lines
        .into_iter()
        .map(|mut line| {
            let len = line.chars().count();
            for _ in len..max_len {
                line.push(' ');
            }
            line
        })
        .collect()
}

// ============================================================================
// Font struct
// ============================================================================

/// A typography font with glyph and ligature maps.
#[derive(Debug, Clone)]
pub struct Font {
    name: String,
    line_height: usize,
    glyphs: HashMap<String, Glyph>,
    ligatures: Glyphs,
    letter_spacing: i32,
    space_width: usize,
    baseline: usize,
    underline: LineStyle,
    underline2: LineStyle,
    overline: LineStyle,
    strike: LineStyle,
    placeholder: Glyph,
}

impl Font {
    /// Create a new Font.
    pub fn new(
        name: String,
        glyphs: HashMap<String, Glyph>,
        ligatures: Glyphs,
        letter_spacing: i32,
        space_width: usize,
        baseline: Option<usize>,
        underline: Option<LineStyleOverride>,
        underline2: Option<LineStyleOverride>,
        overline: Option<LineStyleOverride>,
        strike: Option<LineStyleOverride>,
    ) -> Self {
        let line_height = glyphs.values().next().map(|g| g.len()).unwrap_or(5);
        let baseline = baseline.unwrap_or_else(|| line_height.saturating_sub(2));

        let space_glyph = Self::make_space(space_width, line_height);
        let mut all_glyphs = glyphs;
        all_glyphs.insert(" ".to_string(), space_glyph);

        let placeholder = Self::make_placeholder(line_height);

        let underline_style =
            LineStyle::new(baseline, LineType::Underline, None).with_override(underline);
        let underline2_style =
            LineStyle::new(baseline + 1, LineType::Underline2, None).with_override(underline2);
        let overline_style =
            LineStyle::new(0, LineType::Overline, None).with_override(overline);
        let strike_style =
            LineStyle::new(line_height / 2, LineType::Strike, None).with_override(strike);

        Font {
            name,
            line_height,
            glyphs: all_glyphs,
            ligatures,
            letter_spacing,
            space_width,
            baseline,
            underline: underline_style,
            underline2: underline2_style,
            overline: overline_style,
            strike: strike_style,
            placeholder,
        }
    }

    /// Generate a space glyph of the given width and height.
    pub fn make_space(width: usize, line_height: usize) -> Glyph {
        vec![" ".repeat(width); line_height]
    }

    /// Generate a placeholder box glyph for unknown characters.
    pub fn make_placeholder(line_height: usize) -> Glyph {
        let mut g = vec!["│ │".to_string(); line_height];
        if line_height > 0 {
            g[0] = "┌─┐".to_string();
        }
        if line_height > 1 {
            g[line_height - 1] = "└─┘".to_string();
        }
        g
    }

    /// Parse a `.toff` file from a string source.
    pub fn parse_toff(src: &str) -> Result<Font, FontError> {
        let sections = parse_ini(src);

        let header = sections
            .get("header")
            .ok_or(FontError::MissingHeader)?;

        let name = header
            .get("name")
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        let baseline: Option<usize> = header
            .get("baseline")
            .and_then(|v| v.trim().parse().ok());

        let letter_spacing: i32 = header
            .get("letter_spacing")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);

        let space_width: usize = header
            .get("space_width")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(1);

        // Parse line style sections (named sections like [underline], [strike], etc.)
        let parse_line_style_section =
            |data: &HashMap<String, String>| -> Option<LineStyleOverride> {
                let line_str = data.get("line").map(|s| s.trim()).unwrap_or("");
                if !line_str.is_empty() {
                    let index: usize = data.get("index")?.trim().parse().ok()?;
                    let line = LineType::from_str(line_str)?;
                    let char = if line == LineType::Custom {
                        data.get("char").and_then(|s| s.trim().chars().next())
                    } else {
                        None
                    };
                    Some(LineStyleOverride::Style(LineStyle::new(index, line, char)))
                } else {
                    // Only an index (no line type override)
                    let index: usize = data.get("index")?.trim().parse().ok()?;
                    Some(LineStyleOverride::Index(index))
                }
            };

        let mut underline_override: Option<LineStyleOverride> = None;
        let mut underline2_override: Option<LineStyleOverride> = None;
        let mut overline_override: Option<LineStyleOverride> = None;
        let mut strike_override: Option<LineStyleOverride> = None;

        for section_name in &["underline", "underline2", "overline", "strike"] {
            if let Some(data) = sections.get(*section_name) {
                let override_val = parse_line_style_section(data);
                match *section_name {
                    "underline" => underline_override = override_val,
                    "underline2" => underline2_override = override_val,
                    "overline" => overline_override = override_val,
                    "strike" => strike_override = override_val,
                    _ => {}
                }
            }
        }

        // Also handle simple int overrides in [header] for line styles
        // (some toff files may put e.g. `underline: 4` in the header section)
        for (key, override_ref) in &mut [
            ("underline", &mut underline_override),
            ("underline2", &mut underline2_override),
            ("overline", &mut overline_override),
            ("strike", &mut strike_override),
        ] {
            if override_ref.is_none() {
                if let Some(val) = header.get(*key) {
                    if let Ok(idx) = val.trim().parse::<usize>() {
                        **override_ref = Some(LineStyleOverride::Index(idx));
                    }
                }
            }
        }

        // Parse glyph and ligature sections
        let mut glyphs = Glyphs::new();
        let mut ligatures = Glyphs::new();

        let skip_sections: &[&str] = &[
            "header", "underline", "underline2", "overline", "strike", "ligatures",
        ];

        let section_names: Vec<String> = sections.keys().cloned().collect();

        for section_name in &section_names {
            if skip_sections.contains(&section_name.as_str()) {
                continue;
            }

            let data = &sections[section_name];
            let glyph_text = match data.get("glyphs") {
                Some(t) => t,
                None => continue,
            };

            let rows = split_glyph_text(glyph_text);
            if rows.is_empty() {
                continue;
            }
            let rows_ref: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();

            if let Some(chars_static) = string_module_chars(section_name) {
                let chars = ascii_to_char_slices(chars_static);
                match Glyphs::from_lines(&chars, &rows_ref, None) {
                    Ok(g) => glyphs.extend(g),
                    Err(e) => eprintln!(
                        "Warning: failed to parse section '{}': {}",
                        section_name, e
                    ),
                }
            } else if let Some(chars_val) = data.get("chars") {
                let chars_str: String = chars_val.chars().filter(|&c| c != ' ').collect();
                let chars: Vec<String> = chars_str.chars().map(|c| c.to_string()).collect();
                let chars_ref: Vec<&str> = chars.iter().map(|s| s.as_str()).collect();
                match Glyphs::from_lines(&chars_ref, &rows_ref, None) {
                    Ok(g) => glyphs.extend(g),
                    Err(e) => eprintln!(
                        "Warning: failed to parse section '{}': {}",
                        section_name, e
                    ),
                }
            }
        }

        // Parse ligatures section
        if let Some(data) = sections.get("ligatures") {
            if let (Some(sequences_str), Some(glyph_text)) =
                (data.get("sequences"), data.get("glyphs"))
            {
                let sequences: Vec<&str> = sequences_str.split_whitespace().collect();
                let rows = split_glyph_text(glyph_text);
                if !rows.is_empty() {
                    let rows_ref: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
                    match Glyphs::from_lines(&sequences, &rows_ref, None) {
                        Ok(g) => ligatures.extend(g),
                        Err(e) => eprintln!("Warning: failed to parse ligatures: {}", e),
                    }
                }
            }
        }

        Ok(Font::new(
            name,
            glyphs.0,
            ligatures,
            letter_spacing,
            space_width,
            baseline,
            underline_override,
            underline2_override,
            overline_override,
            strike_override,
        ))
    }

    /// Load a font from a `.toff` file path.
    pub fn from_file(path: &Path) -> Result<Font, FontError> {
        if !path.exists() {
            return Err(FontError::NotFound(path.display().to_string()));
        }
        let src = std::fs::read_to_string(path)?;
        Self::parse_toff(&src)
    }

    /// Get a built-in font by name, or `None` if the name is not found.
    ///
    /// Built-in font names:
    /// - `"condensedsans"`
    /// - `"condensedsemi"` (default)
    /// - `"condensedserif"`
    /// - `"extended.condensedsans"`
    /// - `"extended.sans"`
    pub fn builtin(name: &str) -> Option<&'static Font> {
        BUILTIN_FONTS.get(name)
    }

    /// List all available built-in font names.
    pub fn builtin_names() -> Vec<&'static str> {
        let mut names: Vec<&'static str> = BUILTIN_FONTS.keys().copied().collect();
        names.sort();
        names
    }

    // ========================================================================
    // Properties
    // ========================================================================

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn line_height(&self) -> usize {
        self.line_height
    }

    pub fn letter_spacing(&self) -> i32 {
        self.letter_spacing
    }

    pub fn space_width(&self) -> usize {
        self.space_width
    }

    pub fn baseline(&self) -> usize {
        self.baseline
    }

    pub fn underline(&self) -> &LineStyle {
        &self.underline
    }

    pub fn underline2(&self) -> &LineStyle {
        &self.underline2
    }

    pub fn overline(&self) -> &LineStyle {
        &self.overline
    }

    pub fn strike(&self) -> &LineStyle {
        &self.strike
    }

    /// All ligature sequences available in this font.
    pub fn ligature_keys(&self) -> Vec<&str> {
        self.ligatures.keys().map(|s| s.as_str()).collect()
    }

    /// Look up a glyph for a single character or ligature sequence.
    /// Returns the placeholder glyph for unknown keys.
    pub fn get(&self, key: &str) -> &Glyph {
        if key.chars().count() == 1 {
            self.glyphs.get(key).unwrap_or(&self.placeholder)
        } else {
            self.ligatures.get(key).unwrap_or(&self.placeholder)
        }
    }

    /// Check whether a char or ligature is available in this font.
    pub fn contains(&self, key: &str) -> bool {
        if key.chars().count() == 1 {
            self.glyphs.contains_key(key)
        } else {
            self.ligatures.contains_key(key)
        }
    }

    /// Wrap in an `Arc` for cheap cloning.
    pub fn into_arc(self) -> Arc<Font> {
        Arc::new(self)
    }

    /// Reference to the underlying ligature map (for internal use).
    #[allow(dead_code)]
    pub(crate) fn ligatures(&self) -> &Glyphs {
        &self.ligatures
    }
}

// ============================================================================
// Built-in font cache
// ============================================================================

static BUILTIN_FONTS: Lazy<HashMap<&'static str, Font>> = Lazy::new(|| {
    let mut m = HashMap::new();

    macro_rules! load_font {
        ($key:expr, $src:expr) => {
            match Font::parse_toff($src) {
                Ok(font) => {
                    m.insert($key, font);
                }
                Err(e) => {
                    eprintln!("Warning: failed to load built-in font '{}': {}", $key, e);
                }
            }
        };
    }

    load_font!("condensedsans", include_str!("fonts/condensedsans.toff"));
    load_font!("condensedsemi", include_str!("fonts/condensedsemi.toff"));
    load_font!("condensedserif", include_str!("fonts/condensedserif.toff"));
    load_font!(
        "extended.condensedsans",
        include_str!("fonts/extended/condensedsans.toff")
    );
    load_font!("extended.sans", include_str!("fonts/extended/sans.toff"));

    m
});

