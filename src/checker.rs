//! Declaration-level checks: builds the class/interface model, resolves `:` bases,
//! validates inheritance, overrides and interface conformance. Expression-level
//! type checking happens in `codegen`, which uses this model.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ast::*;
use crate::error::{err, Pos, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Optional(Box<Type>),
    Class(String),
    Interface(String),
    /// Type of the `none` literal before it meets an expected optional type.
    NoneLit,
}

impl Type {
    pub fn is_copy(&self) -> bool {
        matches!(self, Type::Int | Type::Float | Type::Bool | Type::Void)
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Type::Class(_) | Type::Interface(_))
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "str"),
            Type::Void => write!(f, "void"),
            Type::Array(t) => write!(f, "[{}]", t),
            Type::Map(k, v) => write!(f, "{{{}: {}}}", k, v),
            Type::Optional(t) => write!(f, "{}?", t),
            Type::Class(n) | Type::Interface(n) => write!(f, "{}", n),
            Type::NoneLit => write!(f, "none"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    pub params: Vec<(String, Type)>,
    pub ret: Type,
}

impl Signature {
    fn same_shape(&self, other: &Signature) -> bool {
        self.ret == other.ret
            && self.params.len() == other.params.len()
            && self.params.iter().zip(&other.params).all(|(a, b)| a.1 == b.1)
    }

    pub fn describe(&self) -> String {
        let ps: Vec<String> = self.params.iter().map(|(n, t)| format!("{}: {}", n, t)).collect();
        format!("({}) -> {}", ps.join(", "), self.ret)
    }
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub decl: ClassDecl,
    pub parent: Option<String>,
    /// Interfaces listed directly after `:`.
    pub interfaces: Vec<String>,
    pub fields: Vec<(String, Type)>,
    pub methods: HashMap<String, Signature>,
    pub init: Option<Signature>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub methods: Vec<(String, Signature)>,
}

#[derive(Debug)]
pub struct Model {
    pub classes: HashMap<String, ClassInfo>,
    /// Classes ordered so that parents come before children.
    pub class_order: Vec<String>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub interface_order: Vec<String>,
    pub functions: HashMap<String, Signature>,
}

pub const BUILTINS: &[&str] = &["print", "len", "str", "int", "float", "input"];
const RESERVED_TYPES: &[&str] = &[
    "Rc", "RefCell", "String", "Vec", "BTreeMap", "Option", "Some", "None", "Box", "Self", "std", "main",
    "int", "float", "bool", "str", "void",
];

impl Model {
    pub fn build(prog: &Program) -> Result<Model> {
        let mut m = Model {
            classes: HashMap::new(),
            class_order: vec![],
            interfaces: HashMap::new(),
            interface_order: vec![],
            functions: HashMap::new(),
        };

        // 1. Register names so types can refer to each other in any order.
        let mut type_names: HashSet<&str> = HashSet::new();
        for i in &prog.interfaces {
            check_type_name(&i.name, i.pos)?;
            if !type_names.insert(&i.name) {
                return err(format!("`{}` is defined more than once", i.name), i.pos);
            }
        }
        for c in &prog.classes {
            check_type_name(&c.name, c.pos)?;
            if !type_names.insert(&c.name) {
                return err(format!("`{}` is defined more than once", c.name), c.pos);
            }
        }
        let class_names: HashSet<String> = prog.classes.iter().map(|c| c.name.clone()).collect();
        let iface_names: HashSet<String> = prog.interfaces.iter().map(|i| i.name.clone()).collect();
        let resolve = |t: &TypeExpr| resolve_type(t, &class_names, &iface_names);

        // 2. Interfaces.
        for i in &prog.interfaces {
            let mut methods: Vec<(String, Signature)> = Vec::new();
            for s in &i.methods {
                if methods.iter().any(|(n, _)| n == &s.name) {
                    return err(format!("method `{}` declared twice in `{}`", s.name, i.name), s.pos);
                }
                methods.push((s.name.clone(), signature(&s.params, &s.ret, &resolve)?));
            }
            m.interfaces.insert(i.name.clone(), InterfaceInfo { methods });
            m.interface_order.push(i.name.clone());
        }

        // 3. Functions.
        for f in &prog.functions {
            if BUILTINS.contains(&f.name.as_str()) {
                return err(format!("`{}` is a built-in function and cannot be redefined", f.name), f.pos);
            }
            if type_names.contains(f.name.as_str()) {
                return err(format!("function `{}` has the same name as a type", f.name), f.pos);
            }
            if m.functions.contains_key(&f.name) {
                return err(format!("function `{}` is defined more than once", f.name), f.pos);
            }
            if f.is_override {
                return err("`override` is only allowed on methods", f.pos);
            }
            m.functions.insert(f.name.clone(), signature(&f.params, &f.ret, &resolve)?);
        }
        match m.functions.get("main") {
            None => return err("program has no `fn main()`", Pos { line: 1, col: 1, file: 0 }),
            Some(sig) if !sig.params.is_empty() || sig.ret != Type::Void => {
                let pos = prog.functions.iter().find(|f| f.name == "main").unwrap().pos;
                return err("`main` must take no parameters and return nothing", pos);
            }
            _ => {}
        }

        // 4. Classes: bases, fields, methods.
        for c in &prog.classes {
            let mut parent = None;
            let mut interfaces = Vec::new();
            for (b, pos) in &c.bases {
                if class_names.contains(b) {
                    if parent.is_some() {
                        return err(
                            format!("`{}` already has a base class; a class can extend only one class (but any number of interfaces)", c.name),
                            *pos,
                        );
                    }
                    if b == &c.name {
                        return err("a class cannot extend itself", *pos);
                    }
                    parent = Some(b.clone());
                } else if iface_names.contains(b) {
                    if interfaces.contains(b) {
                        return err(format!("interface `{}` listed twice", b), *pos);
                    }
                    interfaces.push(b.clone());
                } else {
                    return err(format!("unknown class or interface `{}`", b), *pos);
                }
            }
            let mut fields = Vec::new();
            for f in &c.fields {
                if fields.iter().any(|(n, _): &(String, Type)| n == &f.name) {
                    return err(format!("field `{}` declared twice", f.name), f.pos);
                }
                let ty = resolve(&f.ty).map_err(|e| at_pos(e, f.pos))?;
                if ty == Type::Void {
                    return err("fields cannot have type void", f.pos);
                }
                if ty.is_object() && f.default.is_none() {
                    return err(
                        format!("field `{}` of type `{}` needs a default value (or make it optional: `{}?`)", f.name, ty, ty),
                        f.pos,
                    );
                }
                fields.push((f.name.clone(), ty));
            }
            let mut methods = HashMap::new();
            for meth in &c.methods {
                if methods.contains_key(&meth.name) {
                    return err(format!("method `{}` defined twice in `{}`", meth.name, c.name), meth.pos);
                }
                methods.insert(meth.name.clone(), signature(&meth.params, &meth.ret, &resolve)?);
            }
            let init = match &c.init {
                Some(i) => Some(signature(&i.params, &TypeExpr::Void, &resolve)?),
                None => None,
            };
            m.classes.insert(
                c.name.clone(),
                ClassInfo { decl: c.clone(), parent, interfaces, fields, methods, init, children: vec![] },
            );
        }

        // 5. Inheritance cycles and topological order.
        for c in &prog.classes {
            let mut seen = HashSet::new();
            let mut cur = Some(c.name.clone());
            while let Some(n) = cur {
                if !seen.insert(n.clone()) {
                    return err(format!("inheritance cycle involving `{}`", c.name), c.pos);
                }
                cur = m.classes[&n].parent.clone();
            }
        }
        let mut order: Vec<String> = Vec::new();
        let mut names: Vec<&String> = prog.classes.iter().map(|c| &c.name).collect();
        names.sort_by_key(|n| m.ancestors(n).len());
        for n in names {
            order.push(n.clone());
        }
        m.class_order = order;
        for n in m.class_order.clone() {
            if let Some(p) = m.classes[&n].parent.clone() {
                m.classes.get_mut(&p).unwrap().children.push(n.clone());
            }
        }

        // 6. Member rules that depend on the hierarchy.
        for n in m.class_order.clone() {
            m.check_class(&n)?;
        }
        Ok(m)
    }

    fn check_class(&self, name: &str) -> Result<()> {
        let info = &self.classes[name];
        // Fields may not shadow inherited ones.
        if let Some(p) = &info.parent {
            let inherited = self.all_fields(p);
            for f in &info.decl.fields {
                if inherited.iter().any(|(n, _, _)| n == &f.name) {
                    return err(format!("field `{}` is already inherited from a base class", f.name), f.pos);
                }
            }
        }
        // Overrides.
        for meth in &info.decl.methods {
            let sig = &info.methods[&meth.name];
            let inherited = info.parent.as_ref().and_then(|p| self.find_method(p, &meth.name));
            match (inherited, meth.is_override) {
                (Some((owner, psig)), true) => {
                    if !psig.same_shape(sig) {
                        return err(
                            format!(
                                "override of `{}` must match `{}.{}{}`",
                                meth.name,
                                owner,
                                meth.name,
                                psig.describe()
                            ),
                            meth.pos,
                        );
                    }
                }
                (Some((owner, _)), false) => {
                    return err(
                        format!("method `{}` overrides `{}.{}`; mark it `override fn`", meth.name, owner, meth.name),
                        meth.pos,
                    )
                }
                (None, true) => {
                    return err(format!("`{}` is marked override but no base class has this method", meth.name), meth.pos)
                }
                (None, false) => {}
            }
        }
        // Interface conformance (including methods inherited from base classes).
        for iname in &info.interfaces {
            for (mname, isig) in &self.interfaces[iname].methods {
                match self.find_method(name, mname) {
                    None => {
                        return err(
                            format!("`{}` does not implement `{}.{}{}`", name, iname, mname, isig.describe()),
                            info.decl.pos,
                        )
                    }
                    Some((_, sig)) if !sig.same_shape(isig) => {
                        return err(
                            format!("`{}.{}` must match `{}.{}{}`", name, mname, iname, mname, isig.describe()),
                            info.decl.pos,
                        )
                    }
                    _ => {}
                }
            }
        }
        // Constructor chaining.
        if let Some(init) = &info.decl.init {
            let parent_init = info.parent.as_ref().and_then(|p| self.find_init(p));
            let calls_super = init.body.iter().any(|s| matches!(&s.kind, StmtKind::Expr(Expr { kind: ExprKind::SuperInit(_), .. })));
            if let Some((pname, _)) = parent_init {
                if !calls_super {
                    return err(format!("`init` must call `super(...)` to initialize base class `{}`", pname), init.pos);
                }
            }
        }
        Ok(())
    }

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

    /// Interfaces implemented by a class, directly or through a base class.
    pub fn all_interfaces(&self, class: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for a in self.ancestors(class).iter().rev() {
            for i in &self.classes[a].interfaces {
                if !out.contains(i) {
                    out.push(i.clone());
                }
            }
        }
        out
    }

    pub fn interface_method(&self, iface: &str, method: &str) -> Option<Signature> {
        self.interfaces[iface].methods.iter().find(|(n, _)| n == method).map(|(_, s)| s.clone())
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
            if self.interface_method(&i, method).is_some() {
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

    pub fn resolve(&self, t: &TypeExpr) -> Result<Type> {
        let classes: HashSet<String> = self.classes.keys().cloned().collect();
        let ifaces: HashSet<String> = self.interfaces.keys().cloned().collect();
        resolve_type(t, &classes, &ifaces)
    }

    /// Can a value of type `from` be used where `to` is expected?
    pub fn assignable(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }
        match (from, to) {
            (Type::NoneLit, Type::Optional(_)) => true,
            (Type::Optional(a), Type::Optional(b)) => self.assignable(a, b),
            (f, Type::Optional(inner)) => self.assignable(f, inner),
            (Type::Class(d), Type::Class(c)) => self.is_subclass(d, c),
            (Type::Class(d), Type::Interface(i)) => self.all_interfaces(d).contains(i),
            (Type::Array(a), Type::Array(b)) => self.assignable(a, b),
            (Type::Map(ka, va), Type::Map(kb, vb)) => ka == kb && self.assignable(va, vb),
            _ => false,
        }
    }
}

/// Gives a position to errors raised without one (e.g. from type resolution).
pub fn at_pos(mut e: crate::error::CompileError, pos: Pos) -> crate::error::CompileError {
    if e.pos == Pos::default() {
        e.pos = pos;
    }
    e
}

pub fn is_map_key(t: &Type) -> bool {
    matches!(t, Type::Int | Type::Str | Type::Bool)
}

pub fn class_trait(class: &str) -> String {
    format!("__I_{}", class)
}

fn check_type_name(name: &str, pos: Pos) -> Result<()> {
    if RESERVED_TYPES.contains(&name) || BUILTINS.contains(&name) {
        return err(format!("`{}` is reserved and cannot be used as a type name", name), pos);
    }
    if !name.starts_with(|c: char| c.is_uppercase()) {
        return err(format!("type names must start with an uppercase letter: `{}`", name), pos);
    }
    Ok(())
}

fn resolve_type(t: &TypeExpr, classes: &HashSet<String>, ifaces: &HashSet<String>) -> Result<Type> {
    Ok(match t {
        TypeExpr::Int => Type::Int,
        TypeExpr::Float => Type::Float,
        TypeExpr::Bool => Type::Bool,
        TypeExpr::Str => Type::Str,
        TypeExpr::Void => Type::Void,
        TypeExpr::Array(inner) => {
            let i = resolve_type(inner, classes, ifaces)?;
            if i == Type::Void {
                return err("array of void is not allowed", Pos::default());
            }
            Type::Array(Box::new(i))
        }
        TypeExpr::Map(k, v) => {
            let kt = resolve_type(k, classes, ifaces)?;
            if !is_map_key(&kt) {
                return err(format!("map keys must be `int`, `str` or `bool`, not `{}`", kt), Pos::default());
            }
            let vt = resolve_type(v, classes, ifaces)?;
            if vt == Type::Void {
                return err("map of void is not allowed", Pos::default());
            }
            Type::Map(Box::new(kt), Box::new(vt))
        }
        TypeExpr::Optional(inner) => {
            let i = resolve_type(inner, classes, ifaces)?;
            if matches!(i, Type::Optional(_)) {
                return err("nested optional `T??` is not allowed", Pos::default());
            }
            Type::Optional(Box::new(i))
        }
        TypeExpr::Named(n, pos) => {
            if classes.contains(n) {
                Type::Class(n.clone())
            } else if ifaces.contains(n) {
                Type::Interface(n.clone())
            } else {
                return err(format!("unknown type `{}`", n), *pos);
            }
        }
    })
}

fn signature(
    params: &[Param],
    ret: &TypeExpr,
    resolve: &dyn Fn(&TypeExpr) -> Result<Type>,
) -> Result<Signature> {
    let mut ps: Vec<(String, Type)> = Vec::new();
    for p in params {
        if ps.iter().any(|(n, _)| n == &p.name) {
            return err(format!("parameter `{}` declared twice", p.name), p.pos);
        }
        let t = resolve(&p.ty).map_err(|e| at_pos(e, p.pos))?;
        if t == Type::Void {
            return err("parameters cannot have type void", p.pos);
        }
        ps.push((p.name.clone(), t));
    }
    Ok(Signature { params: ps, ret: resolve(ret)? })
}
