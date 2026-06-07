use std::collections::HashMap;

/// A single glyph: a list of row strings, one per font line.
///
/// All rows must be the same character-count width.
pub type Glyph = Vec<String>;

/// A glyph dictionary mapping single chars or ligature sequences to glyphs.
#[derive(Debug, Clone, Default)]
pub struct Glyphs(pub HashMap<String, Glyph>);

impl Glyphs {
    /// Create an empty `Glyphs` map.
    pub fn new() -> Self {
        Glyphs(HashMap::new())
    }

    /// Create a `Glyphs` instance from a sprite-sheet style layout.
    ///
    /// `chars` is a slice of glyph keys (single chars or ligature strings).
    /// `rows` are the packed glyph rows (all the same length), with separator
    /// columns (all-separator-char columns) delimiting individual glyphs.
    ///
    /// # Errors
    /// Returns `Err` if the row lengths differ or the glyph count doesn't match `chars`.
    pub fn from_lines(
        chars: &[&str],
        rows: &[&str],
        separator: Option<char>,
    ) -> Result<Self, String> {
        if chars.is_empty() {
            return Ok(Glyphs::new());
        }
        if rows.is_empty() {
            return Err("No glyph rows provided".to_string());
        }
        let width = rows[0].chars().count();
        if !rows.iter().all(|r| r.chars().count() == width) {
            return Err("Line length mismatch.".to_string());
        }
        if chars.len() == 1 {
            let glyph: Glyph = rows.iter().map(|r| r.to_string()).collect();
            let mut map = HashMap::new();
            map.insert(chars[0].to_string(), glyph);
            return Ok(Glyphs(map));
        }
        let map = Self::get_char_map(chars, rows, separator)?;
        Ok(Glyphs(map))
    }

    /// Parse the sprite-sheet into a `HashMap<char_key, Glyph>`.
    pub fn get_char_map(
        chars: &[&str],
        rows: &[&str],
        separator: Option<char>,
    ) -> Result<HashMap<String, Glyph>, String> {
        let sep = separator.unwrap_or(' ');

        // Find separator columns: columns where every row is the separator char.
        let row_chars: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
        let width = row_chars[0].len();
        let breaks: Vec<usize> = (0..width)
            .filter(|&col| row_chars.iter().all(|row| row.get(col).copied() == Some(sep)))
            .collect();

        // Build ranges: [-1, break0], [break0, break1], ..., [breakN, width]
        let starts: Vec<isize> = std::iter::once(-1_isize)
            .chain(breaks.iter().map(|&b| b as isize))
            .collect();
        let ends: Vec<usize> = breaks.iter().copied().chain(std::iter::once(width)).collect();

        if starts.len() != chars.len() {
            return Err(format!(
                "Number of glyphs ({}) does not match number of chars ({}).",
                starts.len(),
                chars.len()
            ));
        }

        let mut result = HashMap::new();
        for (char_key, (&start, &end)) in chars.iter().zip(starts.iter().zip(ends.iter())) {
            let col_start = (start + 1) as usize;
            let glyph: Glyph = row_chars
                .iter()
                .map(|row| row[col_start..end].iter().collect::<String>())
                .collect();
            result.insert(char_key.to_string(), glyph);
        }

        Ok(result)
    }

    /// Number of trailing space characters in `line`.
    pub fn line_trail(line: &str) -> usize {
        line.chars().rev().take_while(|&c| c == ' ').count()
    }

    /// Number of leading space characters in `line`.
    pub fn line_lead(line: &str) -> usize {
        line.chars().take_while(|&c| c == ' ').count()
    }

    /// Maximum cells two glyphs can overlap without occluding each other.
    ///
    /// Returns the minimum across all rows of (trailing spaces in left + leading spaces in right).
    pub fn max_overlap(left: &Glyph, right: &Glyph) -> usize {
        left.iter()
            .zip(right.iter())
            .map(|(ll, lr)| Self::line_trail(ll) + Self::line_lead(lr))
            .min()
            .unwrap_or(0)
    }

    /// Merge two glyph rows with the given spacing.
    ///
    /// Positive spacing adds space characters between them.
    /// Negative spacing overlaps them: right's non-space characters overwrite left's.
    pub fn merge_line(left: &str, right: &str, spacing: i32) -> String {
        if spacing >= 0 {
            let mut result = left.to_string();
            for _ in 0..spacing {
                result.push(' ');
            }
            result.push_str(right);
            result
        } else {
            let left_chars: Vec<char> = left.chars().collect();
            let right_chars: Vec<char> = right.chars().collect();
            let overlap = (-spacing) as usize;
            let left_len = left_chars.len();

            // left[..left_len - overlap]
            let mut result: Vec<char> = left_chars[..left_len.saturating_sub(overlap)].to_vec();

            // Overlap zone: right wins on non-space chars
            let left_overlap_start = left_len.saturating_sub(overlap);
            for i in 0..overlap {
                let ll = left_chars.get(left_overlap_start + i).copied().unwrap_or(' ');
                let lr = right_chars.get(i).copied().unwrap_or(' ');
                result.push(if lr == ' ' { ll } else { lr });
            }

            // right[overlap..]
            result.extend_from_slice(&right_chars[overlap.min(right_chars.len())..]);

            result.iter().collect()
        }
    }

    /// Merge two glyphs row-by-row with the given spacing.
    pub fn merge(left: &Glyph, right: &Glyph, spacing: i32) -> Glyph {
        left.iter()
            .zip(right.iter())
            .map(|(ll, lr)| Self::merge_line(ll, lr, spacing))
            .collect()
    }

    /// Per-row signed offset from the end of `left` where the right fragment's
    /// foreground color should start.
    ///
    /// Returns a list of non-positive offsets (0 = at the end of left, negative = overlap).
    pub fn boundary(left: &Glyph, right: &Glyph, spacing: i32) -> Vec<i32> {
        let line_height = left.len();
        (0..line_height)
            .map(|row| {
                let lead = Self::line_lead(&right[row]) as i32;
                let trail = Self::line_trail(&left[row]) as i32;
                let a = (spacing + lead).min(0);
                let b = spacing.max(-trail);
                a.min(b)
            })
            .collect()
    }

    /// Per-row signed offset from the end of `left` where the background color
    /// transitions from left to right.
    ///
    /// Uses a majority-vote across all rows to decide where the BG boundary falls.
    pub fn bg_boundary(left: &Glyph, right: &Glyph, spacing: i32) -> Vec<i32> {
        let line_height = left.len();
        let abs_spacing = spacing.unsigned_abs() as usize;
        let mut offsets = vec![0i32; line_height];

        let left_chars: Vec<Vec<char>> = left.iter().map(|r| r.chars().collect()).collect();
        let right_chars: Vec<Vec<char>> = right.iter().map(|r| r.chars().collect()).collect();

        for d in 0..abs_spacing {
            let majority: i32 = (0..line_height)
                .map(|row| {
                    let l_len = left_chars[row].len();
                    let l_char = if d + 1 <= l_len {
                        left_chars[row][l_len - d - 1]
                    } else {
                        ' '
                    };
                    let r_idx = abs_spacing - d - 1;
                    let r_char = right_chars[row].get(r_idx).copied().unwrap_or(' ');

                    let l_score = if l_char != ' ' { 1i32 } else { 0 };
                    let r_score = if r_char != ' ' { 1i32 } else { 0 };
                    l_score - r_score
                })
                .sum();

            if majority > 0 {
                break;
            } else {
                offsets = vec![-(d as i32 + 1); line_height];
            }
        }

        offsets
    }

    /// Number of rows in this glyph set (returns 0 if empty).
    pub fn line_height(&self) -> usize {
        self.0.values().next().map(|g| g.len()).unwrap_or(0)
    }

    /// All glyph keys in this map (chars and/or ligature sequences).
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    /// Look up a glyph by key.
    pub fn get(&self, key: &str) -> Option<&Glyph> {
        self.0.get(key)
    }

    /// Insert a glyph.
    pub fn insert(&mut self, key: String, glyph: Glyph) {
        self.0.insert(key, glyph);
    }

    /// Extend from another `Glyphs` map (right-side wins on duplicate keys).
    pub fn extend(&mut self, other: Glyphs) {
        self.0.extend(other.0);
    }

    /// Returns true if the map contains the given key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Returns true if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
