//! Format Document: the whole text in the standard layout (`kalinga fmt`).

use lsp_types::{DocumentFormattingParams, Position, Range, TextEdit};

use crate::world::World;

pub fn format(world: &mut World, p: DocumentFormattingParams) -> Option<Vec<TextEdit>> {
    let text = world.text(&p.text_document.uri)?;
    // Code that does not lex is left alone; the diagnostics say why.
    let formatted = kalinga_fmt::format(text).ok()?;
    if formatted == text {
        return Some(vec![]);
    }
    let lines = text.split('\n').count() as u32;
    let whole = Range::new(Position::new(0, 0), Position::new(lines, 0));
    Some(vec![TextEdit::new(whole, formatted)])
}
