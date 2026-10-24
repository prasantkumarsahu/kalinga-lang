//! "did you mean ...?" for misspelt names.

use kalinga_syntax::error::{Diagnostic, Result, Span};

/// The candidate closest to `name` when it is close enough to be a typo: at most a third
/// of the characters differ, ignoring case (so a name of one or two letters only matches
/// in another case). Ties go to the first.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let limit = name.chars().count() / 3;
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        // Packages' clashing names are renamed `Name__unit`; compare what is written.
        let written = c.split("__").next().unwrap_or(c);
        if written == name || written.is_empty() {
            continue;
        }
        let d = distance(&name.to_lowercase(), &written.to_lowercase());
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, written));
        }
    }
    best.map(|(_, c)| c)
}

/// An error `message` about the unknown `name`, saying "did you mean `x`?" when one of
/// the `candidates` is close.
pub fn unknown<'a, T>(
    message: impl Into<String>,
    pos: Span,
    name: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Result<T> {
    let d = Diagnostic::error(message, pos);
    Err(match closest(name, candidates) {
        Some(c) => d.with_help(format!("did you mean `{}`?", c)),
        None => d,
    })
}

/// Edits (insert, delete, replace, or swap two neighbours) that turn `a` into `b`.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // d[i][j]: edits between a[..i] and b[..j]
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::{closest, distance};

    #[test]
    fn typos() {
        assert_eq!(distance("lenght", "length"), 1);
        assert_eq!(closest("spek", ["speak", "fetch"]), Some("speak"));
        assert_eq!(closest("lenght", ["len", "length"]), Some("length"));
        assert_eq!(closest("Cout", ["count"]), Some("count"));
        assert_eq!(closest("x", ["y"]), None, "short names are not guessed at");
        assert_eq!(closest("X", ["x"]), Some("x"));
        assert_eq!(closest("zebra", ["apple", "speak"]), None);
        assert_eq!(closest("draw", ["draw"]), None, "the same name is no suggestion");
        assert_eq!(closest("Vec", ["Vec2__mathx"]), Some("Vec2"));
    }
}
