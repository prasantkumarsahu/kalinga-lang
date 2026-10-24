//! Asking the type checker what is at the cursor: the identifier being typed is replaced
//! by `kalinga_sema::index::PROBE`, and brackets the line left open are closed, so the
//! program parses.

use kalinga_sema::index::{PROBE, Probe};
use lsp_types::Url;

use crate::snapshot::Snapshot;
use crate::world::World;

/// The probe's findings and the analysis they come from.
pub fn probe(world: &World, uri: &Url, text: &str, from: usize, to: usize) -> Option<(Snapshot, Probe)> {
    probe_with(world, uri, text, from, to, PROBE)
}

/// Like [`probe`], with `insert` (which contains the probe) in place of `text[from..to]`.
pub fn probe_with(
    world: &World,
    uri: &Url,
    text: &str,
    from: usize,
    to: usize,
    insert: &str,
) -> Option<(Snapshot, Probe)> {
    let line_start = text[..from].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[to..].find('\n').map_or(text.len(), |i| to + i);
    let closers = closers(&text[line_start..from]);
    let attempts = [
        // The rest of the line may already close what is open (`f(x.|)`).
        format!("{}{}{}", &text[..from], insert, &text[to..]),
        format!("{}{}{}{}", &text[..from], insert, closers, &text[line_end..]),
    ];
    attempts.iter().find_map(|patched| {
        let snap = world.analyze(uri, patched);
        let probe = snap.analysis.index.probe.clone()?;
        Some((snap, probe))
    })
}

/// The brackets that close what `line` leaves open, innermost first. Text inside strings
/// is skipped, except the code in `{...}`.
pub fn closers(line: &str) -> String {
    let mut stack: Vec<char> = vec![];
    let mut in_string = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (in_string, c) {
            (true, '\\') => {
                chars.next();
            }
            (true, '"') => in_string = false,
            (true, '{') => {
                in_string = false;
                stack.push('"');
            }
            (true, _) => {}
            (false, '"') => in_string = true,
            (false, '(') => stack.push(')'),
            (false, '[') => stack.push(']'),
            (false, '{') => stack.push('}'),
            // Closing a string's `{...}` goes back into the string.
            (false, ')' | ']' | '}') => in_string = stack.pop() == Some('"'),
            _ => {}
        }
    }
    stack
        .iter()
        .rev()
        .map(|c| match c {
            // Inside a string's `{...}`: close it and the string.
            '"' => "}\"".to_string(),
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::closers;

    #[test]
    fn closes_what_is_open() {
        assert_eq!(closers("    print(len(xs[0"), "]))");
        assert_eq!(closers("    print(\"a {b."), "}\")");
        assert_eq!(closers("    f(\"(\", "), ")");
        assert_eq!(closers("    if x {"), "}");
    }
}
