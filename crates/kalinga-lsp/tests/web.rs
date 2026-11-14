//! Web code in the editor: what a web build lacks is reported while typing, for files that
//! import `std/web` and for files of a project with `target = "web"`; `std/web` completes.

mod common;

use common::*;

fn messages(dir: &std::path::Path, text: &str) -> Vec<(u32, String)> {
    let (mut world, uri) = open(dir, text);
    world.diagnostics(&uri).into_iter().map(|d| (d.range.start.line, d.message)).collect()
}

const THREADS: &str = "fn main() {\n    t = Thread.spawn(fn() => 1)\n    println(t.join())\n}\n";

#[test]
fn files_that_import_std_web_are_checked_as_web_builds() {
    let dir = temp_dir("web-import");
    let web = messages(&dir, &format!("import \"std/web\"\n{}", THREADS));
    assert!(web.iter().any(|(line, m)| *line == 2 && m.contains("not available in a web build")), "{:?}", web);
    let native = messages(&dir, THREADS);
    assert!(native.is_empty(), "without std/web the file may be native: {:?}", native);
}

#[test]
fn a_project_target_decides() {
    let dir = temp_dir("web-project");
    std::fs::write(dir.join("utkal.toml"), "[package]\nname = \"site\"\ntarget = \"web\"\n").unwrap();
    let web = messages(&dir, "fn main() {\n    name = input()\n    println(name)\n}\n");
    assert!(web.iter().any(|(line, m)| *line == 1 && m.contains("input()")), "{:?}", web);

    std::fs::write(dir.join("utkal.toml"), "[package]\nname = \"tool\"\n").unwrap();
    let native = messages(&dir, "import \"std/web\"\n\nfn main() {\n    Document.app()\n}\n");
    assert!(
        native
            .iter()
            .any(|(line, m)| *line == 0 && m.contains("target = \\\"web\\\"") || m.contains("target = \"web\"")),
        "{:?}",
        native
    );
}

#[test]
fn std_web_names_complete() {
    let dir = temp_dir("web-complete");
    let program =
        |body: &str| format!("import \"std/web\"\n\nfn main() {{\n    el = Document.create(Tag.Div)\n{}\n}}\n", body);
    let kinds = labels(&complete_in(&dir, &program("    k = EventKind.|")));
    for k in ["Click", "Input", "KeyDown", "Other"] {
        assert!(kinds.contains(&k.to_string()), "{} in {:?}", k, kinds);
    }
    for body in ["    el.on(EventKind.|)", "    el.on(EventKind.|, fn(e: Event) {})", "    el.on(EventKind.|"] {
        let inside = labels(&complete_in(&dir, &program(body)));
        assert!(inside.contains(&"Click".to_string()), "{}: {:?}", body, inside);
    }
    // Where an enum is wanted, its variants come first, written in full.
    for body in ["    el.on(|)", "    el.on(|", "    x = Document.create(|)"] {
        let items = complete_in(&dir, &program(body));
        let first = items.iter().min_by_key(|i| i.sort_text.clone().unwrap_or_else(|| i.label.clone()));
        let want = if body.contains("create") { "Tag.Div" } else { "EventKind.Click" };
        assert_eq!(first.map(|i| i.label.as_str()), Some(want), "{}", body);
    }
    let other = complete_in(&dir, &program("    el.on(|)"));
    let o = other.iter().find(|i| i.label == "EventKind.Other").expect("Other");
    assert_eq!(o.insert_text.as_deref(), Some("EventKind.Other(${1:name})"));
    let tags = labels(&complete_in(&dir, &program("    x = Tag.|")));
    assert!(tags.contains(&"Input".to_string()) && tags.contains(&"Button".to_string()), "{:?}", tags);
    let methods = labels(&complete_in(&dir, &program("    el.|")));
    for m in ["on", "set_text", "add_class", "append"] {
        assert!(methods.contains(&m.to_string()), "{} in {:?}", m, methods);
    }
    let statics = labels(&complete_in(&dir, &program("    Document.|")));
    assert!(statics.contains(&"create".to_string()) && statics.contains(&"input".to_string()), "{:?}", statics);
}
