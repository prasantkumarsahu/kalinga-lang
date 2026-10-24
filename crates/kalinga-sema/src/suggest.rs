//! "did you mean ...?" for misspelt names.

use kalinga_syntax::error::{Diagnostic, Result, Span};

/// Names other languages use, and what Kalinga calls the same thing.
const OTHER_LANGUAGES: &[(&str, &str)] = &[
    ("length", "len"),
    ("size", "len"),
    ("count", "len"),
    ("toUpperCase", "upper"),
    ("toLowerCase", "lower"),
    ("strip", "trim"),
    ("includes", "contains"),
    ("indexOf", "find"),
    ("puts", "println"),
];

/// The candidate `name` most likely meant, or `None`:
/// - one close enough to be a typo: at most a third of the characters differ, ignoring
///   case (so a name of one or two letters only matches in another case); ties go to the
///   first;
/// - else the same name in snake_case (`startsWith` → `starts_with`);
/// - else what Kalinga calls a name from another language (`length` → `len`).
///
/// Only candidates are ever suggested.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    // Packages' clashing names are renamed `Name__unit`; compare what is written.
    let written: Vec<&str> = candidates
        .into_iter()
        .map(|c| c.split("__").next().unwrap_or(c))
        .filter(|w| *w != name && !w.is_empty())
        .collect();
    let limit = name.chars().count() / 3;
    let mut best: Option<(usize, &str)> = None;
    for &w in &written {
        let d = distance(&name.to_lowercase(), &w.to_lowercase());
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, w));
        }
    }
    if let Some((_, w)) = best {
        return Some(w);
    }
    let snake = snake_case(name);
    if let Some(&w) = written.iter().find(|w| w.to_lowercase() == snake) {
        return Some(w);
    }
    let (_, kalinga) = OTHER_LANGUAGES.iter().find(|(other, _)| *other == name)?;
    written.into_iter().find(|w| w == kalinga)
}

/// `startsWith` → `starts_with`.
fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
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

    #[test]
    fn names_from_other_languages() {
        assert_eq!(closest("startsWith", ["len", "starts_with"]), Some("starts_with"));
        assert_eq!(closest("parseInt", ["parse_int", "parse_float"]), Some("parse_int"));
        assert_eq!(closest("isDone", ["join", "is_done"]), Some("is_done"));
        assert_eq!(closest("length", ["len", "upper"]), Some("len"));
        assert_eq!(closest("size", ["len"]), Some("len"));
        assert_eq!(closest("toUpperCase", ["upper", "lower"]), Some("upper"));
        assert_eq!(closest("indexOf", ["find"]), Some("find"));
        assert_eq!(closest("size", ["width", "height"]), None, "only a name the type has");
        assert_eq!(closest("lenght", ["length", "len"]), Some("length"), "a typo wins");
    }
}
