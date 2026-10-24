//! Positions in a text. The compiler counts 1-based lines and characters; LSP counts
//! 0-based lines and UTF-16 code units.

use kalinga_syntax::error::Span;
use lsp_types::{Position, Range};

/// Line `line` (0-based) without its line ending, or "" past the end.
pub fn line(text: &str, line: usize) -> &str {
    text.split('\n').nth(line).map_or("", |l| l.strip_suffix('\r').unwrap_or(l))
}

/// The LSP position of the 1-based `line` and character `col`.
pub fn position(text: &str, line_no: usize, col: usize) -> Position {
    let l = line(text, line_no.saturating_sub(1));
    let units: usize = l.chars().take(col.saturating_sub(1)).map(char::len_utf16).sum();
    Position::new(line_no.saturating_sub(1) as u32, units as u32)
}

/// The range a span covers (at least one character, so it can be seen).
pub fn range(text: &str, span: Span) -> Range {
    let start = position(text, span.line, span.col);
    let end = position(text, span.line, span.col + span.len.max(1));
    Range::new(start, end)
}

/// The 1-based (line, character column) of an LSP position.
pub fn line_col(text: &str, pos: Position) -> (usize, usize) {
    let l = line(text, pos.line as usize);
    let mut units = 0;
    let mut col = 1;
    for c in l.chars() {
        if units >= pos.character as usize {
            break;
        }
        units += c.len_utf16();
        col += 1;
    }
    (pos.line as usize + 1, col)
}

/// The byte offset of an LSP position.
pub fn offset(text: &str, pos: Position) -> usize {
    let (line_no, col) = line_col(text, pos);
    let start: usize = text.split_inclusive('\n').take(line_no - 1).map(str::len).sum();
    let l = line(text, line_no - 1);
    start + l.char_indices().nth(col - 1).map_or(l.len(), |(i, _)| i)
}

/// The 0-based line a byte offset is on.
pub fn line_index(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count()
}

/// The 1-based (line, character column) of a byte offset.
pub fn line_col_of_offset(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (before.matches('\n').count() + 1, before[line_start..].chars().count() + 1)
}

/// Does `span` (on one line) contain the 1-based position?
pub fn contains(span: Span, line_no: usize, col: usize) -> bool {
    span.line == line_no && col >= span.col && col <= span.col + span.len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_columns() {
        let text = "let s = \"é😀\"\nx";
        // `😀` is two UTF-16 units.
        assert_eq!(position(text, 1, 12), Position::new(0, 12));
        assert_eq!(line_col(text, Position::new(0, 12)), (1, 12));
        assert_eq!(offset(text, Position::new(1, 0)), text.find('x').unwrap_or(0));
        let r = range(text, Span { line: 2, col: 1, len: 1, file: 0 });
        assert_eq!(r, Range::new(Position::new(1, 0), Position::new(1, 1)));
    }
}
