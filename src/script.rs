//! Display units for complex scripts (Sinhala conjuncts).
//!
//! Ratatui/crossterm print one grapheme per cell and drop zero-width marks
//! (ZWJ, some vowel signs). Unicode grapheme rules also split Sinhala
//! conjuncts (`ශ්` + ZWJ + `රී`). The terminal then never sees a complete
//! run for DirectWrite to shape.
//!
//! We merge virama/ZWJ/consonant sequences into one unit and write that
//! whole string into a single cell so the terminal can shape it.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const VIRAMA: char = '\u{0DCA}';
const ZWJ: char = '\u{200D}';
const ZWNJ: char = '\u{200C}';

pub fn contains_sinhala(s: &str) -> bool {
    s.chars().any(is_sinhala)
}

fn is_sinhala(c: char) -> bool {
    (0x0D80..=0x0DFF).contains(&(c as u32))
}

fn is_sinhala_consonant(c: char) -> bool {
    (0x0D9A..=0x0DC6).contains(&(c as u32))
}

fn ends_with_linker(s: &str) -> bool {
    let mut chars = s.chars().rev().peekable();
    match chars.next() {
        Some(ZWJ | ZWNJ) => chars.next() == Some(VIRAMA),
        Some(VIRAMA) => true,
        _ => false,
    }
}

fn starts_with_consonant(s: &str) -> bool {
    s.chars().next().is_some_and(is_sinhala_consonant)
}

/// Visual clusters: Unicode graphemes, then glue Sinhala conjuncts.
pub fn display_units(s: &str) -> Vec<String> {
    let graphemes: Vec<&str> = s.graphemes(true).collect();
    if !contains_sinhala(s) {
        return graphemes.into_iter().map(str::to_string).collect();
    }

    let mut units = Vec::new();
    let mut i = 0;
    while i < graphemes.len() {
        let mut unit = graphemes[i].to_string();
        i += 1;
        while i < graphemes.len() {
            let next = graphemes[i];
            let join = next == "\u{200D}"
                || next == "\u{200C}"
                || (ends_with_linker(&unit) && starts_with_consonant(next));
            if !join {
                break;
            }
            unit.push_str(next);
            i += 1;
        }
        units.push(unit);
    }
    units
}

pub fn unit_width(unit: &str) -> u16 {
    UnicodeWidthStr::width(unit).max(1) as u16
}

pub fn display_width(s: &str) -> u16 {
    display_units(s).iter().map(|u| unit_width(u.as_str())).sum()
}

pub fn ellipsize_shaped(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if display_width(s) as usize <= max_cols {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    let limit = max_cols.saturating_sub(1);
    for unit in display_units(s) {
        let w = unit_width(&unit) as usize;
        if used + w > limit {
            break;
        }
        out.push_str(&unit);
        used += w;
    }
    out.push('…');
    out
}

pub fn wrap_shaped(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut used = 0usize;
        let mut pending_space = false;
        for unit in display_units(paragraph) {
            if unit == " " {
                pending_space = !current.is_empty();
                continue;
            }
            let w = unit_width(unit.as_str()) as usize;
            let extra = usize::from(pending_space);
            if used + extra + w > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                used = 0;
                pending_space = false;
            }
            if pending_space {
                current.push(' ');
                used += 1;
                pending_space = false;
            }
            if w > width && current.is_empty() {
                current.push_str(&unit);
                lines.push(std::mem::take(&mut current));
                used = 0;
                continue;
            }
            current.push_str(&unit);
            used += w;
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

/// Write `text` into `area` using display units (one conjunct per cell).
pub fn render_shaped(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = wrap_shaped(text, area.width as usize);
    for (row, line) in lines.iter().take(area.height as usize).enumerate() {
        let y = area.y + row as u16;
        let mut x = area.x;
        for unit in display_units(line) {
            let w = unit_width(unit.as_str());
            if x.saturating_add(w) > area.right() {
                break;
            }
            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.set_symbol(&unit);
                cell.set_style(style);
            }
            for dx in 1..w {
                if let Some(cell) = buf.cell_mut(Position::new(x + dx, y)) {
                    cell.reset();
                    cell.set_style(style);
                }
            }
            x = x.saturating_add(w);
        }
        while x < area.right() {
            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.set_symbol(" ");
                cell.set_style(style);
            }
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sri_conjunct_is_one_unit_and_keeps_zwj() {
        let s = "ශ්‍රී";
        let units = display_units(s);
        assert_eq!(units, vec![s.to_string()], "{units:?}");
        assert!(units[0].contains('\u{200D}'));
    }

    #[test]
    fn lanka_keeps_all_codepoints() {
        let s = "ශ්‍රී ලංකාව";
        let joined: String = display_units(s).into_iter().collect();
        assert_eq!(joined, s);
    }

    #[test]
    fn wrap_does_not_split_conjunct() {
        let s = "ශ්‍රීලංකා";
        let lines = wrap_shaped(s, 2);
        for line in &lines {
            if line.contains('ශ') {
                assert!(
                    line.contains('\u{200D}') || display_units(line).iter().any(|u| u.contains('ශ')),
                    "{line:?} units={:?}",
                    display_units(line)
                );
            }
            for unit in display_units(line) {
                if unit.contains('ශ') {
                    assert!(unit.contains('\u{200D}'), "split conjunct {unit:?}");
                }
            }
        }
    }

    #[test]
    fn latin_unchanged() {
        assert_eq!(
            display_units("hello"),
            ["h", "e", "l", "l", "o"].map(str::to_string)
        );
        assert_eq!(ellipsize_shaped("hello world", 8), "hello w…");
    }
}
