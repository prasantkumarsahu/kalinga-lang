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
    Enum(String),
    /// A value-type struct, with type arguments if it is generic (`Grid<int>`).
    Struct(String, Vec<Type>),
    /// An opaque Rust type declared with `extern type`.
    Extern(String),
    /// `Result<T, E>`: `Ok(T)` or `Err(E)`.
    Result(Box<Type>, Box<Type>),
    /// `fn(A, B) -> R`
    Func(Vec<Type>, Box<Type>),
    /// Placeholder for "not known yet" in expected-type hints (lambda results).
    Unknown,
    /// A type parameter (`T`) inside generic code.
    Param(String),
    /// An instance of a generic class: `Stack<int>`.
    Generic(String, Vec<Type>),
    /// Type of the `none` literal before it meets an expected optional type.
    NoneLit,
}

impl Type {
    pub fn is_copy(&self) -> bool {
        matches!(self, Type::Int | Type::Float | Type::Bool | Type::Void)
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Type::Class(_) | Type::Interface(_) | Type::Generic(..))
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
            Type::Class(n) | Type::Interface(n) | Type::Enum(n) | Type::Extern(n) => write!(f, "{}", n),
            Type::Struct(n, args) if args.is_empty() => write!(f, "{}", n),
            Type::Struct(n, args) => {
                let a: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                write!(f, "{}<{}>", n, a.join(", "))
            }
            Type::NoneLit => write!(f, "none"),
            Type::Unknown => write!(f, "_"),
            Type::Param(n) => write!(f, "{}", n),
            Type::Result(t, e) => write!(f, "Result<{}, {}>", t, e),
            Type::Generic(n, args) => {
                let a: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                write!(f, "{}<{}>", n, a.join(", "))
            }
            Type::Func(ps, r) => {
                let ps: Vec<String> = ps.iter().map(|p| p.to_string()).collect();
                if **r == Type::Void {
                    write!(f, "fn({})", ps.join(", "))
                } else {
                    write!(f, "fn({}) -> {}", ps.join(", "), r)
                }
            }
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
    /// `static fn`s: not inherited through objects, called as `Class.name(...)`.
    pub statics: HashMap<String, Signature>,
    pub private_fields: HashSet<String>,
    pub private_methods: HashSet<String>,
    pub abstract_methods: HashSet<String>,
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

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub variants: Vec<(String, Vec<(String, Type)>)>,
}

impl EnumInfo {
    pub fn variant(&self, name: &str) -> Option<&Vec<(String, Type)>> {
        self.variants.iter().find(|(n, _)| n == name).map(|(_, f)| f)
    }
}

#[derive(Debug, Clone)]
pub struct StructMethod {
    pub sig: Signature,
    pub is_mut: bool,
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

pub const BUILTINS: &[&str] = &["print", "len", "str", "int", "float", "input"];
const RESERVED_TYPES: &[&str] = &[
    "Rc", "RefCell", "String", "Vec", "BTreeMap", "Result", "Ok", "Err", "Math", "Random", "Time", "Option", "Some", "None", "Box", "Self", "std", "main",
    "int", "float", "bool", "str", "void",
];

impl Model {
    /// `require_main` is false when only checking a file, which may be a library
    /// meant to be imported by another file.
    pub fn build(prog: &Program, require_main: bool) -> Result<Model> {
        let mut m = Model {
            classes: HashMap::new(),
            class_order: vec![],
            interfaces: HashMap::new(),
            interface_order: vec![],
            functions: HashMap::new(),
            extern_fns: HashMap::new(),
            fn_tparams: HashMap::new(),
            enums: HashMap::new(),
            enum_order: vec![],
            plain_enums: HashSet::new(),
            structs: HashMap::new(),
            struct_order: vec![],
            plain_structs: HashSet::new(),
            names: TypeNames::default(),
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
        for (name, pos) in &prog.extern_types {
            check_type_name(name, *pos)?;
            if !type_names.insert(name) {
                return err(format!("`{}` is defined more than once", name), *pos);
            }
        }
        for st in &prog.structs {
            check_type_name(&st.name, st.pos)?;
            if !type_names.insert(&st.name) {
                return err(format!("`{}` is defined more than once", st.name), st.pos);
            }
        }
        for e in &prog.enums {
            check_type_name(&e.name, e.pos)?;
            if !type_names.insert(&e.name) {
                return err(format!("`{}` is defined more than once", e.name), e.pos);
            }
        }
        let names = TypeNames {
            classes: prog.classes.iter().map(|c| c.name.clone()).collect(),
            ifaces: prog.interfaces.iter().map(|i| i.name.clone()).collect(),
            enums: prog.enums.iter().map(|e| e.name.clone()).collect(),
            structs: prog.structs.iter().map(|s| s.name.clone()).collect(),
            struct_arity: prog.structs.iter().filter(|s| !s.tparams.is_empty()).map(|s| (s.name.clone(), s.tparams.len())).collect(),
            externs: prog.extern_types.iter().map(|(n, _)| n.clone()).collect(),
            generic: prog.classes.iter().filter(|c| !c.tparams.is_empty()).map(|c| (c.name.clone(), c.tparams.len())).collect(),
        };
        let check_tparams = |tps: &[String], pos: Pos| -> Result<()> {
            for (i, t) in tps.iter().enumerate() {
                if !t.starts_with(|c: char| c.is_uppercase()) {
                    return err(format!("type parameters must start with an uppercase letter: `{}`", t), pos);
                }
                if type_names.contains(t.as_str()) || RESERVED_TYPES.contains(&t.as_str()) {
                    return err(format!("type parameter `{}` has the same name as a type", t), pos);
                }
                if tps[..i].contains(t) {
                    return err(format!("type parameter `{}` listed twice", t), pos);
                }
            }
            Ok(())
        };
        let class_names = names.classes.clone();
        let iface_names = names.ifaces.clone();
        let resolve = |t: &TypeExpr| resolve_type(t, &names, &[]);

        // Enums.
        for e in &prog.enums {
            let mut variants: Vec<(String, Vec<(String, Type)>)> = Vec::new();
            if e.variants.is_empty() {
                return err(format!("enum `{}` needs at least one variant", e.name), e.pos);
            }
            for v in &e.variants {
                if !v.name.starts_with(|c: char| c.is_uppercase()) || v.name == "Self" {
                    return err(format!("variant names must start with an uppercase letter: `{}`", v.name), v.pos);
                }
                if variants.iter().any(|(n, _)| n == &v.name) {
                    return err(format!("variant `{}` declared twice in `{}`", v.name, e.name), v.pos);
                }
                let sig = signature(&v.fields, &TypeExpr::Void, &resolve)?;
                variants.push((v.name.clone(), sig.params));
            }
            m.enums.insert(e.name.clone(), EnumInfo { variants });
            m.enum_order.push(e.name.clone());
        }
        // Structs.
        for st in &prog.structs {
            check_tparams(&st.tparams, st.pos)?;
            let tps = st.tparams.clone();
            let resolve = |t: &TypeExpr| resolve_type(t, &names, &tps);
            let mut fields: Vec<(String, Type)> = Vec::new();
            for f in &st.fields {
                if fields.iter().any(|(n, _)| n == &f.name) {
                    return err(format!("field `{}` declared twice", f.name), f.pos);
                }
                let ty = resolve(&f.ty).map_err(|e| at_pos(e, f.pos))?;
                if ty == Type::Void {
                    return err("fields cannot have type void", f.pos);
                }
                fields.push((f.name.clone(), ty));
            }
            let mut methods = HashMap::new();
            for meth in &st.methods {
                if methods.contains_key(&meth.name) || fields.iter().any(|(n, _)| n == &meth.name) {
                    return err(format!("`{}` is defined twice in `{}`", meth.name, st.name), meth.pos);
                }
                if !meth.tparams.is_empty() {
                    return err("struct methods cannot be generic (yet)", meth.pos);
                }
                let sig = signature(&meth.params, &meth.ret, &resolve)?;
                methods.insert(
                    meth.name.clone(),
                    StructMethod { sig, is_mut: meth.is_mut, is_static: meth.is_static, is_private: meth.is_private },
                );
            }
            let mut ops: Vec<StructOp> = Vec::new();
            for (index, o) in st.ops.iter().enumerate() {
                let sig = signature(&o.params, &o.ret, &resolve)?;
                let param = match (sig.params.len(), o.op) {
                    (0, BinOp::Sub) => None,
                    (1, _) => Some(sig.params[0].1.clone()),
                    _ => {
                        return err(
                            format!("`op {}` takes one parameter (the right-hand side); only `op -()` may take none", o.op.rust()),
                            o.pos,
                        )
                    }
                };
                if sig.ret == Type::Void {
                    return err("an operator must return a value", o.pos);
                }
                if ops.iter().any(|x| x.op == o.op && x.param == param) {
                    return err(format!("`op {}` with this parameter type is defined twice", o.op.rust()), o.pos);
                }
                ops.push(StructOp { op: o.op, param, ret: sig.ret, index });
            }
            let private_fields = st.fields.iter().filter(|f| f.is_private).map(|f| f.name.clone()).collect();
            m.structs.insert(st.name.clone(), StructInfo { decl: st.clone(), tparams: st.tparams.clone(), fields, private_fields, methods, ops });
            m.struct_order.push(st.name.clone());
        }
        for e in &prog.enums {
            if m.value_contains(&e.name, &e.name, &mut HashSet::new()) {
                return err(
                    format!("enum `{}` contains itself; put the recursive part in an array (`[{}]`) or use a class", e.name, e.name),
                    e.pos,
                );
            }
        }
        for st in &prog.structs {
            if m.value_contains(&st.name, &st.name, &mut HashSet::new()) {
                return err(
                    format!("struct `{}` contains itself; put the recursive part in an array (`[{}]`) or use a class", st.name, st.name),
                    st.pos,
                );
            }
        }
        // Enums and structs are "plain" (printable, comparable) when all their parts are;
        // iterate to a fixpoint since they can contain each other.
        m.plain_enums = m.enums.keys().cloned().collect();
        m.plain_structs = m.structs.keys().cloned().collect();
        loop {
            let before = m.plain_enums.len() + m.plain_structs.len();
            let bad_enums: Vec<String> = m
                .plain_enums
                .iter()
                .filter(|n| m.enums[*n].variants.iter().any(|(_, fs)| fs.iter().any(|(_, t)| !m.is_plain(t))))
                .cloned()
                .collect();
            // For generic structs, type parameters count as plain here; uses then also need
            // plain type arguments (see `is_plain`).
            let bad_structs: Vec<String> =
                m.plain_structs.iter().filter(|n| m.structs[*n].fields.iter().any(|(_, t)| !m.is_plain_params_ok(t))).cloned().collect();
            for b in bad_enums {
                m.plain_enums.remove(&b);
            }
            for b in bad_structs {
                m.plain_structs.remove(&b);
            }
            if m.plain_enums.len() + m.plain_structs.len() == before {
                break;
            }
        }

        // 2. Interfaces.
        for i in &prog.interfaces {
            let mut methods: Vec<(String, Signature)> = Vec::new();
            for s in &i.methods {
                if methods.iter().any(|(n, _)| n == &s.name) {
                    return err(format!("method `{}` declared twice in `{}`", s.name, i.name), s.pos);
                }
                methods.push((s.name.clone(), signature(&s.params, &s.ret, &resolve)?));
            }
            let mut supers = Vec::new();
            for (b, pos) in &i.bases {
                if !iface_names.contains(b) {
                    return err(format!("`{}` is not an interface (interfaces can only extend interfaces)", b), *pos);
                }
                if supers.contains(b) {
                    return err(format!("interface `{}` listed twice", b), *pos);
                }
                supers.push(b.clone());
            }
            m.interfaces.insert(i.name.clone(), InterfaceInfo { methods, supers });
            m.interface_order.push(i.name.clone());
        }
        for i in &prog.interfaces {
            if m.iface_closure_of(&i.name).iter().skip(1).any(|s| s == &i.name) {
                return err(format!("interface `{}` extends itself", i.name), i.pos);
            }
            let mut seen: HashMap<String, (String, Signature)> = HashMap::new();
            for s in m.iface_closure(&i.name) {
                for (mname, sig) in &m.interfaces[&s].methods {
                    if let Some((other, osig)) = seen.get(mname) {
                        if other != &s && osig != sig {
                            return err(
                                format!("`{}` gets method `{}` from both `{}` and `{}` with different signatures", i.name, mname, other, s),
                                i.pos,
                            );
                        }
                    }
                    seen.insert(mname.clone(), (s.clone(), sig.clone()));
                }
            }
        }
        // Parents first, so trait supertraits are declared before use (not required by Rust, but readable).
        let order = m.interface_order.clone();
        m.interface_order = order;

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
            if f.is_override || f.is_private || f.is_static || f.is_abstract {
                return err("`override`, `private`, `static` and `abstract` are only allowed on class members", f.pos);
            }
            check_tparams(&f.tparams, f.pos)?;
            if f.name == "main" && !f.tparams.is_empty() {
                return err("`main` cannot be generic", f.pos);
            }
            let tps = f.tparams.clone();
            let resolve_fn = |t: &TypeExpr| resolve_type(t, &names, &tps);
            m.functions.insert(f.name.clone(), signature(&f.params, &f.ret, &resolve_fn)?);
            m.fn_tparams.insert(f.name.clone(), f.tparams.clone());
        }
        for x in &prog.extern_fns {
            if BUILTINS.contains(&x.name.as_str()) || type_names.contains(x.name.as_str()) {
                return err(format!("`{}` is already a built-in function or a type", x.name), x.pos);
            }
            if m.functions.contains_key(&x.name) || m.extern_fns.contains_key(&x.name) {
                return err(format!("function `{}` is defined more than once", x.name), x.pos);
            }
            let sig = signature(&x.params, &x.ret, &resolve)?;
            let path = match (&x.path, &x.body) {
                (_, Some(_)) => format!("__x_{}", x.name),
                (Some(p), None) => p.clone(),
                (None, None) => x.name.clone(),
            };
            m.extern_fns.insert(x.name.clone(), (sig, path));
        }
        match m.functions.get("main") {
            None if require_main => return err("program has no `fn main()`", Pos { line: 1, col: 1, file: 0 }),
            Some(sig)
                if !sig.params.is_empty()
                    || !matches!(&sig.ret, Type::Void)
                        && !matches!(&sig.ret, Type::Result(t, e) if **t == Type::Void && m.is_plain(e)) =>
            {
                let pos = prog.functions.iter().find(|f| f.name == "main").unwrap().pos;
                return err("`main` must take no parameters and return nothing (or `Result<void, E>` with a printable `E`)", pos);
            }
            _ => {}
        }

        // 4. Classes: bases, fields, methods.
        for c in &prog.classes {
            check_tparams(&c.tparams, c.pos)?;
            let tps = c.tparams.clone();
            let resolve = |t: &TypeExpr| resolve_type(t, &names, &tps);
            if !c.tparams.is_empty() {
                if !c.bases.is_empty() {
                    return err("generic classes cannot extend classes or implement interfaces (yet)", c.bases[0].1);
                }
                if c.is_abstract || c.methods.iter().any(|m| m.is_static) {
                    return err("generic classes cannot be abstract or have `static fn`s (yet)", c.pos);
                }
            }
            for (b, pos) in &c.bases {
                if names.generic.contains_key(b) {
                    return err(format!("`{}` is generic and cannot be extended (yet)", b), *pos);
                }
            }
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
                if (ty.is_object() || matches!(ty, Type::Enum(_) | Type::Func(..) | Type::Struct(..) | Type::Extern(_) | Type::Result(..)))
                    && f.default.is_none()
                {
                    return err(
                        format!("field `{}` of type `{}` needs a default value (or make it optional: `{}?`)", f.name, ty, ty),
                        f.pos,
                    );
                }
                fields.push((f.name.clone(), ty));
            }
            let mut methods = HashMap::new();
            let mut statics = HashMap::new();
            let mut private_methods = HashSet::new();
            let mut abstract_methods = HashSet::new();
            for meth in &c.methods {
                if methods.contains_key(&meth.name) || statics.contains_key(&meth.name) {
                    return err(format!("method `{}` defined twice in `{}`", meth.name, c.name), meth.pos);
                }
                if meth.is_abstract && !c.is_abstract {
                    return err(
                        format!("`{}` has an abstract method, so it must be declared `abstract class {}`", c.name, c.name),
                        meth.pos,
                    );
                }
                if meth.is_abstract && (meth.is_private || meth.is_static) {
                    return err("an abstract method cannot be `private` or `static`", meth.pos);
                }
                if meth.is_static && meth.is_override {
                    return err("a `static fn` cannot override", meth.pos);
                }
                let sig = signature(&meth.params, &meth.ret, &resolve)?;
                if meth.is_private {
                    private_methods.insert(meth.name.clone());
                }
                if meth.is_abstract {
                    abstract_methods.insert(meth.name.clone());
                }
                if meth.is_static {
                    statics.insert(meth.name.clone(), sig);
                } else {
                    methods.insert(meth.name.clone(), sig);
                }
            }
            let private_fields: HashSet<String> = c.fields.iter().filter(|f| f.is_private).map(|f| f.name.clone()).collect();
            let init = match &c.init {
                Some(i) => Some(signature(&i.params, &TypeExpr::Void, &resolve)?),
                None => None,
            };
            m.classes.insert(
                c.name.clone(),
                ClassInfo {
                    decl: c.clone(),
                    parent,
                    interfaces,
                    fields,
                    methods,
                    statics,
                    private_fields,
                    private_methods,
                    abstract_methods,
                    init,
                    children: vec![],
                    tparams: c.tparams.clone(),
                },
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
        let mut sorted: Vec<&String> = prog.classes.iter().map(|c| &c.name).collect();
        sorted.sort_by_key(|n| m.ancestors(n).len());
        for n in sorted {
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
        m.names = names;
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
        for meth in info.decl.methods.iter().filter(|m| !m.is_static) {
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
        // Interface conformance (including methods inherited from base classes and super-interfaces).
        for direct in &info.interfaces {
            for iname in self.iface_closure(direct) {
                for (mname, isig) in &self.interfaces[&iname].methods {
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
                        Some((owner, _)) if self.classes[&owner].private_methods.contains(mname) => {
                            return err(
                                format!("`{}.{}` implements `{}` so it cannot be `private`", owner, mname, iname),
                                info.decl.pos,
                            )
                        }
                        _ => {}
                    }
                }
            }
        }
        // Concrete classes must implement every inherited abstract method.
        if !info.decl.is_abstract {
            for (mname, owner, _) in self.method_table(name) {
                if self.classes[&owner].abstract_methods.contains(&mname) {
                    return err(
                        format!(
                            "`{}` must implement abstract method `{}.{}` (or be declared `abstract class`)",
                            name, owner, mname
                        ),
                        info.decl.pos,
                    );
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
    fn iface_closure_of(&self, iface: &str) -> Vec<String> {
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

    /// `is_plain`, but type parameters count as plain (used for generic struct definitions).
    fn is_plain_params_ok(&self, t: &Type) -> bool {
        match t {
            Type::Param(_) => true,
            Type::Array(i) | Type::Optional(i) => self.is_plain_params_ok(i),
            Type::Map(k, v) => self.is_plain_params_ok(k) && self.is_plain_params_ok(v),
            Type::Struct(st, args) => self.plain_structs.contains(st) && args.iter().all(|a| self.is_plain_params_ok(a)),
            other => self.is_plain(other),
        }
    }

    /// A struct's type parameters mapped to `args` (empty for non-generic structs).
    pub fn struct_bindings(&self, st: &str, args: &[Type]) -> HashMap<String, Type> {
        self.structs[st].tparams.iter().cloned().zip(args.iter().cloned()).collect()
    }

    /// Does the enum or struct `current` hold `target` by value (not behind an array,
    /// map or object)? Such a type would be infinitely large.
    fn value_contains(&self, current: &str, target: &str, seen: &mut HashSet<String>) -> bool {
        if !seen.insert(current.to_string()) {
            return false;
        }
        fn direct(t: &Type, out: &mut Vec<String>) {
            match t {
                Type::Enum(e) | Type::Struct(e, _) => out.push(e.clone()),
                Type::Optional(i) => direct(i, out),
                _ => {}
            }
        }
        let mut inner = vec![];
        if let Some(en) = self.enums.get(current) {
            for (_, fs) in &en.variants {
                for (_, t) in fs {
                    direct(t, &mut inner);
                }
            }
        }
        if let Some(st) = self.structs.get(current) {
            for (_, t) in &st.fields {
                direct(t, &mut inner);
            }
        }
        inner.iter().any(|e| e == target || self.value_contains(e, target, seen))
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

/// Replaces type parameters using `map` (unknown parameters are left as they are).
pub fn subst(t: &Type, map: &HashMap<String, Type>) -> Type {
    match t {
        Type::Param(n) => map.get(n).cloned().unwrap_or_else(|| t.clone()),
        Type::Array(i) => Type::Array(Box::new(subst(i, map))),
        Type::Optional(i) => Type::Optional(Box::new(subst(i, map))),
        Type::Map(k, v) => Type::Map(Box::new(subst(k, map)), Box::new(subst(v, map))),
        Type::Func(ps, r) => Type::Func(ps.iter().map(|p| subst(p, map)).collect(), Box::new(subst(r, map))),
        Type::Generic(n, args) => Type::Generic(n.clone(), args.iter().map(|a| subst(a, map)).collect()),
        Type::Struct(n, args) => Type::Struct(n.clone(), args.iter().map(|a| subst(a, map)).collect()),
        Type::Result(t, e) => Type::Result(Box::new(subst(t, map)), Box::new(subst(e, map))),
        other => other.clone(),
    }
}

pub fn subst_sig(sig: &Signature, map: &HashMap<String, Type>) -> Signature {
    Signature { params: sig.params.iter().map(|(n, t)| (n.clone(), subst(t, map))).collect(), ret: subst(&sig.ret, map) }
}

/// Learns type-parameter bindings by matching `pattern` (may contain `T`) against `actual`.
pub fn unify(pattern: &Type, actual: &Type, map: &mut HashMap<String, Type>) {
    match (pattern, actual) {
        (_, Type::NoneLit | Type::Unknown) => {}
        (Type::Param(n), a) => {
            map.entry(n.clone()).or_insert_with(|| a.clone());
        }
        (Type::Array(p), Type::Array(a)) | (Type::Optional(p), Type::Optional(a)) => unify(p, a, map),
        (Type::Optional(p), a) => unify(p, a, map),
        (Type::Map(pk, pv), Type::Map(ak, av)) => {
            unify(pk, ak, map);
            unify(pv, av, map);
        }
        (Type::Func(pp, pr), Type::Func(ap, ar)) if pp.len() == ap.len() => {
            for (p, a) in pp.iter().zip(ap) {
                unify(p, a, map);
            }
            unify(pr, ar, map);
        }
        (Type::Result(pt, pe), Type::Result(at, ae)) => {
            unify(pt, at, map);
            unify(pe, ae, map);
        }
        (Type::Generic(pn, pa), Type::Generic(an, aa)) | (Type::Struct(pn, pa), Type::Struct(an, aa)) if pn == an => {
            for (p, a) in pa.iter().zip(aa) {
                unify(p, a, map);
            }
        }
        _ => {}
    }
}

pub fn has_params(t: &Type) -> bool {
    match t {
        Type::Param(_) => true,
        Type::Array(i) | Type::Optional(i) => has_params(i),
        Type::Map(k, v) => has_params(k) || has_params(v),
        Type::Func(ps, r) => ps.iter().any(has_params) || has_params(r),
        Type::Generic(_, args) | Type::Struct(_, args) => args.iter().any(has_params),
        Type::Result(t, e) => has_params(t) || has_params(e),
        _ => false,
    }
}

fn resolve_type(t: &TypeExpr, names: &TypeNames, tparams: &[String]) -> Result<Type> {
    Ok(match t {
        TypeExpr::Int => Type::Int,
        TypeExpr::Float => Type::Float,
        TypeExpr::Bool => Type::Bool,
        TypeExpr::Str => Type::Str,
        TypeExpr::Void => Type::Void,
        TypeExpr::Array(inner) => {
            let i = resolve_type(inner, names, tparams)?;
            if i == Type::Void {
                return err("array of void is not allowed", Pos::default());
            }
            Type::Array(Box::new(i))
        }
        TypeExpr::Map(k, v) => {
            let kt = resolve_type(k, names, tparams)?;
            if !is_map_key(&kt) {
                return err(format!("map keys must be `int`, `str` or `bool`, not `{}`", kt), Pos::default());
            }
            let vt = resolve_type(v, names, tparams)?;
            if vt == Type::Void {
                return err("map of void is not allowed", Pos::default());
            }
            Type::Map(Box::new(kt), Box::new(vt))
        }
        TypeExpr::Func(ps, r) => {
            let mut params = Vec::new();
            for p in ps {
                let t = resolve_type(p, names, tparams)?;
                if t == Type::Void {
                    return err("a function parameter cannot be void", Pos::default());
                }
                params.push(t);
            }
            Type::Func(params, Box::new(resolve_type(r, names, tparams)?))
        }
        TypeExpr::Optional(inner) => {
            let i = resolve_type(inner, names, tparams)?;
            if matches!(i, Type::Optional(_)) {
                return err("nested optional `T??` is not allowed", Pos::default());
            }
            Type::Optional(Box::new(i))
        }
        TypeExpr::Named(n, args, pos) => {
            if tparams.contains(n) {
                if !args.is_empty() {
                    return err(format!("type parameter `{}` does not take type arguments", n), *pos);
                }
                return Ok(Type::Param(n.clone()));
            }
            if n == "Result" {
                if args.len() != 2 {
                    return err("`Result` needs two types: `Result<T, E>` (value type, error type)", *pos);
                }
                let t = resolve_type(&args[0], names, tparams)?;
                let e = resolve_type(&args[1], names, tparams)?;
                if e == Type::Void {
                    return err("the error type of a `Result` cannot be void", *pos);
                }
                return Ok(Type::Result(Box::new(t), Box::new(e)));
            }
            if let Some(&arity) = names.struct_arity.get(n) {
                if args.len() != arity {
                    return err(format!("`{}` needs {} type argument(s), e.g. `{}<int>`", n, arity, n), *pos);
                }
                let mut targs = Vec::new();
                for a in args {
                    targs.push(resolve_type(a, names, tparams)?);
                }
                return Ok(Type::Struct(n.clone(), targs));
            }
            if let Some(&arity) = names.generic.get(n) {
                if args.len() != arity {
                    return err(format!("`{}` needs {} type argument(s), e.g. `{}<int>`", n, arity, n), *pos);
                }
                let mut targs = Vec::new();
                for a in args {
                    let t = resolve_type(a, names, tparams)?;
                    if t == Type::Void {
                        return err("`void` cannot be a type argument", *pos);
                    }
                    targs.push(t);
                }
                return Ok(Type::Generic(n.clone(), targs));
            }
            if !args.is_empty() {
                return err(format!("`{}` is not generic and takes no type arguments", n), *pos);
            }
            if names.classes.contains(n) {
                Type::Class(n.clone())
            } else if names.ifaces.contains(n) {
                Type::Interface(n.clone())
            } else if names.enums.contains(n) {
                Type::Enum(n.clone())
            } else if names.structs.contains(n) {
                Type::Struct(n.clone(), vec![])
            } else if names.externs.contains(n) {
                Type::Extern(n.clone())
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
