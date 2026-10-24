//! Hover: the signature and docs of the name under the cursor.

use kalinga_sema::builtins::LIBRARIES;
use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};

use crate::describe::describe;
use crate::text;
use crate::world::World;

pub fn hover(world: &mut World, params: HoverParams) -> Option<Hover> {
    let at = params.text_document_position_params;
    let snap = world.good_snapshot(&at.text_document.uri)?;
    let source = snap.source(0);
    let (line, col) = text::line_col(source, at.position);
    if let Some(name) = snap.name_at(0, line, col) {
        let d = describe(&snap, &name.symbol, name.ty.as_ref())?;
        let mut md = format!("```kalinga\n{}\n```", d.code);
        if let Some(owner) = &d.owner {
            md.push_str(&format!("\n\nin `{}`", owner));
        }
        if let Some(docs) = &d.docs {
            md.push_str(&format!("\n\n---\n\n{}", docs));
        }
        return Some(markdown(md, text::range(source, name.span)));
    }
    let (word, span) = snap.ident_at(0, line, col)?;
    if LIBRARIES.contains(&word.as_str()) {
        return Some(markdown(format!("```kalinga\n{}\n```\n\nBuilt-in library.", word), text::range(source, span)));
    }
    let import = snap.import_of_alias(&word)?;
    Some(markdown(format!("```kalinga\nimport \"{}\" as {}\n```", import, word), text::range(source, span)))
}

fn markdown(value: String, range: lsp_types::Range) -> Hover {
    Hover { contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value }), range: Some(range) }
}
