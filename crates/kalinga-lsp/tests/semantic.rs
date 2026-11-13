//! Semantic tokens: each name coloured by what it is.

mod common;

use lsp_types::*;

/// (text, type, modifiers) of every token in `src`.
fn tokens(src: &str) -> Vec<(String, String, Vec<String>)> {
    let (mut world, uri) = common::open(&std::env::temp_dir(), src);
    let params = SemanticTokensParams {
        text_document: TextDocumentIdentifier { uri },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let Some(SemanticTokensResult::Tokens(t)) = kalinga_lsp::semantic::full(&mut world, params) else {
        panic!("no tokens")
    };
    let legend = kalinga_lsp::semantic::legend();
    let lines: Vec<&str> = src.lines().collect();
    let (mut line, mut col) = (0u32, 0u32);
    t.data
        .iter()
        .map(|tok| {
            line += tok.delta_line;
            col = if tok.delta_line == 0 { col + tok.delta_start } else { tok.delta_start };
            let text: String = lines[line as usize].chars().skip(col as usize).take(tok.length as usize).collect();
            let mods = (0..legend.token_modifiers.len())
                .filter(|b| tok.token_modifiers_bitset & (1 << b) != 0)
                .map(|b| legend.token_modifiers[b].as_str().to_string())
                .collect();
            (text, legend.token_types[tok.token_type as usize].as_str().to_string(), mods)
        })
        .collect()
}

fn kind_of(toks: &[(String, String, Vec<String>)], text: &str) -> Vec<String> {
    toks.iter()
        .filter(|t| t.0 == text)
        .map(|t| format!("{}{}", t.1, t.2.iter().map(|m| format!(".{m}")).collect::<String>()))
        .collect()
}

#[test]
fn names_are_coloured_by_what_they_are() {
    let src = r#"enum Color {
    Red
    Green
}

struct Point {
    x: int
}

class User {
    static var count: int = 0
    name: str

    init(name: str) {
        self.name = name
    }

    fn greet(other: str) -> str {
        return self.name + other
    }
}

#[deprecated]
fn old() -> int {
    return 1
}

const LIMIT: int = 3

fn main() {
    let user = User("Ann")
    var total = len(user.greet("Bo")) + old() + LIMIT
    let double = fn(n: int) -> int { return n * 2 }
    total += double(Point(1).x)
    let c = Color.Red
    println("{total} {User.count} {c}")
}
"#;
    let t = tokens(src);
    assert_eq!(kind_of(&t, "Color"), ["enum.declaration", "enum"]);
    assert_eq!(kind_of(&t, "Red"), ["enumMember.declaration", "enumMember"]);
    assert_eq!(kind_of(&t, "Point"), ["struct.declaration", "struct"]);
    assert_eq!(kind_of(&t, "User")[0], "class.declaration");
    assert!(kind_of(&t, "count").contains(&"property.declaration.static".to_string()), "{:?}", kind_of(&t, "count"));
    assert!(kind_of(&t, "name").contains(&"parameter.declaration.readonly".to_string()), "{:?}", kind_of(&t, "name"));
    assert!(kind_of(&t, "name").contains(&"property".to_string()), "{:?}", kind_of(&t, "name"));
    assert_eq!(kind_of(&t, "greet"), ["method.declaration", "method"]);
    assert_eq!(kind_of(&t, "other"), ["parameter.declaration.readonly", "parameter.readonly"]);
    assert_eq!(kind_of(&t, "old"), ["function.declaration.deprecated", "function.deprecated"]);
    assert_eq!(kind_of(&t, "LIMIT"), ["variable.declaration.readonly.static", "variable.readonly.static"]);
    assert_eq!(kind_of(&t, "user"), ["variable.declaration.readonly", "variable.readonly"]);
    assert_eq!(kind_of(&t, "total")[0], "variable.declaration");
    assert_eq!(kind_of(&t, "n"), ["parameter.declaration.readonly", "parameter.readonly"]);
    assert_eq!(kind_of(&t, "len"), ["function.defaultLibrary"]);
    assert_eq!(kind_of(&t, "x"), ["property.declaration", "property"]);
}
