//! Go to definition, references, rename, the outline, quick fixes and diagnostics.

mod common;

use common::*;
use lsp_types::*;

const PROGRAM: &str = r#"interface Show {
    fn show() -> str
}

class Animal : Show {
    name: str
    init(name: str) {
        self.name = name
    }
    override fn show() -> str {
        return self.name
    }
}

class Dog : Animal {
    init(name: str) {
        super(name)
    }
    override fn show() -> str {
        return "dog"
    }
}

fn describe(s: Show) -> str {
    return s.show()
}

fn main() {
    var count = 1
    let d = Dog("rex")
    count += 1
    println(describe(d), d.show(), d.name, count)
}
"#;

/// The position of the `nth` occurrence of `word` in PROGRAM (on its first character).
fn pos_of(word: &str, nth: usize) -> Position {
    let at = PROGRAM.match_indices(word).nth(nth).map(|(i, _)| i).unwrap_or_else(|| panic!("{}", word));
    marked(&format!("{}|{}", &PROGRAM[..at], &PROGRAM[at..])).1
}

fn line_of(range: &Range) -> u32 {
    range.start.line
}

fn no_progress() -> (WorkDoneProgressParams, PartialResultParams) {
    (WorkDoneProgressParams::default(), PartialResultParams::default())
}

#[test]
fn definition_of_locals_methods_types_and_fields() {
    let (mut world, uri) = open(&temp_dir("nav-def"), PROGRAM);
    let mut def = |word: &str, nth: usize| {
        let (w, p) = no_progress();
        let params = GotoDefinitionParams {
            text_document_position_params: at(&uri, pos_of(word, nth)),
            work_done_progress_params: w,
            partial_result_params: p,
        };
        match kalinga_lsp::navigation::definition(&mut world, params) {
            Some(GotoDefinitionResponse::Scalar(loc)) => line_of(&loc.range),
            other => panic!("no definition for {}: {:?}", word, other),
        }
    };
    assert_eq!(def("count += 1", 0), 28, "a local goes to its `var`");
    assert_eq!(def("describe(d)", 0), 23);
    assert_eq!(def("Dog(\"rex\")", 0), 14);
    assert_eq!(def("name, count", 0), 5, "an inherited field goes to the base class");
    assert_eq!(def("show(), d.name", 0), 18, "a method call goes to the override that runs for a Dog");
}

#[test]
fn references_and_rename_follow_overrides() {
    let (mut world, uri) = open(&temp_dir("nav-refs"), PROGRAM);
    let (w, p) = no_progress();
    let params = ReferenceParams {
        text_document_position: at(&uri, pos_of("show() -> str\n}", 0)),
        work_done_progress_params: w,
        partial_result_params: p,
        context: ReferenceContext { include_declaration: true },
    };
    let refs = kalinga_lsp::navigation::references(&mut world, params).expect("references");
    let mut lines: Vec<u32> = refs.iter().map(|l| line_of(&l.range)).collect();
    lines.sort();
    // The interface method, both implementations and both calls.
    assert_eq!(lines, vec![1, 9, 18, 24, 31]);

    let rename = RenameParams {
        text_document_position: at(&uri, pos_of("show(), d.name", 0)),
        new_name: "display".into(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let edit = kalinga_lsp::navigation::rename(&mut world, rename).expect("rename");
    let edits = &edit.changes.expect("changes")[&uri];
    assert_eq!(edits.len(), 5);
    let mut text = PROGRAM.to_string();
    let mut sorted = edits.clone();
    sorted.sort_by_key(|e| std::cmp::Reverse((e.range.start.line, e.range.start.character)));
    for e in sorted {
        let start = offset_of(&text, e.range.start);
        let end = offset_of(&text, e.range.end);
        text.replace_range(start..end, &e.new_text);
    }
    assert!(!text.contains("show"), "{}", text);
    let renamed = kalinga_driver::analyze("main.kl", &text, &Default::default());
    assert!(renamed.diagnostics.is_empty(), "the renamed program still compiles: {:?}", renamed.diagnostics);
}

fn offset_of(text: &str, p: Position) -> usize {
    let line_start: usize = text.split_inclusive('\n').take(p.line as usize).map(str::len).sum();
    line_start + p.character as usize
}

#[test]
fn rename_of_a_local_and_bad_names() {
    let (mut world, uri) = open(&temp_dir("nav-rename"), PROGRAM);
    let mut rename = |word: &str, name: &str| {
        let params = RenameParams {
            text_document_position: at(&uri, pos_of(word, 0)),
            new_name: name.into(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        kalinga_lsp::navigation::rename(&mut world, params).and_then(|e| e.changes).map(|c| c[&uri].len())
    };
    assert_eq!(rename("count = 1", "total"), Some(3));
    assert_eq!(rename("count = 1", "match"), None, "keywords are not names");
    assert_eq!(rename("count = 1", "2x"), None);
    assert_eq!(rename("println", "say"), None, "built-ins cannot be renamed");
}

#[test]
fn outline_nests_members_in_their_types() {
    let (mut world, uri) = open(&temp_dir("nav-outline"), PROGRAM);
    let params = DocumentSymbolParams {
        text_document: TextDocumentIdentifier { uri },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let Some(DocumentSymbolResponse::Nested(top)) = kalinga_lsp::navigation::symbols(&mut world, params) else {
        panic!("no outline")
    };
    let names: Vec<&str> = top.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["describe", "main", "Animal", "Dog", "Show"]);
    let animal = top.iter().find(|s| s.name == "Animal").and_then(|s| s.children.as_ref()).expect("members");
    let members: Vec<&str> = animal.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(members, ["name", "show"]);
}

fn fixes(src: &str) -> Vec<(String, String)> {
    let (mut world, uri) = open(&temp_dir("nav-fix"), src);
    let diagnostics = world.diagnostics(&uri);
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range::default(),
        context: CodeActionContext { diagnostics, only: None, trigger_kind: None },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    kalinga_lsp::actions::actions(&mut world, params)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| match a {
            CodeActionOrCommand::CodeAction(a) => {
                let edit = a.edit?.changes?.remove(&uri)?.into_iter().next()?;
                Some((a.title, edit.new_text))
            }
            CodeActionOrCommand::Command(_) => None,
        })
        .collect()
}

#[test]
fn quick_fixes_for_override_and_missing_methods() {
    let game = "interface Game {\n    fn update(dt: float)\n    fn draw()\n}\n";
    let marks =
        fixes(&format!("{}class G : Game {{\n    fn update(dt: float) {{}}\n    override fn draw() {{}}\n}}\n", game));
    assert_eq!(marks, vec![("Mark it `override fn`".to_string(), "override ".to_string())]);
    let missing = fixes(&format!("{}class G : Game {{\n    override fn draw() {{}}\n}}\n", game));
    assert_eq!(
        missing,
        vec![("Add the missing method".to_string(), "\n    override fn update(dt: float) {\n    }\n".to_string())]
    );
    let extra = fixes("class A {\n    override fn f() {}\n}\n");
    assert_eq!(extra, vec![("Remove `override`".to_string(), String::new())]);
}

#[test]
fn diagnostics_cover_every_class_rule_and_body() {
    let src =
        "interface I {\n    fn f()\n}\nclass A : I {\n}\nclass B : I {\n}\nfn main() {\n    let x: int = \"s\"\n}\n";
    let (mut world, uri) = open(&temp_dir("nav-diag"), src);
    let lines: Vec<u32> = world.diagnostics(&uri).iter().map(|d| d.range.start.line).collect();
    assert_eq!(lines, vec![3, 5, 8], "both classes and the body are reported at once");
}

#[test]
fn errors_in_imported_files_show_on_the_import_line() {
    let dir = temp_dir("nav-import");
    std::fs::write(dir.join("lib.kl"), "fn helper() -> int {\n    return \"no\"\n}\n").expect("write");
    let (mut world, uri) = open(&dir, "import \"lib.kl\"\n\nfn main() {\n    println(helper())\n}\n");
    let d = world.diagnostics(&uri);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].range.start.line, 0);
    assert!(d[0].message.starts_with("in lib.kl:2:12: "), "{}", d[0].message);
}

#[test]
fn format_document() {
    let (mut world, uri) = open(&temp_dir("nav-format"), "fn main(){\nprintln( 1+2 )\n}\n");
    let params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri },
        options: FormattingOptions::default(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let edits = kalinga_lsp::formatting::format(&mut world, params).expect("edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "fn main() {\n    println(1 + 2)\n}\n");
}
