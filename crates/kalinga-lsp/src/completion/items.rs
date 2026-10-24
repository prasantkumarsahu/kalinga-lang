//! The completion items for each kind of place.

use std::collections::HashSet;

use kalinga_sema::builtins::{self, CONSTANTS, FUNCTIONS, LIBRARIES};
use kalinga_sema::index::{Symbol, registry_receiver};
use kalinga_sema::{Model, Signature, Type, subst_sig};
use lsp_types::{CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind};

use super::context::Opener;
use crate::describe::{params_and_ret, variant};
use crate::snapshot::Snapshot;

/// A completion item; `snippet` is inserted instead of the label when given.
pub fn item(label: &str, kind: CompletionItemKind, detail: Option<String>, snippet: Option<String>) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail,
        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
        insert_text: snippet,
        ..CompletionItem::default()
    }
}

fn with_docs(mut it: CompletionItem, docs: Option<String>) -> CompletionItem {
    it.documentation =
        docs.map(|value| Documentation::MarkupContent(MarkupContent { kind: MarkupKind::Markdown, value }));
    it
}

/// `name(${1:a}, ${2:b})`
pub fn call_snippet(name: &str, params: &[String]) -> String {
    let ps: Vec<String> = params.iter().enumerate().map(|(i, p)| format!("${{{}:{}}}", i + 1, p)).collect();
    format!("{}({})", name, ps.join(", "))
}

fn param_names(sig: &Signature) -> Vec<String> {
    sig.params.iter().map(|(n, _)| n.clone()).collect()
}

fn member_docs(snap: &Snapshot, owner: &str, name: &str) -> Option<String> {
    let d = snap.decl(&Symbol::Member { owner: owner.to_string(), name: name.to_string() })?;
    snap.docs(d.start)
}

fn method_item(snap: &Snapshot, owner: &str, name: &str, sig: &Signature) -> CompletionItem {
    let detail = snap.show(&format!("{}.{}{}", owner, name, params_and_ret(sig)));
    let it = item(name, CompletionItemKind::METHOD, Some(detail), Some(call_snippet(name, &param_names(sig))));
    with_docs(it, member_docs(snap, owner, name))
}

fn field_item(snap: &Snapshot, owner: &str, name: &str, ty: &Type) -> CompletionItem {
    let detail = snap.show(&format!("{}.{}: {}", owner, name, ty));
    with_docs(item(name, CompletionItemKind::FIELD, Some(detail), None), member_docs(snap, owner, name))
}

/// The members of a value of type `ty`; `on_self` shows private ones of its own type too.
pub fn members(snap: &Snapshot, ty: &Type, on_self: bool) -> Vec<CompletionItem> {
    let Some(m) = snap.model() else { return vec![] };
    match ty {
        Type::Class(c) => class_members(snap, m, c, &[], on_self),
        Type::Generic(c, targs) => class_members(snap, m, c, targs, on_self),
        // With a generic interface's type arguments filled in.
        Type::Interface(i, args) => m
            .iface_closure(i)
            .iter()
            .flat_map(|iface| {
                let map = m.iface_bindings(iface, &m.iface_args(i, args, iface).unwrap_or_default());
                let methods = m.interfaces[iface].methods.iter();
                methods.map(move |(n, sig)| method_item(snap, iface, n, &subst_sig(sig, &map))).collect::<Vec<_>>()
            })
            .collect(),
        Type::Struct(st, targs) => struct_members(snap, m, st, targs, on_self),
        other => match registry_receiver(other) {
            Some(r) => builtins::methods_of(r)
                .map(|meth| with_docs(builtin_method_item(meth), Some(meth.doc.to_string())))
                .collect(),
            None => vec![],
        },
    }
}

fn builtin_method_item(meth: &builtins::Method) -> CompletionItem {
    let names: Vec<String> = meth.params.iter().map(|p| p.split(':').next().unwrap_or(p).trim().to_string()).collect();
    let detail = format!("{}.{}", meth.receiver, meth.signature());
    item(meth.name, CompletionItemKind::METHOD, Some(detail), Some(call_snippet(meth.name, &names)))
}

fn class_members(snap: &Snapshot, m: &Model, c: &str, targs: &[Type], on_self: bool) -> Vec<CompletionItem> {
    let bindings = m.bindings(c, targs);
    let mut out = vec![];
    for (f, t, owner) in m.all_fields(c) {
        if !m.classes[&owner].private_fields.contains(&f) || (on_self && owner == c) {
            out.push(field_item(snap, &owner, &f, &kalinga_sema::subst(&t, &bindings)));
        }
    }
    let mut seen = HashSet::new();
    for a in m.ancestors(c) {
        let mut names: Vec<&String> = m.classes[&a].methods.keys().collect();
        names.sort();
        for name in names {
            let Some((owner, sig)) = m.find_method(c, name) else { continue };
            let private = m.classes[&owner].private_methods.contains(name);
            if seen.insert(name.clone()) && (!private || (on_self && owner == c)) {
                out.push(method_item(snap, &owner, name, &subst_sig(&sig, &bindings)));
            }
        }
    }
    out
}

fn struct_members(snap: &Snapshot, m: &Model, st: &str, targs: &[Type], on_self: bool) -> Vec<CompletionItem> {
    let info = &m.structs[st];
    let bindings = m.struct_bindings(st, targs);
    let mut out: Vec<CompletionItem> = info
        .fields
        .iter()
        .filter(|(f, _)| on_self || !info.private_fields.contains(f))
        .map(|(f, t)| field_item(snap, st, f, &kalinga_sema::subst(t, &bindings)))
        .collect();
    let mut names: Vec<&String> = info.methods.keys().collect();
    names.sort();
    for name in names {
        let meth = &info.methods[name];
        if !meth.is_static && (on_self || !meth.is_private) {
            out.push(method_item(snap, st, name, &subst_sig(&meth.sig, &bindings)));
        }
    }
    out
}

/// After `Type.`: an enum's variants, or the `static fn`s, constants and static variables
/// of a class or struct.
pub fn type_members(snap: &Snapshot, name: &str) -> Vec<CompletionItem> {
    let Some(m) = snap.model() else { return vec![] };
    if let Some(e) = m.enums.get(name) {
        return e.variants.iter().map(|(v, fs)| variant_item(snap, name, v, fs)).collect();
    }
    let mut out = vec![];
    if let Some(c) = m.classes.get(name) {
        let mut statics: Vec<(&String, &Signature)> = c.statics.iter().collect();
        statics.sort_by_key(|(n, _)| *n);
        out.extend(statics.into_iter().map(|(n, sig)| method_item(snap, name, n, sig)));
    }
    if let Some(s) = m.structs.get(name) {
        let mut statics: Vec<(&String, _)> = s.methods.iter().filter(|(_, meth)| meth.is_static).collect();
        statics.sort_by_key(|(n, _)| *n);
        out.extend(statics.into_iter().map(|(n, meth)| method_item(snap, name, n, &meth.sig)));
    }
    for (n, s) in m.static_values_of(name).unwrap_or_default() {
        let kind = if s.mutable { CompletionItemKind::VARIABLE } else { CompletionItemKind::CONSTANT };
        let detail = snap.show(&format!("{}.{}: {}", name, n, s.ty));
        out.push(with_docs(item(n, kind, Some(detail), None), member_docs(snap, name, n)));
    }
    out
}

fn variant_item(snap: &Snapshot, enum_name: &str, v: &str, fields: &[(String, Type)]) -> CompletionItem {
    let detail = snap.show(&format!("{}.{}", enum_name, variant(v, fields)));
    let names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
    let snippet = (!fields.is_empty()).then(|| call_snippet(v, &names));
    with_docs(item(v, CompletionItemKind::ENUM_MEMBER, Some(detail), snippet), member_docs(snap, enum_name, v))
}

/// The patterns a `match` on a value of type `ty` can use.
pub fn patterns(snap: &Snapshot, ty: &Type) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = match ty {
        Type::Enum(e, _) => snap
            .model()
            .and_then(|m| m.enums.get(e))
            .map_or_else(Vec::new, |info| info.variants.iter().map(|(v, fs)| variant_item(snap, e, v, fs)).collect()),
        Type::Result(..) => {
            ["Ok", "Err"].iter().map(|v| item(v, CompletionItemKind::ENUM_MEMBER, None, None)).collect()
        }
        Type::Bool => ["true", "false"].iter().map(|v| item(v, CompletionItemKind::KEYWORD, None, None)).collect(),
        _ => vec![],
    };
    out.push(item("_", CompletionItemKind::KEYWORD, Some("matches anything else".into()), Some("_ => ".into())));
    out
}

/// `Math.`, `Random.`, ...: the library's functions and constants.
pub fn library(lib: &str) -> Vec<CompletionItem> {
    let prefix = format!("{}.", lib);
    let functions = FUNCTIONS.iter().filter_map(|f| {
        let name = f.name.strip_prefix(&prefix)?;
        let names: Vec<String> = f.params.iter().map(|p| p.split(':').next().unwrap_or(p).trim().to_string()).collect();
        let it = item(name, CompletionItemKind::FUNCTION, Some(f.signature()), Some(call_snippet(name, &names)));
        Some(with_docs(it, Some(f.doc.to_string())))
    });
    let constants = CONSTANTS.iter().filter_map(|c| {
        let name = c.name.strip_prefix(&prefix)?;
        let it = item(name, CompletionItemKind::CONSTANT, Some(format!("{}: {}", c.name, c.ty)), None);
        Some(with_docs(it, Some(c.doc.to_string())))
    });
    functions.chain(constants).collect()
}

/// Is `symbol` declared in one of `units`?
fn declared_in(snap: &Snapshot, symbol: &Symbol, units: &HashSet<String>) -> bool {
    snap.decl(symbol).is_some_and(|d| snap.unit_of(d).is_some_and(|u| units.contains(u)))
}

/// The top-level declarations of the program visible from the analysed file (with
/// `units`), or of one package (for `alias.`).
pub fn globals(snap: &Snapshot, units: Option<&HashSet<String>>) -> Vec<CompletionItem> {
    let Some(m) = snap.model() else { return vec![] };
    let visible = units.cloned().unwrap_or_else(|| snap.visible_units());
    let shown = |name: &str| snap.show(name).rsplit('.').next().unwrap_or(name).to_string();
    let docs = |symbol: &Symbol| snap.decl(symbol).and_then(|d| snap.docs(d.start));
    let mut out = vec![];
    let mut functions: Vec<(&String, &Signature)> =
        m.functions.iter().chain(m.extern_fns.iter().map(|(n, (sig, _))| (n, sig))).collect();
    functions.sort_by_key(|(n, _)| *n);
    for (name, sig) in functions {
        let symbol = Symbol::Function(name.clone());
        if name == "main" || !declared_in(snap, &symbol, &visible) {
            continue;
        }
        let label = shown(name);
        let detail = snap.show(&format!("{}{}", label, params_and_ret(sig)));
        let it =
            item(&label, CompletionItemKind::FUNCTION, Some(detail), Some(call_snippet(&label, &param_names(sig))));
        out.push(with_docs(it, docs(&symbol)));
    }
    let mut types: Vec<&String> =
        m.classes.keys().chain(m.structs.keys()).chain(m.enums.keys()).chain(m.interfaces.keys()).collect();
    types.extend(m.names.externs.iter());
    types.sort();
    for name in types {
        let symbol = Symbol::Type(name.clone());
        if declared_in(snap, &symbol, &visible) {
            out.push(with_docs(type_item(m, &shown(name), name), docs(&symbol)));
        }
    }
    let mut consts: Vec<(&String, &Type)> = m.consts.iter().collect();
    consts.sort_by_key(|(n, _)| *n);
    for (name, t) in consts {
        let symbol = Symbol::Const(name.clone());
        if declared_in(snap, &symbol, &visible) {
            let label = shown(name);
            let detail = snap.show(&format!("const {}: {}", label, t));
            out.push(with_docs(item(&label, CompletionItemKind::CONSTANT, Some(detail), None), docs(&symbol)));
        }
    }
    out
}

/// A type, inserted as a constructor call when it is one.
fn type_item(m: &Model, label: &str, name: &str) -> CompletionItem {
    if let Some(c) = m.classes.get(name) {
        let init = m.find_init(name).map(|(_, s)| param_names(&s)).unwrap_or_default();
        let snippet = (!c.decl.is_abstract).then(|| call_snippet(label, &init));
        return item(label, CompletionItemKind::CLASS, Some(format!("class {}", label)), snippet);
    }
    if let Some(s) = m.structs.get(name) {
        let fields: Vec<String> = s.fields.iter().map(|(n, _)| n.clone()).collect();
        return item(
            label,
            CompletionItemKind::STRUCT,
            Some(format!("struct {}", label)),
            Some(call_snippet(label, &fields)),
        );
    }
    if m.enums.contains_key(name) {
        return item(label, CompletionItemKind::ENUM, Some(format!("enum {}", label)), None);
    }
    if m.interfaces.contains_key(name) {
        return item(label, CompletionItemKind::INTERFACE, Some(format!("interface {}", label)), None);
    }
    item(label, CompletionItemKind::TYPE_PARAMETER, Some(format!("extern type {}", label)), None)
}

/// Built-in functions, libraries and keywords that start expressions and statements.
pub fn language() -> Vec<CompletionItem> {
    let functions = FUNCTIONS.iter().filter(|f| !f.name.contains('.')).map(|f| {
        let names: Vec<String> = f.params.iter().map(|p| p.split(':').next().unwrap_or(p).trim().to_string()).collect();
        let names = if names == ["values..."] { vec![] } else { names };
        let snippet =
            if f.name == "Channel" { "Channel<${1:int}>()".to_string() } else { call_snippet(f.name, &names) };
        with_docs(
            item(f.name, CompletionItemKind::FUNCTION, Some(f.signature()), Some(snippet)),
            Some(f.doc.to_string()),
        )
    });
    let libraries =
        LIBRARIES.iter().map(|l| item(l, CompletionItemKind::MODULE, Some(format!("built-in library {}", l)), None));
    let keywords = [
        "if", "else", "while", "for", "match", "let", "var", "return", "break", "continue", "await", "true", "false",
        "none", "fn",
    ]
    .iter()
    .map(|k| item(k, CompletionItemKind::KEYWORD, None, None));
    functions.chain(libraries).chain(keywords).collect()
}

/// Where a type is written: the built-in types and the program's.
pub fn types(snap: &Snapshot) -> Vec<CompletionItem> {
    let builtin = ["int", "float", "bool", "str", "void", "Result", "Channel", "Shared", "Task", "Async"];
    let mut out: Vec<CompletionItem> = builtin
        .iter()
        .map(|t| item(t, CompletionItemKind::TYPE_PARAMETER, Some("built-in type".into()), None))
        .collect();
    out.extend(globals(snap, None).into_iter().filter(|i| is_type_kind(i.kind)).map(|mut i| {
        i.insert_text = None;
        i.insert_text_format = None;
        i
    }));
    out
}

fn is_type_kind(kind: Option<CompletionItemKind>) -> bool {
    matches!(
        kind,
        Some(
            CompletionItemKind::CLASS
                | CompletionItemKind::STRUCT
                | CompletionItemKind::ENUM
                | CompletionItemKind::INTERFACE
                | CompletionItemKind::TYPE_PARAMETER
        )
    )
}

/// After `class Name :`: the classes and interfaces it can extend or implement.
pub fn bases(snap: &Snapshot) -> Vec<CompletionItem> {
    types(snap)
        .into_iter()
        .filter(|i| matches!(i.kind, Some(CompletionItemKind::CLASS | CompletionItemKind::INTERFACE)))
        .collect()
}

/// At the start of a line at the top level.
pub fn top_level() -> Vec<CompletionItem> {
    let snippets = [
        ("fn", "fn ${1:name}(${2}) {\n\t$0\n}"),
        ("test fn", "test fn ${1:name}() {\n\tassert_eq(${2:value}, ${3:expected})$0\n}"),
        ("async fn", "async fn ${1:name}(${2}) -> ${3:int} {\n\t$0\n}"),
        ("class", "class ${1:Name} {\n\t$0\n}"),
        ("abstract class", "abstract class ${1:Name} {\n\t$0\n}"),
        ("struct", "struct ${1:Name} {\n\t$0\n}"),
        ("interface", "interface ${1:Name} {\n\t$0\n}"),
        ("enum", "enum ${1:Name} {\n\t$0\n}"),
        ("const", "const ${1:NAME}: ${2:int} = $0"),
        ("import", "import \"${1:file.kl}\""),
        ("import as", "import \"${1:package}\" as ${2:name}"),
        ("use rust", "use rust \"${1:native/file.rs}\""),
        ("use crate", "use crate \"${1:name}\" = \"${2:1}\""),
        ("extern fn", "extern fn ${1:name}(${2}) -> ${3:int}"),
        ("extern type", "extern type ${1:Name}"),
    ];
    snippets.iter().map(|(label, s)| item(label, CompletionItemKind::KEYWORD, None, Some(s.to_string()))).collect()
}

/// At the start of a line in a class, struct or interface body: member keywords, and for
/// a class, `override fn` stubs for what it inherits (required ones first).
pub fn body(snap: &Snapshot, opener: &Opener, written: &HashSet<String>) -> Vec<CompletionItem> {
    let kw = |label: &str, snippet: &str| item(label, CompletionItemKind::KEYWORD, None, Some(snippet.to_string()));
    match opener {
        Opener::Interface => vec![kw("fn", "fn ${1:name}(${2}) -> ${3:void}")],
        Opener::Struct(_) => {
            let mut out = vec![
                kw("fn", "fn ${1:name}(${2}) -> ${3:float} {\n\t$0\n}"),
                kw("mut fn", "mut fn ${1:name}(${2}) {\n\t$0\n}"),
                kw("static fn", "static fn ${1:name}(${2}) {\n\t$0\n}"),
                kw("private", "private "),
                kw("const", "const ${1:NAME}: ${2:int} = $0"),
                kw("static var", "static var ${1:name}: ${2:int} = $0"),
            ];
            out.extend(["+", "-", "*", "/", "%"].iter().map(|op| {
                kw(
                    &format!("op {}", op),
                    &format!("op {}(${{1:other}}: ${{2:Self}}) -> ${{3:Self}} {{\n\treturn $0\n}}", op),
                )
            }));
            out
        }
        Opener::Class { bases, is_abstract, .. } => {
            let mut out = vec![
                kw("fn", "fn ${1:name}(${2}) {\n\t$0\n}"),
                kw("init", "init(${1}) {\n\t$0\n}"),
                kw("private", "private "),
                kw("static fn", "static fn ${1:name}(${2}) {\n\t$0\n}"),
                kw("static async fn", "static async fn ${1:name}(${2}) -> ${3:int} {\n\t$0\n}"),
                kw("const", "const ${1:NAME}: ${2:int} = $0"),
                kw("static var", "static var ${1:name}: ${2:int} = $0"),
            ];
            if *is_abstract {
                out.push(kw("abstract fn", "abstract fn ${1:name}(${2}) -> ${3:void}"));
            }
            out.extend(overrides(snap, bases, written));
            out
        }
        Opener::Enum | Opener::Match(_) | Opener::Block => vec![],
    }
}

/// `override fn` stubs for the methods of `bases` (and the interfaces above them) that
/// are not `written` yet; abstract and interface methods first.
fn overrides(snap: &Snapshot, bases: &[String], written: &HashSet<String>) -> Vec<CompletionItem> {
    let Some(m) = snap.model() else { return vec![] };
    // (name, signature, must be written)
    let mut candidates: Vec<(String, Signature, bool)> = vec![];
    let mut add = |name: &String, sig: &Signature, required: bool| {
        if !written.contains(name) && !candidates.iter().any(|(n, _, _)| n == name) {
            candidates.push((name.clone(), sig.clone(), required));
        }
    };
    for base in bases {
        if m.classes.contains_key(base) {
            for a in m.ancestors(base) {
                let info = &m.classes[&a];
                let mut names: Vec<&String> = info.methods.keys().collect();
                names.sort();
                for name in names.into_iter().filter(|n| !info.private_methods.contains(*n)) {
                    add(name, &info.methods[name], info.abstract_methods.contains(name));
                }
            }
            for i in m.all_interfaces(base) {
                m.iface_closure(&i)
                    .iter()
                    .for_each(|i| m.interfaces[i].methods.iter().for_each(|(n, s)| add(n, s, true)));
            }
        }
        if m.interfaces.contains_key(base) {
            m.iface_closure(base)
                .iter()
                .for_each(|i| m.interfaces[i].methods.iter().for_each(|(n, s)| add(n, s, true)));
        }
    }
    candidates
        .into_iter()
        .map(|(name, sig, required)| {
            let header = snap.show(&format!("override fn {}{}", name, params_and_ret(&sig)));
            let mut it = item(
                &format!("override fn {}", name),
                CompletionItemKind::METHOD,
                Some(header.clone()),
                Some(format!("{} {{\n\t$0\n}}", header)),
            );
            it.sort_text = Some(format!("{}{}", if required { "0" } else { "1" }, name));
            it.filter_text = Some(name);
            it
        })
        .collect()
}

/// After `alias.` for `import "package" as alias`: the package's declarations.
pub fn package(snap: &Snapshot, alias: &str) -> Vec<CompletionItem> {
    let Some(unit) = snap.analysis.units.file_aliases.first().and_then(|a| a.get(alias)) else { return vec![] };
    globals(snap, Some(&HashSet::from([unit.clone()])))
}
