//! How symbols are shown: their signature as Kalinga code, and their `///` docs.

use kalinga_sema::builtins::{self, CONSTANTS, FUNCTIONS};
use kalinga_sema::index::Symbol;
use kalinga_sema::{Model, Signature, Type};

use crate::snapshot::Snapshot;

/// A symbol as an editor shows it.
pub struct Description {
    /// The declaration, as Kalinga code: `fn double(n: int) -> int`.
    pub code: String,
    /// The type or library it belongs to, for members.
    pub owner: Option<String>,
    pub docs: Option<String>,
}

pub fn describe(snap: &Snapshot, symbol: &Symbol, ty: Option<&Type>) -> Option<Description> {
    let user_docs = || snap.decl(symbol).and_then(|d| snap.docs(d.start));
    let d = match symbol {
        Symbol::Local { name, mutable, .. } => {
            let kw = if *mutable { "var" } else { "let" };
            let code = match ty {
                Some(t) => format!("{} {}: {}", kw, name, t),
                None => format!("{} {}", kw, name),
            };
            Description { code, owner: None, docs: None }
        }
        Symbol::Builtin { receiver, name } => builtin(*receiver, name)?,
        Symbol::Library { lib, name } => library(lib, name)?,
        Symbol::Function(name) => Description { code: function(snap.model()?, name)?, owner: None, docs: user_docs() },
        Symbol::Const(name) => {
            let t = snap.model()?.consts.get(name)?;
            Description { code: format!("const {}: {}", name, t), owner: None, docs: user_docs() }
        }
        Symbol::Type(name) => Description { code: type_decl(snap.model()?, name)?, owner: None, docs: user_docs() },
        Symbol::Member { owner, name } => {
            let m = snap.model()?;
            Description { code: member(m, owner, name)?, owner: Some(owner.clone()), docs: user_docs() }
        }
    };
    Some(Description { code: snap.show(&d.code), owner: d.owner.map(|o| snap.show(&o)), docs: d.docs })
}

/// `(a: int, b: str) -> int`
pub fn params_and_ret(sig: &Signature) -> String {
    let ps: Vec<String> = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t)).collect();
    format!("({}){}", ps.join(", "), ret_suffix(&sig.ret))
}

fn ret_suffix(t: &Type) -> String {
    if *t == Type::Void { String::new() } else { format!(" -> {}", t) }
}

fn function(m: &Model, name: &str) -> Option<String> {
    if let Some((sig, _)) = m.extern_fns.get(name) {
        return Some(format!("extern fn {}{}", name, params_and_ret(sig)));
    }
    let sig = m.functions.get(name)?;
    let tps =
        m.fn_tparams.get(name).filter(|t| !t.is_empty()).map(|t| format!("<{}>", t.join(", "))).unwrap_or_default();
    let kw = if m.async_fns.contains(name) { "async fn" } else { "fn" };
    Some(format!("{} {}{}{}", kw, name, tps, params_and_ret(sig)))
}

fn type_params(tps: &[String]) -> String {
    if tps.is_empty() { String::new() } else { format!("<{}>", tps.join(", ")) }
}

fn type_decl(m: &Model, name: &str) -> Option<String> {
    if let Some(c) = m.classes.get(name) {
        let kw = if c.decl.is_abstract { "abstract class" } else { "class" };
        let bases: Vec<&str> = c.decl.bases.iter().map(|(b, _)| b.as_str()).collect();
        let header = if bases.is_empty() { String::new() } else { format!(" : {}", bases.join(", ")) };
        let mut code = format!("{} {}{}{}", kw, name, type_params(&c.tparams), header);
        if let Some((_, init)) = m.find_init(name) {
            code.push_str(&format!("\n    init{}", params_and_ret(&init)));
        }
        return Some(code);
    }
    if let Some(s) = m.structs.get(name) {
        let fields: Vec<String> = s.fields.iter().map(|(n, t)| format!("{}: {}", n, t)).collect();
        return Some(format!("struct {}{}({})", name, type_params(&s.tparams), fields.join(", ")));
    }
    if let Some(i) = m.interfaces.get(name) {
        let methods: Vec<String> =
            i.methods.iter().map(|(n, sig)| format!("\n    fn {}{}", n, params_and_ret(sig))).collect();
        let supers = if i.supers.is_empty() { String::new() } else { format!(" : {}", i.supers.join(", ")) };
        return Some(format!("interface {}{} {{{}\n}}", name, supers, methods.concat()));
    }
    if let Some(e) = m.enums.get(name) {
        let variants: Vec<String> = e.variants.iter().map(|(v, fs)| format!("\n    {}", variant(v, fs))).collect();
        return Some(format!("enum {} {{{}\n}}", name, variants.concat()));
    }
    m.names.externs.contains(name).then(|| format!("extern type {}", name))
}

/// `Right(steps: int)`, or `Left` for a variant without values.
pub fn variant(name: &str, fields: &[(String, Type)]) -> String {
    if fields.is_empty() {
        return name.to_string();
    }
    let fs: Vec<String> = fields.iter().map(|(n, t)| format!("{}: {}", n, t)).collect();
    format!("{}({})", name, fs.join(", "))
}

fn member(m: &Model, owner: &str, name: &str) -> Option<String> {
    if let Some(e) = m.enums.get(owner) {
        return e.variant(name).map(|fs| format!("{}.{}", owner, variant(name, fs)));
    }
    if let Some(i) = m.interfaces.get(owner) {
        return i.methods.iter().find(|(n, _)| n == name).map(|(_, sig)| format!("fn {}{}", name, params_and_ret(sig)));
    }
    if let Some(s) = m.static_value(owner, name) {
        let kw = if s.mutable { "static var" } else { "const" };
        return Some(format!("{} {}: {}", kw, name, s.ty));
    }
    if let Some(c) = m.classes.get(owner) {
        if let Some((_, t)) = c.fields.iter().find(|(n, _)| n == name) {
            return Some(format!("{}: {}", name, t));
        }
        if let Some(sig) = c.methods.get(name) {
            return Some(format!("fn {}{}", name, params_and_ret(sig)));
        }
        let kw = if c.async_statics.contains(name) { "static async fn" } else { "static fn" };
        return c.statics.get(name).map(|sig| format!("{} {}{}", kw, name, params_and_ret(sig)));
    }
    let s = m.structs.get(owner)?;
    if let Some((_, t)) = s.fields.iter().find(|(n, _)| n == name) {
        return Some(format!("{}: {}", name, t));
    }
    let meth = s.methods.get(name)?;
    let kw = match (meth.is_static, meth.is_mut) {
        (true, _) => "static fn",
        (false, true) => "mut fn",
        (false, false) => "fn",
    };
    Some(format!("{} {}{}", kw, name, params_and_ret(&meth.sig)))
}

fn builtin(receiver: Option<&'static str>, name: &str) -> Option<Description> {
    let (code, doc) = match receiver {
        None => {
            let f = builtins::function(name)?;
            (format!("fn {}", f.signature()), f.doc)
        }
        Some(r) => {
            let m = builtins::methods_of(r).find(|m| m.name == name)?;
            (format!("fn {}", m.signature()), m.doc)
        }
    };
    Some(Description { code, owner: receiver.map(str::to_string), docs: Some(doc.to_string()) })
}

fn library(lib: &str, name: &str) -> Option<Description> {
    let full = format!("{}.{}", lib, name);
    if let Some(c) = CONSTANTS.iter().find(|c| c.name == full) {
        return Some(Description { code: format!("const {}: {}", full, c.ty), owner: None, docs: Some(c.doc.into()) });
    }
    let f = FUNCTIONS.iter().find(|f| f.name == full)?;
    Some(Description { code: format!("fn {}", f.signature()), owner: None, docs: Some(f.doc.into()) })
}
