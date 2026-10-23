//! The declaration model of a program: its classes, interfaces, enums, structs and
//! functions with their resolved types, and the queries the type checker and code
//! generator ask of it (inheritance, method lookup, interfaces, printability, ...).

use std::collections::{HashMap, HashSet};

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use crate::types::{Signature, Type, is_map_key, subst};

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub decl: ClassDecl,
    pub parent: Option<String>,
    /// Interfaces listed directly after `:`.
    pub interfaces: Vec<String>,
    pub fields: Vec<(String, Type)>,
    pub methods: HashMap<String, Signature>,
    /// `static fn`s: not inherited through objects, called as `Class.name(...)`.
    pub statics: HashMap<String, Signature>,
    pub private_fields: HashSet<String>,
    pub private_methods: HashSet<String>,
    pub abstract_methods: HashSet<String>,
    /// `static async fn`s.
    pub async_statics: HashSet<String>,
    pub init: Option<Signature>,
    pub children: Vec<String>,
    /// Type parameters of a generic class (empty otherwise).
    pub tparams: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    /// Methods declared by this interface itself.
    pub methods: Vec<(String, Signature)>,
    /// Interfaces listed after `:`.
    pub supers: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EnumInfo {
    pub variants: Vec<(String, Vec<(String, Type)>)>,
    /// `(variant, field index)` of the fields that lead back to this enum by value
    /// (`Add(a: Expr, b: Expr)`); they are boxed in Rust so the enum has a finite size.
    pub boxed: HashSet<(String, usize)>,
}

impl EnumInfo {
    pub fn variant(&self, name: &str) -> Option<&Vec<(String, Type)>> {
        self.variants.iter().find(|(n, _)| n == name).map(|(_, f)| f)
    }

    pub fn is_boxed(&self, variant: &str, field: usize) -> bool {
        self.boxed.contains(&(variant.to_string(), field))
    }
}

#[derive(Debug, Clone)]
pub struct StructMethod {
    pub sig: Signature,
    pub is_mut: bool,
    pub is_async: bool,
    pub is_static: bool,
    pub is_private: bool,
}

#[derive(Debug, Clone)]
pub struct StructOp {
    pub op: BinOp,
    /// `None` for unary minus.
    pub param: Option<Type>,
    pub ret: Type,
    /// Index among this struct's operators, used for the Rust method name.
    pub index: usize,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub decl: StructDecl,
    pub tparams: Vec<String>,
    pub fields: Vec<(String, Type)>,
    pub private_fields: HashSet<String>,
    pub methods: HashMap<String, StructMethod>,
    pub ops: Vec<StructOp>,
    /// `static NAME: Type = value` constants.
    pub consts: Vec<(String, Type)>,
}

impl StructInfo {
    /// Arguments `Name(...)` needs: fields without a default must be given, trailing
    /// fields with defaults may be left out.
    pub fn required_args(&self) -> usize {
        self.decl.fields.iter().rposition(|f| f.default.is_none()).map(|i| i + 1).unwrap_or(0)
    }
}

/// Names of user-defined types, used to resolve `TypeExpr::Named`.
#[derive(Debug, Default)]
pub struct TypeNames {
    pub classes: HashSet<String>,
    pub ifaces: HashSet<String>,
    pub enums: HashSet<String>,
    pub structs: HashSet<String>,
    /// Generic structs and their number of type parameters.
    pub struct_arity: HashMap<String, usize>,
    pub externs: HashSet<String>,
    /// Generic classes and their number of type parameters.
    pub generic: HashMap<String, usize>,
}

impl TypeNames {
    /// Is `name` any declared type (class, interface, enum, struct or extern type)?
    pub fn contains(&self, name: &str) -> bool {
        self.classes.contains(name)
            || self.ifaces.contains(name)
            || self.enums.contains(name)
            || self.structs.contains(name)
            || self.externs.contains(name)
    }
}

#[derive(Debug)]
pub struct Model {
    pub classes: HashMap<String, ClassInfo>,
    /// Classes ordered so that parents come before children.
    pub class_order: Vec<String>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub interface_order: Vec<String>,
    pub functions: HashMap<String, Signature>,
    /// `extern fn`s: signature and the Rust path to call.
    pub extern_fns: HashMap<String, (Signature, String)>,
    /// Type parameters of generic functions.
    pub fn_tparams: HashMap<String, Vec<String>>,
    /// Top-level `async fn`s.
    pub async_fns: HashSet<String>,
    pub enums: HashMap<String, EnumInfo>,
    pub enum_order: Vec<String>,
    /// Enums whose payloads can all be printed and compared.
    pub plain_enums: HashSet<String>,
    pub structs: HashMap<String, StructInfo>,
    pub struct_order: Vec<String>,
    /// Structs whose fields can all be printed and compared.
    pub plain_structs: HashSet<String>,
    pub names: TypeNames,
}

pub const BUILTINS: &[&str] = &["print", "println", "len", "str", "int", "float", "input", "chr", "args"];
pub(crate) const RESERVED_TYPES: &[&str] = &[
    "Rc", "RefCell", "String", "Vec", "BTreeMap", "Result", "Ok", "Err", "Thread", "Channel", "Shared", "Task",
    "Async", "Math", "Random", "Time", "Option", "Some", "None", "Box", "Self", "std", "main", "int", "float", "bool",
    "str", "void",
];

impl Model {
    /// The class itself followed by its parents up to the root.
    pub fn ancestors(&self, name: &str) -> Vec<String> {
        let mut out = vec![];
        let mut cur = Some(name.to_string());
        while let Some(n) = cur {
            cur = self.classes[&n].parent.clone();
            out.push(n);
        }
        out
    }

    pub fn has_subclasses(&self, name: &str) -> bool {
        !self.classes[name].children.is_empty()
    }

    pub fn is_subclass(&self, child: &str, parent: &str) -> bool {
        self.ancestors(child).iter().any(|a| a == parent)
    }

    /// All fields with their declaring class, root fields first.
    pub fn all_fields(&self, name: &str) -> Vec<(String, Type, String)> {
        let mut out = vec![];
        for a in self.ancestors(name).iter().rev() {
            for (f, t) in &self.classes[a].fields {
                out.push((f.clone(), t.clone(), a.clone()));
            }
        }
        out
    }

    /// Most-derived definition of a method: (owner class, signature).
    pub fn find_method(&self, class: &str, method: &str) -> Option<(String, Signature)> {
        for a in self.ancestors(class) {
            if let Some(sig) = self.classes[&a].methods.get(method) {
                return Some((a, sig.clone()));
            }
        }
        None
    }

    /// Nearest strictly-above definition, used by `super.m()`.
    pub fn find_super_method(&self, class: &str, method: &str) -> Option<(String, Signature)> {
        self.classes[class].parent.as_ref().and_then(|p| self.find_method(p, method))
    }

    pub fn find_init(&self, class: &str) -> Option<(String, Signature)> {
        for a in self.ancestors(class) {
            if let Some(sig) = &self.classes[&a].init {
                return Some((a, sig.clone()));
            }
        }
        None
    }

    /// Every method name visible on a class, mapped to its most-derived owner.
    pub fn method_table(&self, class: &str) -> Vec<(String, String, Signature)> {
        let mut seen = HashSet::new();
        let mut out = vec![];
        for a in self.ancestors(class) {
            let mut names: Vec<&String> = self.classes[&a].methods.keys().collect();
            names.sort();
            for mname in names {
                if seen.insert(mname.clone()) {
                    out.push((mname.clone(), a.clone(), self.classes[&a].methods[mname].clone()));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Interfaces implemented by a class, directly, through a base class, or as
    /// super-interfaces of those.
    pub fn all_interfaces(&self, class: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for a in self.ancestors(class).iter().rev() {
            for i in &self.classes[a].interfaces {
                for s in self.iface_closure(i) {
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    /// The interface followed by all of its super-interfaces (each once).
    pub fn iface_closure(&self, iface: &str) -> Vec<String> {
        let mut out = vec![iface.to_string()];
        let mut i = 0;
        while i < out.len() {
            for s in &self.interfaces[&out[i]].supers {
                if !out.contains(s) {
                    out.push(s.clone());
                }
            }
            i += 1;
        }
        out
    }

    /// Like `iface_closure`, but keeps repeats so cycles can be detected.
    pub(crate) fn iface_closure_of(&self, iface: &str) -> Vec<String> {
        let mut out = vec![iface.to_string()];
        let mut i = 0;
        while i < out.len() && out.len() < 1000 {
            let supers = self.interfaces[&out[i]].supers.clone();
            for s in supers {
                if s == iface {
                    out.push(s);
                    return out;
                }
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            i += 1;
        }
        out
    }

    /// Signature of a method visible on an interface (own or inherited).
    pub fn interface_method(&self, iface: &str, method: &str) -> Option<Signature> {
        self.declaring_interface(iface, method)
            .and_then(|d| self.interfaces[&d].methods.iter().find(|(n, _)| n == method).map(|(_, s)| s.clone()))
    }

    /// The interface (this one or a super-interface) that declares `method`.
    pub fn declaring_interface(&self, iface: &str, method: &str) -> Option<String> {
        self.iface_closure(iface).into_iter().find(|i| self.interfaces[i].methods.iter().any(|(n, _)| n == method))
    }

    /// `static fn` visible on a class (own or inherited): (owner, signature).
    pub fn find_static(&self, class: &str, method: &str) -> Option<(String, Signature)> {
        for a in self.ancestors(class) {
            if let Some(sig) = self.classes[&a].statics.get(method) {
                return Some((a, sig.clone()));
            }
        }
        None
    }

    /// Methods that get declared in the class's own Rust trait: introduced by this
    /// class (not inherited, not an override) and not part of an interface it implements.
    pub fn new_trait_methods(&self, class: &str) -> Vec<(String, Signature)> {
        let info = &self.classes[class];
        let ifaces = self.all_interfaces(class);
        let mut out: Vec<(String, Signature)> = info
            .methods
            .iter()
            .filter(|(n, _)| info.parent.as_ref().and_then(|p| self.find_method(p, n)).is_none())
            .filter(|(n, _)| !ifaces.iter().any(|i| self.interface_method(i, n).is_some()))
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Rust trait that declares `method` when called through static type `class`.
    pub fn dispatch_trait(&self, class: &str, method: &str) -> String {
        for i in self.all_interfaces(class) {
            if self.interfaces[&i].methods.iter().any(|(n, _)| n == method) {
                return i;
            }
        }
        // Topmost class that defines it.
        let mut owner = class.to_string();
        for a in self.ancestors(class) {
            if self.classes[&a].methods.contains_key(method) {
                owner = a;
            }
        }
        class_trait(&owner)
    }

    pub fn resolve(&self, t: &TypeExpr, tparams: &[String]) -> Result<Type> {
        resolve_type(t, &self.names, tparams)
    }

    /// Fields of a generic class whose type is a bare type parameter are stored as
    /// `Option<T>` (there is no default value for an unknown type).
    pub fn is_wrapped_field(&self, class: &str, field: &str) -> bool {
        self.classes.get(class).is_some_and(|c| c.fields.iter().any(|(n, t)| n == field && matches!(t, Type::Param(_))))
    }

    /// Maps a generic class's type parameters to concrete arguments.
    pub fn bindings(&self, class: &str, args: &[Type]) -> HashMap<String, Type> {
        self.classes[class].tparams.iter().cloned().zip(args.iter().cloned()).collect()
    }

    /// Values that can be printed and compared with `==`.
    pub fn is_plain(&self, t: &Type) -> bool {
        match t {
            Type::Int | Type::Float | Type::Bool | Type::Str => true,
            Type::Array(i) | Type::Optional(i) => self.is_plain(i),
            Type::Map(k, v) => self.is_plain(k) && self.is_plain(v),
            Type::Enum(e) => self.plain_enums.contains(e),
            Type::Struct(st, args) => self.plain_structs.contains(st) && args.iter().all(|a| self.is_plain(a)),
            Type::Result(t, e) => (**t == Type::Void || self.is_plain(t)) && self.is_plain(e),
            _ => false,
        }
    }

    /// Can values of this type be moved to another thread? Plain data, channels and
    /// shared cells can; objects, function values and type parameters cannot.
    pub fn is_sendable(&self, t: &Type) -> bool {
        match t {
            Type::Int | Type::Float | Type::Bool | Type::Str | Type::Void | Type::Extern(_) => true,
            Type::Channel(_) | Type::Shared(_) => true,
            Type::Array(i) | Type::Optional(i) => self.is_sendable(i),
            Type::Map(k, v) | Type::Result(k, v) => self.is_sendable(k) && self.is_sendable(v),
            Type::Struct(st, args) => {
                let map = self.struct_bindings(st, args);
                self.structs[st].fields.iter().all(|(_, ft)| self.is_sendable(&subst(ft, &map)))
            }
            Type::Enum(e) => self.enums[e].variants.iter().all(|(_, fs)| fs.iter().all(|(_, ft)| self.is_sendable(ft))),
            _ => false,
        }
    }

    /// `is_plain`, but type parameters count as plain (used for generic struct definitions).
    pub(crate) fn is_plain_params_ok(&self, t: &Type) -> bool {
        match t {
            Type::Param(_) => true,
            Type::Array(i) | Type::Optional(i) => self.is_plain_params_ok(i),
            Type::Map(k, v) => self.is_plain_params_ok(k) && self.is_plain_params_ok(v),
            Type::Struct(st, args) => {
                self.plain_structs.contains(st) && args.iter().all(|a| self.is_plain_params_ok(a))
            }
            other => self.is_plain(other),
        }
    }

    /// A struct's type parameters mapped to `args` (empty for non-generic structs).
    pub fn struct_bindings(&self, st: &str, args: &[Type]) -> HashMap<String, Type> {
        self.structs[st].tparams.iter().cloned().zip(args.iter().cloned()).collect()
    }

    /// Does the enum or struct `current` hold `target` by value (not behind an array,
    /// map, object or boxed enum field)? Such a type would be infinitely large.
    pub(crate) fn value_contains(&self, current: &str, target: &str, seen: &mut HashSet<String>) -> bool {
        if !seen.insert(current.to_string()) {
            return false;
        }
        let mut inner = vec![];
        if let Some(en) = self.enums.get(current) {
            for (v, fs) in &en.variants {
                for (i, (_, t)) in fs.iter().enumerate() {
                    if !en.is_boxed(v, i) {
                        held_by_value(t, &mut inner);
                    }
                }
            }
        }
        if let Some(st) = self.structs.get(current) {
            for (_, t) in &st.fields {
                held_by_value(t, &mut inner);
            }
        }
        inner.iter().any(|e| e == target || self.value_contains(e, target, seen))
    }

    /// Does a value of type `t` hold the enum or struct `target` by value?
    pub(crate) fn type_contains(&self, t: &Type, target: &str) -> bool {
        let mut inner = vec![];
        held_by_value(t, &mut inner);
        inner.iter().any(|e| e == target || self.value_contains(e, target, &mut HashSet::new()))
    }

    /// Can a value of type `from` be used where `to` is expected?
    pub fn assignable(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }
        match (from, to) {
            (Type::NoneLit, Type::Optional(_)) => true,
            (Type::Optional(a), Type::Optional(b)) => self.assignable(a, b),
            (Type::Result(a, ea), Type::Result(b, eb)) => ea == eb && self.assignable(a, b),
            (f, Type::Optional(inner)) => self.assignable(f, inner),
            (Type::Class(d), Type::Class(c)) => self.is_subclass(d, c),
            (Type::Class(d), Type::Interface(i)) => self.all_interfaces(d).contains(i),
            (Type::Interface(a), Type::Interface(b)) => self.iface_closure(a).contains(b),
            (Type::Array(a), Type::Array(b)) => self.assignable(a, b),
            (Type::Map(ka, va), Type::Map(kb, vb)) => ka == kb && self.assignable(va, vb),
            _ => false,
        }
    }
}

/// Gives a position to errors raised without one (e.g. from type resolution).
pub fn at_pos(mut e: kalinga_syntax::error::Diagnostic, pos: Span) -> kalinga_syntax::error::Diagnostic {
    if e.span == Span::default() {
        e.span = pos;
    }
    e
}

pub fn class_trait(class: &str) -> String {
    format!("__I_{}", class)
}

pub(crate) fn resolve_type(t: &TypeExpr, names: &TypeNames, tparams: &[String]) -> Result<Type> {
    Ok(match t {
        TypeExpr::Int => Type::Int,
        TypeExpr::Float => Type::Float,
        TypeExpr::Bool => Type::Bool,
        TypeExpr::Str => Type::Str,
        TypeExpr::Void => Type::Void,
        TypeExpr::Array(inner) => {
            let i = resolve_type(inner, names, tparams)?;
            if i == Type::Void {
                return err("array of void is not allowed", Span::default());
            }
            Type::Array(Box::new(i))
        }
        TypeExpr::Map(k, v) => {
            let kt = resolve_type(k, names, tparams)?;
            if !is_map_key(&kt) {
                return err(format!("map keys must be `int`, `str` or `bool`, not `{}`", kt), Span::default());
            }
            let vt = resolve_type(v, names, tparams)?;
            if vt == Type::Void {
                return err("map of void is not allowed", Span::default());
            }
            Type::Map(Box::new(kt), Box::new(vt))
        }
        TypeExpr::Func(ps, r) => {
            let mut params = Vec::new();
            for p in ps {
                let t = resolve_type(p, names, tparams)?;
                if t == Type::Void {
                    return err("a function parameter cannot be void", Span::default());
                }
                params.push(t);
            }
            Type::Func(params, Box::new(resolve_type(r, names, tparams)?))
        }
        TypeExpr::Optional(inner) => {
            let i = resolve_type(inner, names, tparams)?;
            if matches!(i, Type::Optional(_)) {
                return err("nested optional `T??` is not allowed", Span::default());
            }
            Type::Optional(Box::new(i))
        }
        TypeExpr::Named(n, args, pos) => resolve_named(n, args, *pos, names, tparams)?,
    })
}

/// A named type, `Name` or `Name<args>`: a type parameter, a built-in generic type, or a
/// declared class, interface, enum, struct or extern type.
fn resolve_named(n: &str, args: &[TypeExpr], pos: Span, names: &TypeNames, tparams: &[String]) -> Result<Type> {
    if tparams.iter().any(|t| t == n) {
        if !args.is_empty() {
            return err(format!("type parameter `{}` does not take type arguments", n), pos);
        }
        return Ok(Type::Param(n.to_string()));
    }
    if matches!(n, "Channel" | "Shared" | "Task" | "Async") {
        if args.len() != 1 {
            return err(format!("`{}` needs one type: `{}<int>`", n, n), pos);
        }
        let t = Box::new(resolve_type(&args[0], names, tparams)?);
        return Ok(match n {
            "Channel" => Type::Channel(t),
            "Shared" => Type::Shared(t),
            "Async" => Type::Async(t),
            _ => Type::Task(t),
        });
    }
    if n == "Result" {
        if args.len() != 2 {
            return err("`Result` needs two types: `Result<T, E>` (value type, error type)", pos);
        }
        let t = resolve_type(&args[0], names, tparams)?;
        let e = resolve_type(&args[1], names, tparams)?;
        if e == Type::Void {
            return err("the error type of a `Result` cannot be void", pos);
        }
        return Ok(Type::Result(Box::new(t), Box::new(e)));
    }
    if let Some(&arity) = names.struct_arity.get(n) {
        if args.len() != arity {
            return err(format!("`{}` needs {} type argument(s), e.g. `{}<int>`", n, arity, n), pos);
        }
        let mut targs = Vec::new();
        for a in args {
            targs.push(resolve_type(a, names, tparams)?);
        }
        return Ok(Type::Struct(n.to_string(), targs));
    }
    if let Some(&arity) = names.generic.get(n) {
        if args.len() != arity {
            return err(format!("`{}` needs {} type argument(s), e.g. `{}<int>`", n, arity, n), pos);
        }
        let mut targs = Vec::new();
        for a in args {
            let t = resolve_type(a, names, tparams)?;
            if t == Type::Void {
                return err("`void` cannot be a type argument", pos);
            }
            targs.push(t);
        }
        return Ok(Type::Generic(n.to_string(), targs));
    }
    if !args.is_empty() {
        return err(format!("`{}` is not generic and takes no type arguments", n), pos);
    }
    let n = n.to_string();
    Ok(if names.classes.contains(&n) {
        Type::Class(n)
    } else if names.ifaces.contains(&n) {
        Type::Interface(n)
    } else if names.enums.contains(&n) {
        Type::Enum(n)
    } else if names.structs.contains(&n) {
        Type::Struct(n, vec![])
    } else if names.externs.contains(&n) {
        Type::Extern(n)
    } else {
        return err(format!("unknown type `{}`", n), pos);
    })
}

/// The enums and structs a value of type `t` holds directly (not behind an array, map or object).
fn held_by_value(t: &Type, out: &mut Vec<String>) {
    match t {
        Type::Enum(e) | Type::Struct(e, _) => out.push(e.clone()),
        Type::Optional(i) => held_by_value(i, out),
        _ => {}
    }
}
