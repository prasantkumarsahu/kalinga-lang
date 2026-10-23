//! Type-checks function and method bodies and emits Rust source.
//!
//! Object model (see README): every object is `Rc<RefCell<Struct>>`. A class with
//! subclasses is referred to through `Rc<RefCell<dyn __I_Class>>` so calls dispatch
//! dynamically; leaf classes use the concrete struct for static dispatch.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use crate::ast::*;
use crate::checker::{class_trait, has_params, is_map_key, subst, subst_sig, unify, Model, Signature, StructOp, Type, BUILTINS};
use crate::error::{err, Pos, Result};

pub fn generate(prog: &Program, model: &Model) -> Result<String> {
    let g = Gen { m: model, self_values: RefCell::new(HashSet::new()), cast_targets: RefCell::new(HashSet::new()) };
    g.program(prog)
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "const", "dyn", "enum", "extern", "impl", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "static", "struct", "trait", "type", "unsafe", "use", "where", "yield",
    "abstract", "become", "box", "do", "final", "macro", "priv", "typeof", "unsized", "virtual", "try",
    "gen", "union",
];

/// Rust identifier for a user-chosen variable, field or method name.
fn rid(name: &str) -> String {
    if name == "crate" || name == "Self" {
        format!("{}_", name)
    } else if RUST_KEYWORDS.contains(&name) {
        format!("r#{}", name)
    } else if name.starts_with(|c: char| c.is_uppercase()) {
        // Keep locals out of the type/constructor namespace (`None`, `Some`, ...).
        format!("__u_{}", name)
    } else {
        name.to_string()
    }
}

/// Rust name for a user function.
fn fn_id(name: &str) -> String {
    format!("kl_{}", name)
}

#[derive(Clone)]
struct Var {
    ty: Type,
    mutable: bool,
    /// Seen from inside a lambda that captured it.
    captured: bool,
    rust: String,
}

struct Ctx {
    scopes: Vec<HashMap<String, Var>>,
    /// Class whose method/init body is being generated.
    class: Option<String>,
    ret: Type,
    in_init: bool,
    /// Struct whose method (or operator) body is being generated.
    strukt: Option<String>,
    /// Inside a struct's `mut fn`: `self` may be changed.
    self_mut: bool,
    /// Inside a `static fn`: no `self`.
    is_static: bool,
    loops: usize,
    /// Lambdas being generated, innermost last.
    lambdas: Vec<LambdaFrame>,
    /// Type parameters in scope (generic function or class).
    tparams: Vec<String>,
}

/// Variables from outside a lambda that its body uses (they are cloned into the closure).
struct LambdaFrame {
    /// Scopes at or above this index belong to the lambda.
    boundary: usize,
    captures: RefCell<Vec<String>>,
}

impl Ctx {
    fn new(class: Option<String>, ret: Type, in_init: bool) -> Ctx {
        Ctx {
            scopes: vec![HashMap::new()],
            class,
            ret,
            in_init,
            strukt: None,
            self_mut: false,
            is_static: false,
            loops: 0,
            lambdas: vec![],
            tparams: vec![],
        }
    }

    /// Finds a variable. Variables from outside the current lambda are recorded as
    /// captures and are read-only inside it.
    fn lookup(&self, name: &str) -> Option<Var> {
        let idx = (0..self.scopes.len()).rev().find(|i| self.scopes[*i].contains_key(name))?;
        let mut var = self.scopes[idx][name].clone();
        for frame in &self.lambdas {
            if idx < frame.boundary {
                var.mutable = false;
                var.captured = true;
                let mut caps = frame.captures.borrow_mut();
                if !caps.contains(&var.rust) {
                    caps.push(var.rust.clone());
                }
            }
        }
        Some(var)
    }

    fn declare(&mut self, name: &str, ty: Type, mutable: bool, pos: Pos) -> Result<String> {
        let scope = self.scopes.last_mut().unwrap();
        if scope.contains_key(name) {
            return err(format!("`{}` is already declared in this scope", name), pos);
        }
        let rust = rid(name);
        scope.insert(name.to_string(), Var { ty, mutable, captured: false, rust: rust.clone() });
        Ok(rust)
    }
}

struct Gen<'a> {
    m: &'a Model,
    /// Classes whose methods use `self` as a value (recorded while generating bodies).
    self_values: RefCell<HashSet<String>>,
    /// Classes used as the target of `as` / `is`.
    cast_targets: RefCell<HashSet<String>>,
}

fn clone_if(code: String, ty: &Type) -> String {
    if ty.is_copy() { code } else { format!("{}.clone()", code) }
}


fn indent(n: usize) -> String {
    "    ".repeat(n)
}

impl<'a> Gen<'a> {
    // ------------------------------------------------------------ types

    fn rty(&self, t: &Type) -> String {
        match t {
            Type::Int => "i64".into(),
            Type::Float => "f64".into(),
            Type::Bool => "bool".into(),
            Type::Str => "String".into(),
            Type::Void | Type::NoneLit => "()".into(),
            Type::Array(i) => format!("Vec<{}>", self.rty(i)),
            Type::Map(k, v) => format!("BTreeMap<{}, {}>", self.rty(k), self.rty(v)),
            Type::Optional(i) => format!("Option<{}>", self.rty(i)),
            Type::Class(c) => {
                if self.m.has_subclasses(c) {
                    format!("Rc<RefCell<dyn {}>>", class_trait(c))
                } else {
                    format!("Rc<RefCell<{}>>", c)
                }
            }
            Type::Interface(i) => format!("Rc<RefCell<dyn {}>>", i),
            Type::Enum(e) | Type::Struct(e) | Type::Extern(e) => e.clone(),
            Type::Func(ps, r) => {
                let ps: Vec<String> = ps.iter().map(|p| self.rty(p)).collect();
                if **r == Type::Void {
                    format!("Rc<dyn Fn({})>", ps.join(", "))
                } else {
                    format!("Rc<dyn Fn({}) -> {}>", ps.join(", "), self.rty(r))
                }
            }
            Type::Unknown => "_".into(),
            Type::Param(n) => n.clone(),
            Type::Generic(n, args) => {
                let a: Vec<String> = args.iter().map(|t| self.rty(t)).collect();
                format!("Rc<RefCell<{}<{}>>>", n, a.join(", "))
            }
        }
    }

    fn ret_suffix(&self, t: &Type) -> String {
        if *t == Type::Void { String::new() } else { format!(" -> {}", self.rty(t)) }
    }

    fn params_decl(&self, sig: &Signature) -> String {
        sig.params.iter().map(|(n, t)| format!("{}: {}", rid(n), self.rty(t))).collect::<Vec<_>>().join(", ")
    }

    fn params_names(&self, sig: &Signature) -> String {
        sig.params.iter().map(|(n, _)| rid(n)).collect::<Vec<_>>().join(", ")
    }

    /// Converts `code` of type `from` into type `to` (upcasts, optional wrapping).
    fn coerce(&self, code: String, from: &Type, to: &Type, pos: Pos) -> Result<String> {
        if from == to {
            return Ok(code);
        }
        if !self.m.assignable(from, to) {
            return err(format!("expected `{}`, found `{}`", to, from), pos);
        }
        Ok(match (from, to) {
            (Type::NoneLit, Type::Optional(_)) => "None".into(),
            (Type::Optional(a), Type::Optional(b)) => {
                format!("({}).map(|__v| {})", code, self.coerce("__v".into(), a, b, pos)?)
            }
            (f, Type::Optional(inner)) => format!("Some({})", self.coerce(code, f, inner, pos)?),
            (Type::Class(_), Type::Class(_)) | (Type::Class(_), Type::Interface(_)) | (Type::Interface(_), Type::Interface(_)) => {
                format!("({} as {})", code, self.rty(to))
            }
            (Type::Map(_, a), Type::Map(_, b)) => format!(
                "({}).into_iter().map(|(__k, __v)| (__k, {})).collect::<BTreeMap<_, _>>()",
                code,
                self.coerce("__v".into(), a, b, pos)?
            ),
            (Type::Array(a), Type::Array(b)) => format!(
                "({}).into_iter().map(|__v| {}).collect::<Vec<_>>()",
                code,
                self.coerce("__v".into(), a, b, pos)?
            ),
            _ => return err(format!("expected `{}`, found `{}`", to, from), pos),
        })
    }

    // ------------------------------------------------------------ program

    fn program(&self, prog: &Program) -> Result<String> {
        // Bodies come first: generating them records which classes need a self-pointer
        // (`self` as a value, `as`/`is` targets), which decides how classes are emitted.
        let (method_bodies, init_bodies, field_defaults) = self.class_bodies()?;
        let structs = self.struct_code()?;
        let mut functions = String::new();
        for f in &prog.functions {
            let sig = self.m.functions[&f.name].clone();
            let mut cx = Ctx::new(None, sig.ret.clone(), false);
            cx.tparams = f.tparams.clone();
            let body = self.body_in(&mut cx, &sig, &f.body, f.pos, &f.name, 1)?;
            writeln!(
                functions,
                "\nfn {}{}({}){} {{\n{}}}",
                fn_id(&f.name),
                generic_params(&f.tparams),
                self.params_decl(&sig),
                self.ret_suffix(&sig.ret),
                body
            )
            .unwrap();
        }

        let mut out = String::new();
        out.push_str(PRELUDE);
        for (path, _) in &prog.rust_files {
            let module = format!("__rs_{}", rust_module_name(path));
            writeln!(out, "\n#[path = {:?}]\nmod {};\nuse {}::*;", path, module, module).unwrap();
        }
        for x in prog.extern_fns.iter().filter(|x| x.body.is_some()) {
            let (sig, _) = &self.m.extern_fns[&x.name];
            writeln!(
                out,
                "\n// extern fn {} (inline Rust)\nfn __x_{}({}){} {{{}}}",
                x.name,
                x.name,
                self.params_decl(sig),
                self.ret_suffix(&sig.ret),
                x.body.as_ref().unwrap()
            )
            .unwrap();
        }

        for iname in &self.m.interface_order {
            let info = &self.m.interfaces[iname];
            writeln!(out, "\n// interface {}", iname).unwrap();
            let supers = &info.supers;
            let bound = if supers.is_empty() { String::new() } else { format!(": {}", supers.join(" + ")) };
            writeln!(out, "trait {}{} {{", iname, bound).unwrap();
            for (mname, sig) in &info.methods {
                writeln!(out, "    fn {}(&mut self{}){};", rid(mname), self.comma_params(sig), self.ret_suffix(&sig.ret)).unwrap();
            }
            for target in self.m.class_order.iter().filter(|c| self.m.all_interfaces(c).contains(iname) && self.is_cast_target(c)) {
                writeln!(out, "    fn __as_{}(&self) -> Option<{}> {{ None }}", target, self.rty(&Type::Class(target.clone()))).unwrap();
            }
            out.push_str("}\n");
        }

        for ename in &self.m.enum_order {
            self.enum_decl(&mut out, ename);
        }
        out.push_str(&structs);
        for cname in &self.m.class_order {
            self.class(&mut out, cname, &method_bodies, &init_bodies, &field_defaults);
        }
        out.push_str(&functions);
        writeln!(out, "\nfn main() {{\n    __kl_run({});\n}}", fn_id("main")).unwrap();
        Ok(out)
    }

    /// Method, init and field-default code for every class. Bodies are generated once per
    /// defining class and pasted into every subclass.
    #[allow(clippy::type_complexity)]
    fn class_bodies(
        &self,
    ) -> Result<(HashMap<(String, String), String>, HashMap<String, String>, HashMap<(String, String), String>)> {
        let mut method_bodies: HashMap<(String, String), String> = HashMap::new();
        let mut init_bodies: HashMap<String, String> = HashMap::new();
        let mut field_defaults: HashMap<(String, String), String> = HashMap::new();
        for cname in &self.m.class_order {
            let info = &self.m.classes[cname];
            for f in &info.decl.fields {
                let ty = info.fields.iter().find(|(n, _)| n == &f.name).unwrap().1.clone();
                let code = match &f.default {
                    Some(e) => {
                        let mut cx = Ctx::new(None, Type::Void, false);
                        cx.tparams = info.tparams.clone();
                        self.expr_to(&mut cx, e, &ty)?
                    }
                    None => self.default_value(&ty),
                };
                field_defaults.insert((cname.clone(), f.name.clone()), code);
            }
            for meth in &info.decl.methods {
                if meth.is_abstract {
                    method_bodies.insert((cname.clone(), meth.name.clone()), "        unreachable!(\"abstract method\")\n".into());
                    continue;
                }
                let sig = if meth.is_static { info.statics[&meth.name].clone() } else { info.methods[&meth.name].clone() };
                let mut cx = Ctx::new(Some(cname.clone()), sig.ret.clone(), false);
                cx.tparams = info.tparams.clone();
                cx.is_static = meth.is_static;
                let body = self.body_in(&mut cx, &sig, &meth.body, meth.pos, &meth.name, 2)?;
                let key = if meth.is_static { format!("static {}", meth.name) } else { meth.name.clone() };
                method_bodies.insert((cname.clone(), key), body);
            }
            if let Some(init) = &info.decl.init {
                let sig = info.init.clone().unwrap();
                let mut cx = Ctx::new(Some(cname.clone()), Type::Void, true);
                cx.tparams = info.tparams.clone();
                let body = self.body_in(&mut cx, &sig, &init.body, init.pos, "init", 2)?;
                init_bodies.insert(cname.clone(), body);
            }
        }
        Ok((method_bodies, init_bodies, field_defaults))
    }

    /// Rust code for every struct: definition, constructor, methods, operators, printing.
    fn struct_code(&self) -> Result<String> {
        let mut out = String::new();
        for name in &self.m.struct_order {
            let info = &self.m.structs[name];
            let plain = self.m.plain_structs.contains(name);
            writeln!(out, "\n// ===== struct {} =====", name).unwrap();
            writeln!(out, "#[derive(Clone{})]", if plain { ", PartialEq" } else { "" }).unwrap();
            writeln!(out, "struct {} {{", name).unwrap();
            for (f, t) in &info.fields {
                writeln!(out, "    {}: {},", rid(f), self.rty(t)).unwrap();
            }
            out.push_str("}\n");
            writeln!(out, "impl {} {{", name).unwrap();
            for meth in &info.decl.methods {
                let m = &info.methods[&meth.name];
                let mut cx = Ctx::new(None, m.sig.ret.clone(), false);
                cx.strukt = Some(name.clone());
                cx.self_mut = m.is_mut;
                cx.is_static = m.is_static;
                let body = self.body_in(&mut cx, &m.sig, &meth.body, meth.pos, &meth.name, 2)?;
                if m.is_static {
                    writeln!(out, "    fn __s_{}({}){} {{\n{}    }}", meth.name, self.params_decl(&m.sig), self.ret_suffix(&m.sig.ret), body).unwrap();
                } else {
                    let recv = if m.is_mut { "&mut self" } else { "&self" };
                    writeln!(
                        out,
                        "    fn {}({}{}){} {{\n{}    }}",
                        rid(&meth.name),
                        recv,
                        self.comma_params(&m.sig),
                        self.ret_suffix(&m.sig.ret),
                        body
                    )
                    .unwrap();
                }
            }
            for (o, decl) in info.ops.iter().zip(&info.decl.ops) {
                let sig = Signature {
                    params: decl.params.iter().map(|p| p.name.clone()).zip(o.param.iter().cloned()).collect(),
                    ret: o.ret.clone(),
                };
                let mut cx = Ctx::new(None, o.ret.clone(), false);
                cx.strukt = Some(name.clone());
                let body = self.body_in(&mut cx, &sig, &decl.body, decl.pos, &format!("op {}", o.op.rust()), 2)?;
                writeln!(out, "    fn __op_{}(&self{}){} {{\n{}    }}", o.index, self.comma_params(&sig), self.ret_suffix(&o.ret), body).unwrap();
            }
            out.push_str("}\n");
            if plain {
                let parts: Vec<String> = info.fields.iter().map(|(f, _)| format!("{}: {{}}", f)).collect();
                let shows: Vec<String> = info.fields.iter().map(|(f, _)| format!("self.{}.__show()", rid(f))).collect();
                writeln!(
                    out,
                    "impl __KlShow for {} {{\n    fn __show(&self) -> String {{ format!(\"{}({})\"{}{}) }}\n}}",
                    name,
                    name,
                    parts.join(", "),
                    if shows.is_empty() { "" } else { ", " },
                    shows.join(", ")
                )
                .unwrap();
            }
        }
        Ok(out)
    }

    /// Type of struct field `f`, checking `private` access.
    fn struct_field_type(&self, cx: &Ctx, st: &str, f: &str, pos: Pos) -> Result<Type> {
        let info = &self.m.structs[st];
        match info.fields.iter().find(|(n, _)| n == f) {
            Some((_, t)) => {
                if info.private_fields.contains(f) && cx.strukt.as_deref() != Some(st) {
                    return err(format!("field `{}` is private to `{}`", f, st), pos);
                }
                Ok(t.clone())
            }
            None if info.methods.contains_key(f) => err(format!("`{}` is a method of `{}`; call it with `()`", f, st), pos),
            None => err(format!("`{}` has no field `{}`", st, f), pos),
        }
    }

    /// The operator method of struct `st` for `op` with a right-hand side of type `rhs`.
    fn find_op(&self, st: &str, op: BinOp, rhs: Option<&Type>) -> Option<StructOp> {
        let ops = &self.m.structs[st].ops;
        let candidates = ops.iter().filter(|o| o.op == op);
        match rhs {
            None => candidates.into_iter().find(|o| o.param.is_none()).cloned(),
            Some(rt) => {
                let all: Vec<&StructOp> = candidates.filter(|o| o.param.is_some()).collect();
                all.iter().find(|o| o.param.as_ref() == Some(rt)).or_else(|| all.iter().find(|o| self.m.assignable(rt, o.param.as_ref().unwrap()))).map(|o| (*o).clone())
            }
        }
    }

    /// Calls `method` on a struct value: on `self`, on a place (for `mut fn`), or on any value.
    fn struct_method_call(&self, cx: &mut Ctx, st: &str, recv: &Expr, name: &str, args: &[Expr], pos: Pos) -> Result<(String, Type)> {
        let info = &self.m.structs[st];
        let m = match info.methods.get(name) {
            Some(m) if !m.is_static => m.clone(),
            Some(_) => return err(format!("`{}` is a `static fn`; call it as `{}.{}(...)`", name, st, name), pos),
            None => return err(format!("`{}` has no method `{}`", st, name), pos),
        };
        if m.is_private && cx.strukt.as_deref() != Some(st) {
            return err(format!("method `{}` is private to `{}`", name, st), pos);
        }
        let (binds, names) = self.args(cx, &m.sig, args, pos, name)?;
        if matches!(recv.kind, ExprKind::SelfRef) {
            if m.is_mut && !cx.self_mut {
                return err(format!("`{}` changes `self`, so it can only be called from another `mut fn`", name), pos);
            }
            return Ok((format!("{{ {}self.{}({}) }}", binds, rid(name), names), m.sig.ret));
        }
        if m.is_mut {
            let mut n = 0;
            let (pre, place, _) = self.place(cx, recv, &mut n).map_err(|mut e| {
                e.msg = format!("{} (`{}` is a `mut fn`, so it needs a variable it can change)", e.msg, name);
                e
            })?;
            return Ok((format!("{{ {}{}let __t = {}.{}({}); __t }}", pre, binds, place, rid(name), names), m.sig.ret));
        }
        let (setup, rcode, _) = self.receiver(cx, recv)?;
        Ok((format!("{{ {}{}let __t = {}.{}({}); __t }}", setup, binds, rcode, rid(name), names), m.sig.ret))
    }

    /// `Math`, `Random` or `Time` used as a receiver (unless a variable shadows it).
    fn library_receiver(&self, cx: &Ctx, recv: &Expr) -> Option<String> {
        match &recv.kind {
            ExprKind::Ident(n) if matches!(n.as_str(), "Math" | "Random" | "Time") && cx.lookup(n).is_none() => Some(n.clone()),
            _ => None,
        }
    }

    /// A numeric argument as an `f64` (ints are converted).
    fn float_arg(&self, cx: &mut Ctx, e: &Expr, what: &str) -> Result<String> {
        match self.expr(cx, e, None)? {
            (c, Type::Float) => Ok(c),
            (c, Type::Int) => Ok(format!("(({}) as f64)", c)),
            (_, t) => err(format!("`{}` needs a number, found `{}`", what, t), e.pos),
        }
    }

    /// Arguments of the same numeric type (`int` or `float`) for min/max/clamp/abs/sign.
    fn same_number_args(&self, cx: &mut Ctx, args: &[Expr], what: &str) -> Result<(Vec<String>, Type)> {
        let mut codes = vec![];
        let mut ty: Option<Type> = None;
        for a in args {
            let (c, t) = self.expr(cx, a, None)?;
            if !matches!(t, Type::Int | Type::Float) {
                return err(format!("`{}` needs numbers, found `{}`", what, t), a.pos);
            }
            if let Some(prev) = &ty {
                if *prev != t {
                    return err(format!("`{}` needs all `int` or all `float` arguments; convert with `float(x)`", what), a.pos);
                }
            }
            ty = Some(t);
            codes.push(c);
        }
        Ok((codes, ty.unwrap_or(Type::Float)))
    }

    fn library_call(&self, cx: &mut Ctx, ns: &str, name: &str, args: &[Expr], pos: Pos) -> Result<(String, Type)> {
        let what = format!("{}.{}", ns, name);
        let arity = |n: usize| -> Result<()> {
            if args.len() == n { Ok(()) } else { err(format!("`{}` takes {} argument(s), but {} were given", what, n, args.len()), pos) }
        };
        match (ns, name) {
            ("Math", "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "ln" | "log10" | "floor" | "ceil" | "round") => {
                arity(1)?;
                let x = self.float_arg(cx, &args[0], &what)?;
                let f = if name == "ln" { "ln" } else { name };
                Ok((format!("({}).{}()", x, f), Type::Float))
            }
            ("Math", "pow" | "atan2" | "hypot") => {
                arity(2)?;
                let a = self.float_arg(cx, &args[0], &what)?;
                let b = self.float_arg(cx, &args[1], &what)?;
                let f = if name == "pow" { "powf" } else { name };
                Ok((format!("({}).{}({})", a, f, b), Type::Float))
            }
            ("Math", "lerp") => {
                arity(3)?;
                let a = self.float_arg(cx, &args[0], &what)?;
                let b = self.float_arg(cx, &args[1], &what)?;
                let t = self.float_arg(cx, &args[2], &what)?;
                Ok((format!("{{ let __a = {}; __a + ({} - __a) * {} }}", a, b, t), Type::Float))
            }
            ("Math", "abs" | "sign") => {
                arity(1)?;
                let (c, t) = self.same_number_args(cx, args, &what)?;
                let f = if name == "abs" { "abs" } else { "signum" };
                Ok((format!("({}).{}()", c[0], f), t))
            }
            ("Math", "min" | "max") => {
                arity(2)?;
                let (c, t) = self.same_number_args(cx, args, &what)?;
                Ok((format!("({}).{}({})", c[0], name, c[1]), t))
            }
            ("Math", "clamp") => {
                arity(3)?;
                let (c, t) = self.same_number_args(cx, args, &what)?;
                Ok((format!("__clamp({}, {}, {})", c[0], c[1], c[2]), t))
            }
            ("Random", "int") => {
                arity(2)?;
                let lo = self.expr_to(cx, &args[0], &Type::Int)?;
                let hi = self.expr_to(cx, &args[1], &Type::Int)?;
                Ok((format!("__rand_int({}, {})", lo, hi), Type::Int))
            }
            ("Random", "float") => {
                arity(0)?;
                Ok(("__rand_float()".into(), Type::Float))
            }
            ("Random", "chance") => {
                arity(1)?;
                let p = self.float_arg(cx, &args[0], &what)?;
                Ok((format!("(__rand_float() < {})", p), Type::Bool))
            }
            ("Random", "seed") => {
                arity(1)?;
                let n = self.expr_to(cx, &args[0], &Type::Int)?;
                Ok((format!("__rand_seed({})", n), Type::Void))
            }
            ("Random", "pick") => {
                arity(1)?;
                let (setup, rcode, t) = self.receiver(cx, &args[0])?;
                let Type::Array(elem) = t else {
                    return err(format!("`Random.pick` needs an array, found `{}`", t), args[0].pos);
                };
                Ok((format!("{{ {}let __xs = &{}; if __xs.is_empty() {{ None }} else {{ Some(__xs[__rand_int(0, __xs.len() as i64) as usize].clone()) }} }}", setup, rcode), Type::Optional(elem)))
            }
            ("Random", "shuffle") => {
                arity(1)?;
                let mut n = 0;
                let (pre, place, t) = self.place(cx, &args[0], &mut n)?;
                if !matches!(t, Type::Array(_)) {
                    return err(format!("`Random.shuffle` needs an array, found `{}`", t), args[0].pos);
                }
                Ok((format!("{{ {}__shuffle(&mut {}); }}", pre, place), Type::Void))
            }
            ("Time", "now") => {
                arity(0)?;
                Ok(("__time_now()".into(), Type::Float))
            }
            ("Time", "millis") => {
                arity(0)?;
                Ok(("__time_millis()".into(), Type::Int))
            }
            ("Time", "sleep") => {
                arity(1)?;
                let secs = self.float_arg(cx, &args[0], &what)?;
                Ok((format!("__time_sleep({})", secs), Type::Void))
            }
            _ => err(format!("`{}` has no function `{}`", ns, name), pos),
        }
    }

    fn is_cast_target(&self, class: &str) -> bool {
        self.cast_targets.borrow().contains(class)
    }

    /// Objects of `class` store a weak pointer to themselves only when something needs it:
    /// a method (here or inherited) that uses `self` as a value, or `as`/`is` to this class
    /// or one of its bases.
    fn needs_self_ptr(&self, class: &str) -> bool {
        self.m.ancestors(class).iter().any(|a| self.self_values.borrow().contains(a) || self.is_cast_target(a))
    }

    fn enum_decl(&self, out: &mut String, name: &str) {
        let info = &self.m.enums[name];
        let plain = self.m.plain_enums.contains(name);
        writeln!(out, "\n// enum {}", name).unwrap();
        writeln!(out, "#[derive(Clone{})]", if plain { ", PartialEq" } else { "" }).unwrap();
        writeln!(out, "enum {} {{", name).unwrap();
        for (v, fields) in &info.variants {
            if fields.is_empty() {
                writeln!(out, "    {},", v).unwrap();
            } else {
                let ts: Vec<String> = fields.iter().map(|(_, t)| self.rty(t)).collect();
                writeln!(out, "    {}({}),", v, ts.join(", ")).unwrap();
            }
        }
        out.push_str("}\n");
        if plain {
            writeln!(out, "impl __KlShow for {} {{\n    fn __show(&self) -> String {{\n        match self {{", name).unwrap();
            for (v, fields) in &info.variants {
                if fields.is_empty() {
                    writeln!(out, "            {}::{} => String::from({:?}),", name, v, v).unwrap();
                } else {
                    let binds: Vec<String> = (0..fields.len()).map(|i| format!("__f{}", i)).collect();
                    let shows: Vec<String> = binds.iter().map(|b| format!("{}.__show()", b)).collect();
                    let holes = vec!["{}"; fields.len()].join(", ");
                    writeln!(
                        out,
                        "            {}::{}({}) => format!(\"{}({})\", {}),",
                        name,
                        v,
                        binds.join(", "),
                        v,
                        holes,
                        shows.join(", ")
                    )
                    .unwrap();
                }
            }
            out.push_str("        }\n    }\n}\n");
        }
    }

    /// `Shape.Circle(args)` or `Shape.Empty`, when `type_name` is an enum.
    fn variant_value(&self, cx: &mut Ctx, type_name: &str, variant: &str, args: Option<&[Expr]>, pos: Pos) -> Result<(String, Type)> {
        let info = &self.m.enums[type_name];
        let fields = match info.variant(variant) {
            Some(f) => f.clone(),
            None => return err(format!("enum `{}` has no variant `{}`", type_name, variant), pos),
        };
        let ty = Type::Enum(type_name.to_string());
        match args {
            None if fields.is_empty() => Ok((format!("{}::{}", type_name, variant), ty)),
            None => err(
                format!("`{}.{}` needs {} value(s): `{}.{}(...)`", type_name, variant, fields.len(), type_name, variant),
                pos,
            ),
            Some(_) if fields.is_empty() => err(format!("`{}.{}` takes no values; write it without `()`", type_name, variant), pos),
            Some(args) => {
                let sig = Signature { params: fields, ret: Type::Void };
                let (binds, names) = self.args(cx, &sig, args, pos, variant)?;
                Ok((format!("{{ {}{}::{}({}) }}", binds, type_name, variant, names), ty))
            }
        }
    }

    /// Type name used as a receiver (`Shape.Circle`), unless a variable shadows it.
    fn type_receiver(&self, cx: &Ctx, recv: &Expr) -> Option<String> {
        match &recv.kind {
            ExprKind::Ident(n) if cx.lookup(n).is_none() && self.m.enums.contains_key(n) => Some(n.clone()),
            _ => None,
        }
    }

    /// Classes sharing `root` as their topmost base class (including `root`).
    fn hierarchy(&self, root: &str) -> Vec<String> {
        self.m.class_order.iter().filter(|c| self.m.ancestors(c).last().map(String::as_str) == Some(root)).cloned().collect()
    }

    /// `fn __as_T(&self)` that succeeds, for an object whose class is `T` or derives from it.
    fn downcast_hook(&self, out: &mut String, target: &str) {
        let ty = self.rty(&Type::Class(target.to_string()));
        writeln!(out, "    fn __as_{}(&self) -> Option<{}> {{ self.__self.upgrade().map(|__r| __r as {}) }}", target, ty, ty).unwrap();
    }

    fn comma_params(&self, sig: &Signature) -> String {
        if sig.params.is_empty() { String::new() } else { format!(", {}", self.params_decl(sig)) }
    }

    fn default_value(&self, t: &Type) -> String {
        match t {
            Type::Int => "0i64".into(),
            Type::Float => "0.0f64".into(),
            Type::Bool => "false".into(),
            Type::Str => "String::new()".into(),
            Type::Array(_) => "Vec::new()".into(),
            Type::Map(..) => "BTreeMap::new()".into(),
            Type::Param(_) => "None".into(),
            Type::Optional(_) => "None".into(),
            // Checker requires an explicit default for object fields.
            _ => unreachable!("no default for {}", t),
        }
    }

    fn class(
        &self,
        out: &mut String,
        cname: &str,
        method_bodies: &HashMap<(String, String), String>,
        init_bodies: &HashMap<String, String>,
        field_defaults: &HashMap<(String, String), String>,
    ) {
        let m = self.m;
        let info = &m.classes[cname];
        let fields = m.all_fields(cname);
        let ancestors = m.ancestors(cname);

        let generic = !info.tparams.is_empty();
        let self_ty = if generic { format!("{}<{}>", cname, info.tparams.join(", ")) } else { cname.to_string() };

        writeln!(out, "\n// ===== class {} =====", cname).unwrap();
        // struct
        let self_ptr = self.needs_self_ptr(cname);
        writeln!(out, "struct {} {{", self_ty).unwrap();
        if self_ptr {
            writeln!(out, "    __self: std::rc::Weak<RefCell<{}>>,", self_ty).unwrap();
        }
        for (f, t, _) in &fields {
            let rt = if matches!(t, Type::Param(_)) { format!("Option<{}>", self.rty(t)) } else { self.rty(t) };
            writeln!(out, "    {}: {},", rid(f), rt).unwrap();
        }
        out.push_str("}\n");

        if generic {
            writeln!(out, "impl{} {} {{", generic_params(&info.tparams), self_ty).unwrap();
        } else {
        // trait for dynamic dispatch
        let mut supers: Vec<String> = vec![];
        if let Some(p) = &info.parent {
            supers.push(class_trait(p));
        }
        supers.extend(info.interfaces.iter().cloned());
        let bound = if supers.is_empty() { String::new() } else { format!(": {}", supers.join(" + ")) };
        writeln!(out, "trait {}{} {{", class_trait(cname), bound).unwrap();
        for (mname, sig) in m.new_trait_methods(cname) {
            writeln!(out, "    fn {}(&mut self{}){};", rid(&mname), self.comma_params(&sig), self.ret_suffix(&sig.ret)).unwrap();
        }
        for (f, t) in &info.fields {
            writeln!(out, "    fn __get_{}(&self) -> {};", f, self.rty(t)).unwrap();
            writeln!(out, "    fn __set_{}(&mut self, v: {});", f, self.rty(t)).unwrap();
        }
        if info.parent.is_none() {
            for target in self.hierarchy(cname).iter().filter(|t| self.is_cast_target(t)) {
                writeln!(out, "    fn __as_{}(&self) -> Option<{}> {{ None }}", target, self.rty(&Type::Class(target.clone()))).unwrap();
            }
        }
        out.push_str("}\n");

        // inherent impl
        writeln!(out, "impl {} {{", cname).unwrap();
        }
        let ctor = m.find_init(cname);
        let ctor_params = ctor.as_ref().map(|(_, s)| self.params_decl(s)).unwrap_or_default();
        writeln!(out, "    fn __new({}) -> Rc<RefCell<{}>> {{", ctor_params, self_ty).unwrap();
        if self_ptr {
            out.push_str("        Rc::new_cyclic(|__w| {\n");
        } else {
            out.push_str("        Rc::new({\n");
        }
        writeln!(out, "            let mut __o = {} {{", cname).unwrap();
        if self_ptr {
            out.push_str("                __self: __w.clone(),\n");
        }
        for (f, _, owner) in &fields {
            writeln!(out, "                {}: {},", rid(f), field_defaults[&(owner.clone(), f.clone())]).unwrap();
        }
        out.push_str("            };\n");
        if let Some((owner, sig)) = &ctor {
            writeln!(out, "            __o.__{}__init({});", owner, self.params_names(sig)).unwrap();
        }
        out.push_str("            RefCell::new(__o)\n        })\n    }\n");
        for a in &ancestors {
            let ainfo = &m.classes[a];
            if let Some(sig) = &ainfo.init {
                writeln!(out, "    fn __{}__init(&mut self{}) {{\n{}    }}", a, self.comma_params(sig), init_bodies[a]).unwrap();
            }
            let mut names: Vec<&String> = ainfo.methods.keys().collect();
            names.sort();
            for mname in names {
                let sig = &ainfo.methods[mname];
                writeln!(
                    out,
                    "    fn __{}__{}(&mut self{}){} {{\n{}    }}",
                    a,
                    mname,
                    self.comma_params(sig),
                    self.ret_suffix(&sig.ret),
                    method_bodies[&(a.clone(), mname.clone())]
                )
                .unwrap();
            }
        }
        let mut statics: Vec<(&String, &Signature)> = info.statics.iter().collect();
        statics.sort_by(|a, b| a.0.cmp(b.0));
        for (mname, sig) in statics {
            writeln!(
                out,
                "    fn __s_{}({}){} {{\n{}    }}",
                mname,
                self.params_decl(sig),
                self.ret_suffix(&sig.ret),
                method_bodies[&(cname.to_string(), format!("static {}", mname))]
            )
            .unwrap();
        }
        // Virtual methods resolve to the most-derived body.
        for (mname, owner, sig) in m.method_table(cname) {
            writeln!(
                out,
                "    fn {}(&mut self{}){} {{ self.__{}__{}({}) }}",
                rid(&mname),
                self.comma_params(&sig),
                self.ret_suffix(&sig.ret),
                owner,
                mname,
                self.params_names(&sig)
            )
            .unwrap();
        }
        out.push_str("}\n");

        if generic {
            return;
        }

        // trait impls
        let forward = |out: &mut String, mname: &str, sig: &Signature| {
            let names = self.params_names(sig);
            let args = if names.is_empty() { String::new() } else { format!(", {}", names) };
            writeln!(
                out,
                "    fn {}(&mut self{}){} {{ {}::{}(self{}) }}",
                rid(mname),
                self.comma_params(sig),
                self.ret_suffix(&sig.ret),
                cname,
                rid(mname),
                args
            )
            .unwrap();
        };
        for a in &ancestors {
            writeln!(out, "impl {} for {} {{", class_trait(a), cname).unwrap();
            for (mname, sig) in m.new_trait_methods(a) {
                forward(out, &mname, &sig);
            }
            for (f, t) in &m.classes[a].fields {
                let get = clone_if(format!("self.{}", rid(f)), t);
                writeln!(out, "    fn __get_{}(&self) -> {} {{ {} }}", f, self.rty(t), get).unwrap();
                writeln!(out, "    fn __set_{}(&mut self, v: {}) {{ self.{} = v; }}", f, self.rty(t), rid(f)).unwrap();
            }
            if m.classes[a].parent.is_none() {
                for target in ancestors.iter().filter(|t| self.is_cast_target(t)) {
                    self.downcast_hook(out, target);
                }
            }
            out.push_str("}\n");
        }
        for iname in m.all_interfaces(cname) {
            writeln!(out, "impl {} for {} {{", iname, cname).unwrap();
            for (mname, sig) in &m.interfaces[&iname].methods {
                forward(out, mname, sig);
            }
            for target in ancestors.iter().filter(|t| m.all_interfaces(t).contains(&iname) && self.is_cast_target(t)) {
                self.downcast_hook(out, target);
            }
            out.push_str("}\n");
        }
    }

    /// Generates the statements of a function, method, init or lambda body.
    fn body_in(&self, cx: &mut Ctx, sig: &Signature, stmts: &[Stmt], pos: Pos, name: &str, depth: usize) -> Result<String> {
        for (p, t) in &sig.params {
            cx.declare(p, t.clone(), false, pos)?;
        }
        if sig.ret != Type::Void && !always_returns(stmts) {
            return err(format!("`{}` must return a `{}` on every path", name, sig.ret), pos);
        }
        self.block_inner(cx, stmts, depth)
    }

    // ------------------------------------------------------------ statements

    fn block_inner(&self, cx: &mut Ctx, stmts: &[Stmt], depth: usize) -> Result<String> {
        let mut out = String::new();
        for s in stmts {
            let code = self.stmt(cx, s, depth)?;
            out.push_str(&indent(depth));
            out.push_str(&code);
            out.push('\n');
        }
        Ok(out)
    }

    fn block(&self, cx: &mut Ctx, stmts: &[Stmt], depth: usize) -> Result<String> {
        cx.scopes.push(HashMap::new());
        let r = self.block_inner(cx, stmts, depth + 1);
        cx.scopes.pop();
        Ok(format!("{{\n{}{}}}", r?, indent(depth)))
    }

    fn cond(&self, cx: &mut Ctx, e: &Expr) -> Result<String> {
        let (code, t) = self.expr(cx, e, None)?;
        if t != Type::Bool {
            return err(format!("condition must be `bool`, found `{}`", t), e.pos);
        }
        Ok(code)
    }

    fn stmt(&self, cx: &mut Ctx, s: &Stmt, depth: usize) -> Result<String> {
        match &s.kind {
            StmtKind::Let { name, mutable, ty, value } => {
                let (code, t) = match ty {
                    Some(te) => {
                        let t = self.m.resolve(te, &cx.tparams).map_err(|mut e| {
                            if e.pos == Pos::default() {
                                e.pos = s.pos;
                            }
                            e
                        })?;
                        (self.expr_to(cx, value, &t)?, t)
                    }
                    None => self.expr(cx, value, None)?,
                };
                match t {
                    Type::Void => return err("cannot store the result of a function that returns nothing", value.pos),
                    Type::NoneLit => return err("cannot infer the type of `none`; add a type, e.g. `let x: int? = none`", value.pos),
                    _ => {}
                }
                let rust = cx.declare(name, t.clone(), *mutable, s.pos)?;
                let kw = if *mutable { "let mut" } else { "let" };
                Ok(format!("{} {}: {} = {};", kw, rust, self.rty(&t), code))
            }
            StmtKind::Assign { target, op, value } => self.assign(cx, target, *op, value),
            StmtKind::If { cond, then, otherwise } => {
                let c = self.cond(cx, cond)?;
                let t = self.block(cx, then, depth)?;
                let mut out = format!("if {} {}", c, t);
                if let Some(o) = otherwise {
                    write!(out, " else {}", self.block(cx, o, depth)?).unwrap();
                }
                Ok(out)
            }
            StmtKind::IfLet { name, value, then, otherwise } => {
                let (code, t) = self.expr(cx, value, None)?;
                let inner = match t {
                    Type::Optional(i) => *i,
                    other => return err(format!("`if let` needs an optional value (`T?`), found `{}`", other), value.pos),
                };
                cx.scopes.push(HashMap::new());
                let rust = cx.declare(name, inner, false, s.pos)?;
                let body = self.block(cx, then, depth);
                cx.scopes.pop();
                let mut out = format!("if let Some({}) = {} {}", rust, code, body?);
                if let Some(o) = otherwise {
                    write!(out, " else {}", self.block(cx, o, depth)?).unwrap();
                }
                Ok(out)
            }
            StmtKind::Match { value, arms } => self.match_stmt(cx, value, arms, s.pos, depth),
            StmtKind::While { cond, body } => {
                let infinite = matches!(cond.kind, ExprKind::Bool(true));
                let c = self.cond(cx, cond)?;
                cx.loops += 1;
                let b = self.block(cx, body, depth);
                cx.loops -= 1;
                if infinite { Ok(format!("loop {}", b?)) } else { Ok(format!("while {} {}", c, b?)) }
            }
            StmtKind::For { var, iter, body } => {
                let (head, elem_ty, wrap_char) = match iter {
                    ForIter::Range(a, b) => {
                        let ca = self.expr_to(cx, a, &Type::Int)?;
                        let cb = self.expr_to(cx, b, &Type::Int)?;
                        (format!("({})..({})", ca, cb), Type::Int, false)
                    }
                    ForIter::Each(e) => {
                        let (code, t) = self.expr(cx, e, None)?;
                        match t {
                            Type::Array(inner) => (code, *inner, false),
                            Type::Map(k, _) => (format!("({}).into_keys()", code), *k, false),
                            Type::Str => (format!("({}).chars()", code), Type::Str, true),
                            other => return err(format!("cannot loop over `{}` (expected an array, string or `a..b` range)", other), e.pos),
                        }
                    }
                };
                cx.scopes.push(HashMap::new());
                let rust = cx.declare(var, elem_ty, false, s.pos)?;
                cx.loops += 1;
                let b = self.block(cx, body, depth);
                cx.loops -= 1;
                cx.scopes.pop();
                if wrap_char {
                    Ok(format!("for __c in {} {{ let {} = __c.to_string(); {} }}", head, rust, b?))
                } else {
                    Ok(format!("for {} in {} {}", rust, head, b?))
                }
            }
            StmtKind::Return(value) => match (value, cx.ret.clone()) {
                (None, Type::Void) => Ok("return;".into()),
                (None, t) => err(format!("this function must return a `{}`", t), s.pos),
                (Some(v), Type::Void) => err("this function does not return a value", v.pos),
                (Some(v), t) => Ok(format!("return {};", self.expr_to(cx, v, &t)?)),
            },
            StmtKind::Break | StmtKind::Continue => {
                if cx.loops == 0 {
                    return err("`break`/`continue` outside of a loop", s.pos);
                }
                Ok(if matches!(s.kind, StmtKind::Break) { "break;" } else { "continue;" }.into())
            }
            StmtKind::Expr(e) => {
                if let ExprKind::SuperInit(args) = &e.kind {
                    return self.super_init(cx, args, e.pos);
                }
                let (code, _) = self.expr(cx, e, None)?;
                Ok(format!("{};", code))
            }
        }
    }

    fn match_stmt(&self, cx: &mut Ctx, value: &Expr, arms: &[MatchArm], pos: Pos, depth: usize) -> Result<String> {
        let (code, ty) = self.expr(cx, value, None)?;
        let enum_info = match &ty {
            Type::Enum(e) => Some((e.clone(), self.m.enums[e].clone())),
            Type::Int | Type::Str | Type::Bool => None,
            other => return err(format!("cannot `match` on `{}` (use an enum, int, str or bool)", other), value.pos),
        };
        let scrutinee = if ty == Type::Str { format!("({}).as_str()", code) } else { code };
        let mut out = format!("match {} {{\n", scrutinee);
        let mut covered: Vec<String> = vec![];
        let mut wildcard = false;
        let (mut saw_true, mut saw_false) = (false, false);
        for arm in arms {
            if wildcard {
                return err("this arm can never run: an earlier `_` arm already matches everything", arm.pos);
            }
            cx.scopes.push(HashMap::new());
            let pat = self.pattern(cx, arm, &ty, &enum_info, &mut covered, &mut wildcard, &mut saw_true, &mut saw_false);
            let pat = match pat {
                Ok(p) => p,
                Err(e) => {
                    cx.scopes.pop();
                    return Err(e);
                }
            };
            let body = self.block(cx, &arm.body, depth + 1);
            cx.scopes.pop();
            writeln!(out, "{}{} => {}", indent(depth + 1), pat, body?).unwrap();
        }
        let all_variants = enum_info.as_ref().is_some_and(|(_, info)| info.variants.iter().all(|(v, _)| covered.contains(v)));
        if !(wildcard || (saw_true && saw_false) || all_variants) {
            let missing = match &enum_info {
                Some((_, info)) => {
                    let m: Vec<&str> = info.variants.iter().filter(|(v, _)| !covered.contains(v)).map(|(v, _)| v.as_str()).collect();
                    format!("missing: {}", m.join(", "))
                }
                None => "add a `_ =>` arm".to_string(),
            };
            return err(format!("`match` does not handle every case ({})", missing), pos);
        }
        write!(out, "{}}}", indent(depth)).unwrap();
        Ok(out)
    }

    /// Rust pattern for one match arm; declares its bindings in the current scope.
    #[allow(clippy::too_many_arguments)]
    fn pattern(
        &self,
        cx: &mut Ctx,
        arm: &MatchArm,
        ty: &Type,
        enum_info: &Option<(String, crate::checker::EnumInfo)>,
        covered: &mut Vec<String>,
        wildcard: &mut bool,
        saw_true: &mut bool,
        saw_false: &mut bool,
    ) -> Result<String> {
        Ok(match (&arm.pattern, enum_info) {
            (Pattern::Wildcard, _) => {
                *wildcard = true;
                "_".to_string()
            }
            (Pattern::Variant(v, binds), Some((ename, info))) => {
                let fields = match info.variant(v) {
                    Some(f) => f.clone(),
                    None => return err(format!("enum `{}` has no variant `{}`", ename, v), arm.pos),
                };
                if covered.contains(v) {
                    return err(format!("variant `{}` is already handled by an earlier arm", v), arm.pos);
                }
                covered.push(v.clone());
                if binds.len() != fields.len() {
                    return err(format!("`{}` has {} value(s), but the pattern binds {}", v, fields.len(), binds.len()), arm.pos);
                }
                let mut rust = vec![];
                for (b, (_, t)) in binds.iter().zip(&fields) {
                    if b == "_" {
                        rust.push("_".to_string());
                    } else {
                        rust.push(cx.declare(b, t.clone(), false, arm.pos)?);
                    }
                }
                if fields.is_empty() { format!("{}::{}", ename, v) } else { format!("{}::{}({})", ename, v, rust.join(", ")) }
            }
            (Pattern::Int(n), None) if *ty == Type::Int => format!("{}i64", n),
            (Pattern::Str(st), None) if *ty == Type::Str => format!("{:?}", st),
            (Pattern::Bool(b), None) if *ty == Type::Bool => {
                if *b {
                    *saw_true = true
                } else {
                    *saw_false = true
                }
                b.to_string()
            }
            (Pattern::Variant(v, _), None) => return err(format!("`{}` is not a variant of `{}`", v, ty), arm.pos),
            _ => return err(format!("this pattern does not fit a `{}` value", ty), arm.pos),
        })
    }

    fn super_init(&self, cx: &mut Ctx, args: &[Expr], pos: Pos) -> Result<String> {
        if !cx.in_init {
            return err("`super(...)` can only be called inside `init`", pos);
        }
        let class = cx.class.clone().unwrap();
        let parent = match &self.m.classes[&class].parent {
            Some(p) => p.clone(),
            None => return err(format!("`{}` has no base class", class), pos),
        };
        let (owner, sig) = match self.m.find_init(&parent) {
            Some(x) => x,
            None => return err(format!("base class `{}` has no `init`", parent), pos),
        };
        let (binds, names) = self.args(cx, &sig, args, pos, "super")?;
        Ok(format!("{{ {}self.__{}__init({}); }}", binds, owner, names))
    }

    fn assign(&self, cx: &mut Ctx, target: &Expr, op: AssignOp, value: &Expr) -> Result<String> {
        // Field of a base-class reference: go through the generated setter.
        if let ExprKind::Field(recv, f) = &target.kind {
            if !matches!(recv.kind, ExprKind::SelfRef) {
                let (rcode, rt) = self.recv(cx, recv)?;
                if let Type::Class(c) = &rt {
                    if self.m.has_subclasses(c) {
                        let ft = self.field_type(cx, c, f, target.pos)?;
                        let v = self.assign_value(cx, op, &ft, value)?;
                        let new = match op {
                            AssignOp::Set => "__v".to_string(),
                            _ => self.compound("__r.borrow().__get_".to_string() + f + "()", op, &ft),
                        };
                        return Ok(format!(
                            "{{ let __v = {}; let __r = &{}; let __n = {}; __r.borrow_mut().__set_{}(__n); }}",
                            v, rcode, new, f
                        ));
                    }
                }
            }
        }
        // `m[k] = v` inserts (the key may be new).
        if let (ExprKind::Index(base, key), AssignOp::Set) = (&target.kind, op) {
            let mut n = 0;
            let (pre, bplace, bt) = self.place(cx, base, &mut n)?;
            if let Type::Map(k, vt) = bt {
                let v = self.expr_to(cx, value, &vt)?;
                let kc = self.expr_to(cx, key, &k)?;
                return Ok(format!("{{ let __v = {}; let __k = {}; {}{}.insert(__k, __v); }}", v, kc, pre, bplace));
            }
        }
        let wrapped = self.wrapped_target(cx, target)?;
        if op != AssignOp::Set {
            let mut n = 0;
            let (pre, place, ty) = self.place(cx, target, &mut n)?;
            if let Type::Struct(st) = &ty {
                let bin = match op {
                    AssignOp::Add => BinOp::Add,
                    AssignOp::Sub => BinOp::Sub,
                    AssignOp::Mul => BinOp::Mul,
                    AssignOp::Div => BinOp::Div,
                    _ => BinOp::Rem,
                };
                let (vc, vt) = self.expr(cx, value, None)?;
                let Some(o) = self.find_op(st, bin, Some(&vt)) else {
                    return err(format!("`{}` has no `op {}` taking a `{}`", st, bin.rust(), vt), value.pos);
                };
                if o.ret != ty {
                    return err(format!("`{}=` needs `op {}` to return `{}`, but it returns `{}`", bin.rust(), bin.rust(), st, o.ret), value.pos);
                }
                let vc = self.coerce(vc, &vt, o.param.as_ref().unwrap(), value.pos)?;
                return Ok(format!("{{ let __v = {}; {}let __n = {}.__op_{}(__v); {} = __n; }}", vc, pre, place, o.index, place));
            }
        }
        let v = {
            let mut n = 0;
            let (pre, place, ty) = self.place(cx, target, &mut n)?;
            let v = self.assign_value(cx, op, &ty, value)?;
            if wrapped {
                let new = match op {
                    AssignOp::Set => "__v".to_string(),
                    _ => self.compound(format!("__unset({}.clone(), \"field\")", place), op, &ty),
                };
                return Ok(format!("{{ let __v = {}; {}let __n = {}; {} = Some(__n); }}", v, pre, new, place));
            }
            let stmt = match (op, &ty) {
                (AssignOp::Set, _) => format!("{} = __v;", place),
                (AssignOp::Add, Type::Str) => format!("{}.push_str(&__v);", place),
                _ => format!("{} {}= __v;", place, op_sym(op)),
            };
            format!("{{ let __v = {}; {}{} }}", v, pre, stmt)
        };
        Ok(v)
    }

    /// Is `target` a field stored as `Option<T>` (a bare type parameter in a generic class)?
    fn wrapped_target(&self, cx: &mut Ctx, target: &Expr) -> Result<bool> {
        if let ExprKind::Field(recv, f) = &target.kind {
            let class = if matches!(recv.kind, ExprKind::SelfRef) {
                cx.class.clone()
            } else {
                match self.recv(cx, recv)?.1 {
                    Type::Generic(c, _) => Some(c),
                    _ => None,
                }
            };
            return Ok(class.is_some_and(|c| self.m.is_wrapped_field(&c, f)));
        }
        Ok(false)
    }

    /// Type of `self` inside a class (generic classes use their own parameters).
    fn class_type(&self, class: &str) -> Type {
        let tps = &self.m.classes[class].tparams;
        if tps.is_empty() {
            Type::Class(class.to_string())
        } else {
            Type::Generic(class.to_string(), tps.iter().map(|t| Type::Param(t.clone())).collect())
        }
    }

    /// Evaluates the arguments of a call to something generic, inferring its type
    /// parameters. Returns (let-bindings, argument names, bound types, return type).
    #[allow(clippy::too_many_arguments)]
    fn infer_call(
        &self,
        cx: &mut Ctx,
        tparams: &[String],
        targs: &[TypeExpr],
        sig: &Signature,
        args: &[Expr],
        expected: Option<&Type>,
        pos: Pos,
        what: &str,
    ) -> Result<(String, String, Vec<Type>, Type)> {
        if args.len() != sig.params.len() {
            return err(format!("`{}` takes {} argument(s) but {} were given", what, sig.params.len(), args.len()), pos);
        }
        let mut map: HashMap<String, Type> = HashMap::new();
        if !targs.is_empty() {
            if targs.len() != tparams.len() {
                return err(format!("`{}` takes {} type argument(s), not {}", what, tparams.len(), targs.len()), pos);
            }
            for (tp, te) in tparams.iter().zip(targs) {
                let t = self.m.resolve(te, &cx.tparams).map_err(|e| crate::checker::at_pos(e, pos))?;
                map.insert(tp.clone(), t);
            }
        } else if let Some(exp) = expected {
            unify(&sig.ret, exp, &mut map);
        }
        let mut evaluated: Vec<Option<(String, Type)>> = vec![None; args.len()];
        // Plain arguments first, so lambdas can then get full parameter types.
        for pass in 0..2 {
            for (i, a) in args.iter().enumerate() {
                let is_lambda = matches!(a.kind, ExprKind::Lambda { .. });
                if (pass == 0) == is_lambda {
                    continue;
                }
                let pt = subst(&sig.params[i].1, &map);
                let (code, t) = if !has_params(&pt) {
                    (self.expr_to(cx, a, &pt)?, pt.clone())
                } else if is_lambda {
                    self.expr(cx, a, Some(&unknown_params(&pt)))?
                } else {
                    self.expr(cx, a, None)?
                };
                unify(&sig.params[i].1, &t, &mut map);
                evaluated[i] = Some((code, t));
            }
        }
        for tp in tparams {
            if !map.contains_key(tp) {
                return err(format!("cannot tell what `{}` is for `{}`; write it, e.g. `{}<int>(...)`", tp, what, what), pos);
            }
        }
        let mut binds = String::new();
        let mut names = vec![];
        for (i, ev) in evaluated.into_iter().enumerate() {
            let (code, t) = ev.unwrap();
            let want = subst(&sig.params[i].1, &map);
            let code = self.coerce(code, &t, &want, args[i].pos)?;
            write!(binds, "let __a{} = {}; ", i, code).unwrap();
            names.push(format!("__a{}", i));
        }
        let bound: Vec<Type> = tparams.iter().map(|t| map[t].clone()).collect();
        Ok((binds, names.join(", "), bound, subst(&sig.ret, &map)))
    }

    fn turbofish(&self, types: &[Type]) -> String {
        format!("::<{}>", types.iter().map(|t| self.rty(t)).collect::<Vec<_>>().join(", "))
    }

    fn compound(&self, current: String, op: AssignOp, ty: &Type) -> String {
        if op == AssignOp::Add && *ty == Type::Str {
            format!("format!(\"{{}}{{}}\", {}, __v)", current)
        } else {
            format!("({} {} __v)", current, op_sym(op))
        }
    }

    fn assign_value(&self, cx: &mut Ctx, op: AssignOp, ty: &Type, value: &Expr) -> Result<String> {
        if op == AssignOp::Set {
            return self.expr_to(cx, value, ty);
        }
        let ok = match op {
            AssignOp::Add => matches!(ty, Type::Int | Type::Float | Type::Str),
            _ => matches!(ty, Type::Int | Type::Float),
        };
        if !ok {
            return err(format!("`{}=` is not supported for `{}`", op_sym(op), ty), value.pos);
        }
        self.expr_to(cx, value, ty)
    }

    /// An assignable location: (setup statements, Rust place expression, type).
    fn place(&self, cx: &mut Ctx, e: &Expr, n: &mut usize) -> Result<(String, String, Type)> {
        match &e.kind {
            ExprKind::Ident(name) => match cx.lookup(name) {
                Some(v) if v.mutable => Ok((String::new(), v.rust.clone(), v.ty.clone())),
                Some(v) if v.captured => err(
                    format!("cannot change `{}` inside a lambda: lambdas get a read-only copy of outside variables", name),
                    e.pos,
                ),
                Some(_) => err(format!("cannot change `{}`: it was declared with `let` (use `var` for a mutable variable)", name), e.pos),
                None => err(format!("unknown variable `{}`", name), e.pos),
            },
            ExprKind::Field(recv, f) => {
                if self.type_receiver(cx, recv).is_some() {
                    return err("cannot assign to an enum variant", e.pos);
                }
                if matches!(recv.kind, ExprKind::SelfRef) {
                    if let Some(st) = cx.strukt.clone() {
                        if !cx.self_mut {
                            return err("this method changes `self`; declare it `mut fn`", e.pos);
                        }
                        let t = self.struct_field_type(cx, &st, f, e.pos)?;
                        return Ok((String::new(), format!("self.{}", rid(f)), t));
                    }
                    let class = self.self_class(cx, recv.pos)?;
                    let t = self.field_type(cx, &class, f, e.pos)?;
                    return Ok((String::new(), format!("self.{}", rid(f)), t));
                }
                // A field of a struct value: change it where the struct lives.
                if let Type::Struct(st) = self.recv(cx, recv)?.1 {
                    let t = self.struct_field_type(cx, &st, f, e.pos)?;
                    let (pre, bplace, _) = self.place(cx, recv, n)?;
                    return Ok((pre, format!("{}.{}", bplace, rid(f)), t));
                }
                let (rcode, rt) = self.recv(cx, recv)?;
                match &rt {
                    Type::Class(c) if !self.m.has_subclasses(c) => {
                        let t = self.field_type(cx, c, f, e.pos)?;
                        Ok((String::new(), format!("{}.borrow_mut().{}", rcode, rid(f)), t))
                    }
                    Type::Generic(c, targs) => {
                        let t = subst(&self.field_type(cx, c, f, e.pos)?, &self.m.bindings(c, targs));
                        Ok((String::new(), format!("{}.borrow_mut().{}", rcode, rid(f)), t))
                    }
                    Type::Class(_) => err("cannot modify part of a field through a base-class reference; add a method to the class instead", e.pos),
                    other => err(format!("`{}` has no fields", other), e.pos),
                }
            }
            ExprKind::Index(base, idx) => {
                let (pre, bplace, bt) = self.place(cx, base, n)?;
                if let Type::Map(k, v) = bt {
                    let kc = self.expr_to(cx, idx, &k)?;
                    let tmp = format!("__k{}", n);
                    *n += 1;
                    return Ok((format!("{}let {} = {}; ", pre, tmp, kc), format!("(*__map_slot(&mut {}, {}))", bplace, tmp), *v));
                }
                let elem = match bt {
                    Type::Array(i) => *i,
                    other => return err(format!("cannot index into `{}`", other), e.pos),
                };
                let ic = self.expr_to(cx, idx, &Type::Int)?;
                let tmp = format!("__i{}", n);
                *n += 1;
                Ok((format!("{}let {} = __idx({}); ", pre, tmp, ic), format!("{}[{}]", bplace, tmp), elem))
            }
            ExprKind::SelfRef => err("cannot assign to `self`", e.pos),
            _ => err("cannot assign to this expression", e.pos),
        }
    }

    // ------------------------------------------------------------ expressions

    fn expr_to(&self, cx: &mut Ctx, e: &Expr, to: &Type) -> Result<String> {
        let (code, t) = self.expr(cx, e, Some(to))?;
        self.coerce(code, &t, to, e.pos)
    }

    fn self_class(&self, cx: &Ctx, pos: Pos) -> Result<String> {
        if cx.is_static {
            return err("`self` is not available in a `static fn`", pos);
        }
        if !cx.lambdas.is_empty() && cx.class.is_some() {
            return err("`self` cannot be used inside a lambda; copy it first (`let me = self`) and use `me`", pos);
        }
        cx.class.clone().ok_or_else(|| crate::error::CompileError::new("`self` is only available inside class methods", pos))
    }

    fn field_type(&self, cx: &Ctx, class: &str, field: &str, pos: Pos) -> Result<Type> {
        match self.m.all_fields(class).into_iter().find(|(n, _, _)| n == field) {
            Some((_, t, owner)) => {
                if self.m.classes[&owner].private_fields.contains(field) && cx.class.as_deref() != Some(owner.as_str()) {
                    return err(format!("field `{}` is private to `{}`", field, owner), pos);
                }
                Ok(t)
            }
            None => err(format!("`{}` has no field `{}`", class, field), pos),
        }
    }

    /// Type of field `name` when it holds a function.
    fn func_field(&self, class: &str, name: &str) -> Option<Type> {
        self.m.all_fields(class).into_iter().find(|(n, t, _)| n == name && matches!(t, Type::Func(..))).map(|(_, t, _)| t)
    }

    fn check_method_access(&self, cx: &Ctx, owner: &str, method: &str, pos: Pos) -> Result<()> {
        let info = &self.m.classes[owner];
        if info.private_methods.contains(method) && cx.class.as_deref() != Some(owner) {
            return err(format!("method `{}` is private to `{}`", method, owner), pos);
        }
        Ok(())
    }

    /// `Class.method(args)` for a `static fn`.
    fn static_call(&self, cx: &mut Ctx, class: &str, name: &str, args: &[Expr], pos: Pos) -> Result<(String, Type)> {
        let (owner, sig) = match self.m.find_static(class, name) {
            Some(x) => x,
            None if self.m.find_method(class, name).is_some() => {
                return err(format!("`{}` is not a `static fn`; call it on a `{}` object", name, class), pos)
            }
            None => return err(format!("`{}` has no static method `{}`", class, name), pos),
        };
        self.check_method_access(cx, &owner, name, pos)?;
        let (binds, names) = self.args(cx, &sig, args, pos, name)?;
        Ok((format!("{{ {}{}::__s_{}({}) }}", binds, owner, name, names), sig.ret))
    }

    /// Receiver for a method call or field access: (setup statements, Rust expression, type).
    /// Locals, `self.field` and indexing into those are borrowed in place, so calling a
    /// method on `shapes[i]` does not copy the object reference. Anything else is evaluated
    /// once into `__rv`.
    fn receiver(&self, cx: &mut Ctx, e: &Expr) -> Result<(String, String, Type)> {
        let mut n = 0;
        if let Some(x) = self.recv_place(cx, e, &mut n)? {
            return Ok(x);
        }
        let (code, t) = self.expr(cx, e, None)?;
        Ok((format!("let __rv = {}; ", code), "__rv".into(), t))
    }

    /// A place that can be borrowed without cloning, or `None`.
    fn recv_place(&self, cx: &mut Ctx, e: &Expr, n: &mut usize) -> Result<Option<(String, String, Type)>> {
        match &e.kind {
            ExprKind::Ident(name) => Ok(cx.lookup(name).map(|v| (String::new(), v.rust.clone(), v.ty.clone()))),
            ExprKind::Field(recv, f) if matches!(recv.kind, ExprKind::SelfRef) && cx.strukt.is_some() => {
                if cx.is_static {
                    return Ok(None);
                }
                let st = cx.strukt.clone().unwrap();
                let t = self.struct_field_type(cx, &st, f, e.pos)?;
                Ok(Some((String::new(), format!("self.{}", rid(f)), t)))
            }
            ExprKind::Field(recv, f) if !matches!(recv.kind, ExprKind::SelfRef) => {
                let Some((pre, bplace, bt)) = self.recv_place(cx, recv, n)? else { return Ok(None) };
                let Type::Struct(st) = bt else { return Ok(None) };
                let t = self.struct_field_type(cx, &st, f, e.pos)?;
                Ok(Some((pre, format!("{}.{}", bplace, rid(f)), t)))
            }
            ExprKind::Field(recv, f) if matches!(recv.kind, ExprKind::SelfRef) => {
                let class = self.self_class(cx, recv.pos)?;
                if self.m.is_wrapped_field(&class, f) {
                    return Ok(None);
                }
                let t = self.field_type(cx, &class, f, e.pos)?;
                Ok(Some((String::new(), format!("self.{}", rid(f)), t)))
            }
            ExprKind::Index(base, idx) => {
                let Some((pre, bplace, bt)) = self.recv_place(cx, base, n)? else { return Ok(None) };
                let Type::Array(elem) = bt else { return Ok(None) };
                let ic = self.expr_to(cx, idx, &Type::Int)?;
                let tmp = format!("__ri{}", n);
                *n += 1;
                Ok(Some((format!("{}let {} = __idx({}); ", pre, tmp, ic), format!("{}[{}]", bplace, tmp), *elem)))
            }
            _ => Ok(None),
        }
    }

    /// Receiver for a method call or field access, borrowed when it is a plain variable.
    fn recv(&self, cx: &mut Ctx, e: &Expr) -> Result<(String, Type)> {
        if let ExprKind::Ident(name) = &e.kind {
            if let Some(v) = cx.lookup(name) {
                return Ok((v.rust.clone(), v.ty.clone()));
            }
        }
        let (code, t) = self.expr(cx, e, None)?;
        Ok((format!("({})", code), t))
    }

    /// Evaluates call arguments into `__aN` temporaries: (let-bindings, names).
    fn args(&self, cx: &mut Ctx, sig: &Signature, args: &[Expr], pos: Pos, what: &str) -> Result<(String, String)> {
        if args.len() != sig.params.len() {
            return err(
                format!("`{}` takes {} argument(s) but {} were given", what, sig.params.len(), args.len()),
                pos,
            );
        }
        let mut binds = String::new();
        let mut names = vec![];
        for (i, (a, (_, t))) in args.iter().zip(&sig.params).enumerate() {
            let code = self.expr_to(cx, a, t)?;
            write!(binds, "let __a{} = {}; ", i, code).unwrap();
            names.push(format!("__a{}", i));
        }
        Ok((binds, names.join(", ")))
    }

    fn expr(&self, cx: &mut Ctx, e: &Expr, expected: Option<&Type>) -> Result<(String, Type)> {
        match &e.kind {
            ExprKind::Int(v) => Ok((format!("{}i64", v), Type::Int)),
            ExprKind::Float(v) => Ok((format!("{:?}f64", v), Type::Float)),
            ExprKind::Str(s) => Ok((format!("String::from({:?})", s), Type::Str)),
            ExprKind::Bool(b) => Ok((b.to_string(), Type::Bool)),
            ExprKind::None => match expected {
                Some(t @ Type::Optional(_)) => Ok(("None".into(), t.clone())),
                _ => Ok(("None".into(), Type::NoneLit)),
            },
            ExprKind::SelfRef => {
                if let Some(st) = cx.strukt.clone() {
                    if cx.is_static {
                        return err("`self` is not available in a `static fn`", e.pos);
                    }
                    return Ok(("self.clone()".into(), Type::Struct(st)));
                }
                let class = self.self_class(cx, e.pos)?;
                if cx.in_init {
                    return err("`self` cannot be used as a value inside `init` (the object is not finished yet)", e.pos);
                }
                let ty = self.class_type(&class);
                self.self_values.borrow_mut().insert(class);
                Ok((format!("(self.__self.upgrade().unwrap() as {})", self.rty(&ty)), ty))
            }
            ExprKind::Downcast { value, class, test_only } => self.downcast(cx, value, class, *test_only, e.pos),
            ExprKind::Lambda { params, ret, body } => self.lambda(cx, params, ret.as_ref(), body, expected, e.pos),
            ExprKind::Ident(name) => match cx.lookup(name) {
                Some(v) => Ok((clone_if(v.rust.clone(), &v.ty), v.ty.clone())),
                None if self.m.functions.contains_key(name) => {
                    let sig = &self.m.functions[name];
                    let ty = Type::Func(sig.params.iter().map(|(_, t)| t.clone()).collect(), Box::new(sig.ret.clone()));
                    Ok((format!("(Rc::new({}) as {})", fn_id(name), self.rty(&ty)), ty))
                }
                None if self.m.classes.contains_key(name) => err(format!("`{}` is a class; create one with `{}(...)`", name, name), e.pos),
                None => err(format!("unknown variable `{}`", name), e.pos),
            },
            ExprKind::Array(items) => {
                let elem = match expected {
                    Some(Type::Array(t)) => Some((**t).clone()),
                    _ => None,
                };
                let elem = match (elem, items.first()) {
                    (Some(t), _) => t,
                    (None, Some(first)) => self.expr(cx, first, None)?.1,
                    (None, None) => return err("cannot infer the type of `[]`; add a type, e.g. `let xs: [int] = []`", e.pos),
                };
                let mut codes = vec![];
                for it in items {
                    codes.push(self.expr_to(cx, it, &elem).map_err(|mut er| {
                        er.msg = format!("{} (all array elements must have the same type; annotate the variable to mix subclasses)", er.msg);
                        er
                    })?);
                }
                Ok((format!("vec![{}]", codes.join(", ")), Type::Array(Box::new(elem))))
            }
            ExprKind::Map(entries) => {
                let (kt, vt) = match (expected, entries.first()) {
                    (Some(Type::Map(k, v)), _) => ((**k).clone(), (**v).clone()),
                    (_, Some((k, v))) => (self.expr(cx, k, None)?.1, self.expr(cx, v, None)?.1),
                    (_, None) => return err("cannot infer the type of `{}`; add a type, e.g. `let m: {str: int} = {}`", e.pos),
                };
                if !is_map_key(&kt) {
                    return err(format!("map keys must be `int`, `str` or `bool`, not `{}`", kt), e.pos);
                }
                let mut code = String::from("{ let mut __m = BTreeMap::new(); ");
                for (k, v) in entries {
                    let kc = self.expr_to(cx, k, &kt)?;
                    let vc = self.expr_to(cx, v, &vt).map_err(|mut er| {
                        er.msg = format!("{} (all map values must have the same type; annotate the variable to mix subclasses)", er.msg);
                        er
                    })?;
                    write!(code, "__m.insert({}, {}); ", kc, vc).unwrap();
                }
                code.push_str("__m }");
                Ok((code, Type::Map(Box::new(kt), Box::new(vt))))
            }
            ExprKind::Interp(parts) => {
                let mut fmt = String::new();
                let mut args = vec![];
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => fmt.push_str(&s.replace('{', "{{").replace('}', "}}")),
                        InterpPart::Expr(x) => {
                            let (c, t) = self.expr(cx, x, None)?;
                            if !self.m.is_plain(&t) {
                                return err(format!("cannot put a `{}` value into a string", t), x.pos);
                            }
                            fmt.push_str("{}");
                            args.push(format!("__KlShow::__show(&{})", c));
                        }
                    }
                }
                Ok((format!("format!({:?}, {})", fmt, args.join(", ")), Type::Str))
            }
            ExprKind::Unary(op, inner) => {
                let (code, t) = self.expr(cx, inner, None)?;
                match (op, &t) {
                    (UnOp::Neg, Type::Int | Type::Float) => Ok((format!("(-{})", code), t)),
                    (UnOp::Neg, Type::Struct(st)) => match self.find_op(st, BinOp::Sub, None) {
                        Some(o) => Ok((format!("{{ let __l = {}; __l.__op_{}() }}", code, o.index), o.ret)),
                        None => err(format!("`{}` has no `op -()` for negation", st), e.pos),
                    },
                    (UnOp::Not, Type::Bool) => Ok((format!("(!{})", code), t)),
                    (UnOp::Neg, _) => err(format!("cannot negate `{}`", t), e.pos),
                    (UnOp::Not, _) => err(format!("`!` needs a `bool`, found `{}`", t), e.pos),
                }
            }
            ExprKind::Binary(op, l, r) => self.binary(cx, *op, l, r, e.pos),
            ExprKind::Call(name, targs, args) => self.call(cx, name, targs, args, expected, e.pos),
            ExprKind::MethodCall(recv, name, args) => self.method_call(cx, recv, name, args, e.pos),
            ExprKind::Field(recv, f) => {
                if let Some(t) = self.type_receiver(cx, recv) {
                    return self.variant_value(cx, &t, f, None, e.pos);
                }
                if let Some(ns) = self.library_receiver(cx, recv) {
                    return match (ns.as_str(), f.as_str()) {
                        ("Math", "PI") => Ok(("std::f64::consts::PI".into(), Type::Float)),
                        ("Math", "TAU") => Ok(("std::f64::consts::TAU".into(), Type::Float)),
                        ("Math", "E") => Ok(("std::f64::consts::E".into(), Type::Float)),
                        ("Math", "INF") => Ok(("f64::INFINITY".into(), Type::Float)),
                        _ => err(format!("`{}.{}` is not a value; library functions are called with `()`", ns, f), e.pos),
                    };
                }
                if matches!(recv.kind, ExprKind::SelfRef) {
                    if let Some(st) = cx.strukt.clone() {
                        if cx.is_static {
                            return err("`self` is not available in a `static fn`", e.pos);
                        }
                        let t = self.struct_field_type(cx, &st, f, e.pos)?;
                        return Ok((clone_if(format!("self.{}", rid(f)), &t), t));
                    }
                    let class = self.self_class(cx, recv.pos)?;
                    let t = self.field_type(cx, &class, f, e.pos)?;
                    if self.m.is_wrapped_field(&class, f) {
                        return Ok((format!("__unset(self.{}.clone(), {:?})", rid(f), f), t));
                    }
                    return Ok((clone_if(format!("self.{}", rid(f)), &t), t));
                }
                let (setup, rcode, rt) = self.receiver(cx, recv)?;
                match &rt {
                    Type::Struct(st) => {
                        let t = self.struct_field_type(cx, st, f, e.pos)?;
                        let read = clone_if(format!("{}.{}", rcode, rid(f)), &t);
                        Ok((if setup.is_empty() { read } else { format!("{{ {}let __t = {}; __t }}", setup, read) }, t))
                    }
                    Type::Generic(c, targs) => {
                        let t = subst(&self.field_type(cx, c, f, e.pos)?, &self.m.bindings(c, targs));
                        let read = if self.m.is_wrapped_field(c, f) {
                            format!("__unset({}.borrow().{}.clone(), {:?})", rcode, rid(f), f)
                        } else {
                            clone_if(format!("{}.borrow().{}", rcode, rid(f)), &t)
                        };
                        Ok((format!("{{ {}let __t = {}; __t }}", setup, read), t))
                    }
                    Type::Class(c) => {
                        let t = self.field_type(cx, c, f, e.pos)?;
                        let read = if self.m.has_subclasses(c) {
                            format!("{}.borrow().__get_{}()", rcode, f)
                        } else {
                            clone_if(format!("{}.borrow().{}", rcode, rid(f)), &t)
                        };
                        Ok((format!("{{ {}let __t = {}; __t }}", setup, read), t))
                    }
                    Type::Array(_) | Type::Str | Type::Map(..) if f == "len" => err("`len` is a method: use `.len()`", e.pos),
                    Type::Interface(i) => err(format!("interface `{}` has no fields; only methods can be used", i), e.pos),
                    other => err(format!("`{}` has no field `{}`", other, f), e.pos),
                }
            }
            ExprKind::Index(base, idx) => {
                let (bcode, bt) = self.recv(cx, base)?;
                if let Type::Map(k, v) = bt {
                    let kc = self.expr_to(cx, idx, &k)?;
                    return Ok((format!("{{ let __k = {}; __map_get(&{}, &__k) }}", kc, bcode), *v));
                }
                let elem = match bt {
                    Type::Array(i) => *i,
                    other => return err(format!("cannot index into `{}`", other), e.pos),
                };
                let ic = self.expr_to(cx, idx, &Type::Int)?;
                let read = clone_if(format!("{}[__i]", bcode), &elem);
                Ok((format!("{{ let __i = __idx({}); {} }}", ic, read), elem))
            }
            ExprKind::SuperInit(_) => err("`super(...)` must be its own statement inside `init`", e.pos),
            ExprKind::SuperCall(name, args) => {
                let class = self.self_class(cx, e.pos)?;
                let (owner, sig) = match self.m.find_super_method(&class, name) {
                    Some(x) => x,
                    None => return err(format!("no base class of `{}` has a method `{}`", class, name), e.pos),
                };
                self.check_method_access(cx, &owner, name, e.pos)?;
                if self.m.classes[&owner].abstract_methods.contains(name) {
                    return err(format!("`{}.{}` is abstract and has no body to call", owner, name), e.pos);
                }
                let (binds, names) = self.args(cx, &sig, args, e.pos, name)?;
                Ok((format!("{{ {}self.__{}__{}({}) }}", binds, owner, name, names), sig.ret))
            }
        }
    }

    /// Calls a function value held in `code`.
    #[allow(clippy::too_many_arguments)]
    fn call_value(&self, cx: &mut Ctx, code: String, ps: &[Type], ret: &Type, args: &[Expr], pos: Pos, what: &str) -> Result<(String, Type)> {
        let sig = Signature { params: ps.iter().enumerate().map(|(i, t)| (format!("p{}", i), t.clone())).collect(), ret: ret.clone() };
        let (binds, names) = self.args(cx, &sig, args, pos, what)?;
        Ok((format!("{{ let __f = {}; {}__f({}) }}", code, binds, names), ret.clone()))
    }

    fn lambda(
        &self,
        cx: &mut Ctx,
        params: &[(String, Option<TypeExpr>, Pos)],
        ret: Option<&TypeExpr>,
        body: &LambdaBody,
        expected: Option<&Type>,
        pos: Pos,
    ) -> Result<(String, Type)> {
        let hint = match expected {
            Some(Type::Func(ps, r)) if ps.len() == params.len() => Some((ps.clone(), (**r).clone())),
            _ => None,
        };
        let mut ptypes = Vec::new();
        for (i, (name, ty, ppos)) in params.iter().enumerate() {
            let t = match (ty, &hint) {
                (Some(te), _) => self.m.resolve(te, &cx.tparams).map_err(|e| crate::checker::at_pos(e, *ppos))?,
                (None, Some((ps, _))) if ps[i] != Type::Unknown => ps[i].clone(),
                _ => return err(format!("cannot tell the type of `{}`; write it, e.g. `fn({}: int) => ...`", name, name), *ppos),
            };
            ptypes.push(t);
        }
        let declared_ret = match ret {
            Some(te) => Some(self.m.resolve(te, &cx.tparams).map_err(|e| crate::checker::at_pos(e, pos))?),
            None => hint.as_ref().map(|(_, r)| r.clone()).filter(|r| *r != Type::Unknown),
        };

        // The lambda body gets its own scope; outer variables become read-only captures.
        let boundary = cx.scopes.len();
        cx.lambdas.push(LambdaFrame { boundary, captures: RefCell::new(vec![]) });
        cx.scopes.push(HashMap::new());
        let saved = (cx.ret.clone(), cx.loops, cx.in_init);
        cx.loops = 0;
        cx.in_init = false;
        let result = (|| -> Result<(Vec<String>, String, Type)> {
            let mut rust_params = vec![];
            for ((name, _, ppos), t) in params.iter().zip(&ptypes) {
                let r = cx.declare(name, t.clone(), false, *ppos)?;
                rust_params.push(format!("{}: {}", r, self.rty(t)));
            }
            match body {
                LambdaBody::Expr(e) => {
                    let (code, t) = match &declared_ret {
                        Some(r) => (self.expr_to(cx, e, r)?, r.clone()),
                        None => self.expr(cx, e, None)?,
                    };
                    if t == Type::NoneLit {
                        return err("cannot tell the result type; write it, e.g. `fn(x: int) -> int? => none`", e.pos);
                    }
                    let code = if t == Type::Void { format!("{{ {}; }}", code) } else { code };
                    Ok((rust_params, code, t))
                }
                LambdaBody::Block(stmts) => {
                    let r = declared_ret.clone().unwrap_or(Type::Void);
                    cx.ret = r.clone();
                    if r != Type::Void && !always_returns(stmts) {
                        return err(format!("this lambda must return a `{}` on every path", r), pos);
                    }
                    let code = self.block_inner(cx, stmts, 2)?;
                    Ok((rust_params, format!("{{\n{}}}", code), r))
                }
            }
        })();
        (cx.ret, cx.loops, cx.in_init) = saved;
        cx.scopes.pop();
        let frame = cx.lambdas.pop().unwrap();
        let (rust_params, code, rt) = result?;

        let ty = Type::Func(ptypes, Box::new(rt.clone()));
        let caps: String = frame.captures.borrow().iter().map(|c| format!("let {} = {}.clone(); ", c, c)).collect();
        let ret_sig = if rt == Type::Void { String::new() } else { format!(" -> {}", self.rty(&rt)) };
        let body_code = if code.starts_with('{') { code } else { format!("{{ {} }}", code) };
        Ok((
            format!(
                "{{ {}let __f: {} = Rc::new(move |{}|{} {}); __f }}",
                caps,
                self.rty(&ty),
                rust_params.join(", "),
                ret_sig,
                body_code
            ),
            ty,
        ))
    }

    fn downcast(&self, cx: &mut Ctx, value: &Expr, target: &str, test_only: bool, pos: Pos) -> Result<(String, Type)> {
        if !self.m.classes.contains_key(target) {
            return err(format!("`as`/`is` need a class name, and `{}` is not a class", target), pos);
        }
        if !self.m.classes[target].tparams.is_empty() {
            return err(format!("cannot use `as`/`is` with generic class `{}`", target), pos);
        }
        let (code, ty) = self.expr(cx, value, None)?;
        let target_ty = Type::Class(target.to_string());
        let call = match &ty {
            Type::Class(c) if c == target => format!("Some({})", code),
            Type::Class(c) if self.m.is_subclass(target, c) => {
                self.cast_targets.borrow_mut().insert(target.to_string());
                let root = self.m.ancestors(c).last().unwrap().clone();
                format!("{{ let __x = {}; let __t = {}::__as_{}(&*__x.borrow()); __t }}", code, class_trait(&root), target)
            }
            Type::Interface(i) if self.m.all_interfaces(target).contains(i) => {
                self.cast_targets.borrow_mut().insert(target.to_string());
                format!("{{ let __x = {}; let __t = {}::__as_{}(&*__x.borrow()); __t }}", code, i, target)
            }
            Type::Class(_) | Type::Interface(_) => {
                return err(format!("a `{}` can never be a `{}`, so this check would always fail", ty, target), pos)
            }
            other => return err(format!("`as`/`is` work on objects, not on `{}`", other), pos),
        };
        if test_only {
            Ok((format!("({}).is_some()", call), Type::Bool))
        } else {
            Ok((call, Type::Optional(Box::new(target_ty))))
        }
    }

    fn binary(&self, cx: &mut Ctx, op: BinOp, l: &Expr, r: &Expr, pos: Pos) -> Result<(String, Type)> {
        let (lc, lt) = self.expr(cx, l, None)?;
        let (rc, rt) = self.expr(cx, r, None)?;
        if let Type::Struct(st) = &lt {
            if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem) {
                let Some(o) = self.find_op(st, op, Some(&rt)) else {
                    return err(format!("`{}` has no `op {}` taking a `{}`", st, op.rust(), rt), pos);
                };
                let rc = self.coerce(rc, &rt, o.param.as_ref().unwrap(), r.pos)?;
                return Ok((format!("{{ let __l = {}; let __r = {}; __l.__op_{}(__r) }}", lc, rc, o.index), o.ret));
            }
        }
        use BinOp::*;
        match op {
            And | Or => {
                if lt != Type::Bool || rt != Type::Bool {
                    return err(format!("`{}` needs `bool` on both sides, found `{}` and `{}`", op.rust(), lt, rt), pos);
                }
                Ok((format!("({} {} {})", lc, op.rust(), rc), Type::Bool))
            }
            Eq | Ne => {
                let neg = if op == Eq { "" } else { "!" };
                match (&lt, &rt) {
                    (Type::Optional(_), Type::NoneLit) => return Ok((format!("{}({}).is_none()", neg, lc), Type::Bool)),
                    (Type::NoneLit, Type::Optional(_)) => return Ok((format!("{}({}).is_none()", neg, rc), Type::Bool)),
                    _ => {}
                }
                if lt != rt {
                    return err(format!("cannot compare `{}` with `{}`", lt, rt), pos);
                }
                if !self.m.is_plain(&lt) {
                    return err(format!("`{}` values cannot be compared with `{}`", lt, op.rust()), pos);
                }
                Ok((format!("({} {} {})", lc, op.rust(), rc), Type::Bool))
            }
            Lt | Le | Gt | Ge => {
                if lt != rt || !matches!(lt, Type::Int | Type::Float | Type::Str) {
                    return err(format!("cannot compare `{}` with `{}` using `{}`", lt, rt, op.rust()), pos);
                }
                Ok((format!("({} {} {})", lc, op.rust(), rc), Type::Bool))
            }
            Add if lt == Type::Str && rt == Type::Str => Ok((format!("format!(\"{{}}{{}}\", {}, {})", lc, rc), Type::Str)),
            Add | Sub | Mul | Div | Rem => {
                if lt != rt || !matches!(lt, Type::Int | Type::Float) {
                    let hint = if lt == Type::Str || rt == Type::Str { " (convert with `str(x)` first)" } else { "" };
                    return err(format!("cannot apply `{}` to `{}` and `{}`{}", op.rust(), lt, rt, hint), pos);
                }
                Ok((format!("({} {} {})", lc, op.rust(), rc), lt))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call(&self, cx: &mut Ctx, name: &str, targs: &[TypeExpr], args: &[Expr], expected: Option<&Type>, pos: Pos) -> Result<(String, Type)> {
        let generic_class = self.m.classes.get(name).is_some_and(|c| !c.tparams.is_empty());
        let generic_fn = self.m.fn_tparams.get(name).is_some_and(|t| !t.is_empty());
        if !targs.is_empty() && !generic_class && !generic_fn {
            return err(format!("`{}` is not generic and takes no type arguments", name), pos);
        }
        if generic_class && cx.lookup(name).is_none() {
            let tparams = self.m.classes[name].tparams.clone();
            let sig = self.m.find_init(name).map(|(_, s)| s).unwrap_or(Signature { params: vec![], ret: Type::Void });
            let sig = Signature { params: sig.params, ret: self.class_type(name) };
            let (binds, names, bound, ret) = self.infer_call(cx, &tparams, targs, &sig, args, expected, pos, name)?;
            return Ok((format!("{{ {}{}{}::__new({}) }}", binds, name, self.turbofish(&bound), names), ret));
        }
        if generic_fn && cx.lookup(name).is_none() {
            let tparams = self.m.fn_tparams[name].clone();
            let sig = self.m.functions[name].clone();
            let (binds, names, bound, ret) = self.infer_call(cx, &tparams, targs, &sig, args, expected, pos, name)?;
            return Ok((format!("{{ {}{}{}({}) }}", binds, fn_id(name), self.turbofish(&bound), names), ret));
        }
        if let Some(v) = cx.lookup(name) {
            return match v.ty.clone() {
                Type::Func(ps, r) => self.call_value(cx, format!("&{}", v.rust), &ps, &r, args, pos, name),
                other => err(format!("`{}` is a `{}`, not a function", name, other), pos),
            };
        }
        if BUILTINS.contains(&name) {
            return self.builtin(cx, name, args, pos);
        }
        if let Some(info) = self.m.structs.get(name) {
            let (min, max) = (info.required_args(), info.fields.len());
            if args.len() < min || args.len() > max {
                let range = if min == max { format!("{}", max) } else { format!("{} to {}", min, max) };
                return err(format!("`{}(...)` takes {} argument(s) (its fields in order) but {} were given", name, range, args.len()), pos);
            }
            let mut binds = String::new();
            let mut inits = vec![];
            for (i, (f, t)) in info.fields.iter().enumerate() {
                let code = match args.get(i) {
                    Some(a) => self.expr_to(cx, a, t)?,
                    None => {
                        let mut dcx = Ctx::new(None, Type::Void, false);
                        self.expr_to(&mut dcx, info.decl.fields[i].default.as_ref().unwrap(), t)?
                    }
                };
                write!(binds, "let __a{} = {}; ", i, code).unwrap();
                inits.push(format!("{}: __a{}", rid(f), i));
            }
            return Ok((format!("{{ {}{} {{ {} }} }}", binds, name, inits.join(", ")), Type::Struct(name.to_string())));
        }
        if self.m.classes.contains_key(name) {
            if self.m.classes[name].decl.is_abstract {
                return err(format!("`{}` is abstract and cannot be created; create one of its subclasses", name), pos);
            }
            let sig = self.m.find_init(name).map(|(_, s)| s).unwrap_or(Signature { params: vec![], ret: Type::Void });
            let mut codes = vec![];
            if args.len() != sig.params.len() {
                return err(format!("`{}(...)` takes {} argument(s) but {} were given", name, sig.params.len(), args.len()), pos);
            }
            for (a, (_, t)) in args.iter().zip(&sig.params) {
                codes.push(self.expr_to(cx, a, t)?);
            }
            let mut code = format!("{}::__new({})", name, codes.join(", "));
            if self.m.has_subclasses(name) {
                code = format!("({} as {})", code, self.rty(&Type::Class(name.into())));
            }
            return Ok((code, Type::Class(name.into())));
        }
        if self.m.interfaces.contains_key(name) {
            return err(format!("`{}` is an interface and cannot be created directly", name), pos);
        }
        if let Some((sig, path)) = self.m.extern_fns.get(name) {
            let (binds, names) = self.args(cx, sig, args, pos, name)?;
            return Ok((format!("{{ {}{}({}) }}", binds, path, names), sig.ret.clone()));
        }
        let sig = match self.m.functions.get(name) {
            Some(s) => s.clone(),
            None => return err(format!("unknown function `{}`", name), pos),
        };
        let (binds, names) = self.args(cx, &sig, args, pos, name)?;
        Ok((format!("{{ {}{}({}) }}", binds, fn_id(name), names), sig.ret))
    }

    fn builtin(&self, cx: &mut Ctx, name: &str, args: &[Expr], pos: Pos) -> Result<(String, Type)> {
        let one = |cx: &mut Ctx| -> Result<(String, Type)> {
            if args.len() != 1 {
                return err(format!("`{}` takes exactly 1 argument", name), pos);
            }
            self.expr(cx, &args[0], None)
        };
        match name {
            "print" => {
                let mut parts = vec![];
                for a in args {
                    let (c, t) = self.expr(cx, a, None)?;
                    if !self.m.is_plain(&t) {
                        return err(format!("cannot print a `{}` value (print its fields or a method result instead)", t), a.pos);
                    }
                    parts.push(format!("__KlShow::__show(&{})", c));
                }
                if parts.is_empty() {
                    return Ok(("println!()".into(), Type::Void));
                }
                Ok((format!("println!(\"{{}}\", [{}].join(\" \"))", parts.join(", ")), Type::Void))
            }
            "len" => {
                if args.len() != 1 {
                    return err("`len` takes exactly 1 argument", pos);
                }
                let (c, t) = self.recv(cx, &args[0])?;
                match t {
                    Type::Array(_) | Type::Map(..) => Ok((format!("({}.len() as i64)", c), Type::Int)),
                    Type::Str => Ok((format!("({}.chars().count() as i64)", c), Type::Int)),
                    other => err(format!("`len` needs an array, map or string, found `{}`", other), pos),
                }
            }
            "str" => {
                let (c, t) = one(cx)?;
                if !self.m.is_plain(&t) {
                    return err(format!("cannot convert `{}` to str", t), pos);
                }
                Ok((format!("__KlShow::__show(&{})", c), Type::Str))
            }
            "int" => {
                let (c, t) = one(cx)?;
                Ok((
                    match t {
                        Type::Int => c,
                        Type::Float => format!("(({}) as i64)", c),
                        Type::Bool => format!("(({}) as i64)", c),
                        Type::Str => format!("__parse_int(&{})", c),
                        other => return err(format!("cannot convert `{}` to int", other), pos),
                    },
                    Type::Int,
                ))
            }
            "float" => {
                let (c, t) = one(cx)?;
                Ok((
                    match t {
                        Type::Float => c,
                        Type::Int => format!("(({}) as f64)", c),
                        Type::Str => format!("__parse_float(&{})", c),
                        other => return err(format!("cannot convert `{}` to float", other), pos),
                    },
                    Type::Float,
                ))
            }
            "input" => {
                let prompt = match args.len() {
                    0 => "String::new()".to_string(),
                    1 => self.expr_to(cx, &args[0], &Type::Str)?,
                    _ => return err("`input` takes an optional prompt string", pos),
                };
                Ok((format!("__input(&{})", prompt), Type::Str))
            }
            _ => unreachable!(),
        }
    }

    fn method_call(&self, cx: &mut Ctx, recv: &Expr, name: &str, args: &[Expr], pos: Pos) -> Result<(String, Type)> {
        if let Some(t) = self.type_receiver(cx, recv) {
            return self.variant_value(cx, &t, name, Some(args), pos);
        }
        if let Some(ns) = self.library_receiver(cx, recv) {
            return self.library_call(cx, &ns, name, args, pos);
        }
        if let ExprKind::Ident(n) = &recv.kind {
            if cx.lookup(n).is_none() && self.m.classes.contains_key(n) {
                return self.static_call(cx, n, name, args, pos);
            }
            if cx.lookup(n).is_none() && self.m.structs.contains_key(n) {
                let m = match self.m.structs[n].methods.get(name) {
                    Some(m) if m.is_static => m.clone(),
                    Some(_) => return err(format!("`{}` is not a `static fn`; call it on a `{}` value", name, n), pos),
                    None => return err(format!("`{}` has no static method `{}`", n, name), pos),
                };
                if m.is_private && cx.strukt.as_deref() != Some(n.as_str()) {
                    return err(format!("method `{}` is private to `{}`", name, n), pos);
                }
                let (binds, names) = self.args(cx, &m.sig, args, pos, name)?;
                return Ok((format!("{{ {}{}::__s_{}({}) }}", binds, n, name, names), m.sig.ret));
            }
        }
        if matches!(recv.kind, ExprKind::SelfRef) {
            if let Some(st) = cx.strukt.clone() {
                if cx.is_static {
                    return err("`self` is not available in a `static fn`", pos);
                }
                return self.struct_method_call(cx, &st, recv, name, args, pos);
            }
            let class = self.self_class(cx, recv.pos)?;
            let sig = match self.m.find_method(&class, name) {
                Some((owner, s)) => {
                    self.check_method_access(cx, &owner, name, pos)?;
                    s
                }
                None if self.m.find_static(&class, name).is_some() => {
                    return err(format!("`{}` is a `static fn`; call it as `{}.{}(...)`", name, class, name), pos)
                }
                None => {
                    if let Some(Type::Func(ps, r)) = self.func_field(&class, name) {
                        self.field_type(cx, &class, name, pos)?;
                        return self.call_value(cx, format!("self.{}.clone()", rid(name)), &ps, &r, args, pos, name);
                    }
                    return err(format!("`{}` has no method `{}`", class, name), pos);
                }
            };
            let (binds, names) = self.args(cx, &sig, args, pos, name)?;
            return Ok((format!("{{ {}self.{}({}) }}", binds, rid(name), names), sig.ret));
        }

        // Mutating array/map methods need a mutable place.
        let recv_ty = self.recv(cx, recv)?.1;
        if let Type::Struct(st) = &recv_ty {
            return self.struct_method_call(cx, st, recv, name, args, pos);
        }
        if let (Type::Map(k, v), "remove") = (&recv_ty, name) {
            let mut n = 0;
            let (pre, place, _) = self.place(cx, recv, &mut n)?;
            if args.len() != 1 {
                return err("`remove` takes exactly 1 argument (the key)", pos);
            }
            let kc = self.expr_to(cx, &args[0], k)?;
            return Ok((format!("{{ let __k = {}; {}{}.remove(&__k) }}", kc, pre, place), Type::Optional(v.clone())));
        }
        if let (Type::Array(elem), "sort" | "sort_by") = (&recv_ty, name) {
            let mut n = 0;
            let (pre, place, _) = self.place(cx, recv, &mut n)?;
            if name == "sort" {
                if !args.is_empty() {
                    return err("`sort` takes no arguments (use `sort_by(fn(a, b) => ...)` for a custom order)", pos);
                }
                let call = match **elem {
                    Type::Int | Type::Str | Type::Bool => "sort()",
                    Type::Float => "sort_by(|a, b| a.total_cmp(b))",
                    _ => return err(format!("`sort` works on arrays of int, float, str or bool, not `{}`; use `sort_by`", elem), pos),
                };
                return Ok((format!("{{ {}{}.{}; }}", pre, place, call), Type::Void));
            }
            if args.len() != 1 {
                return err("`sort_by` takes one function: `fn(a, b) => <a comes before b>`", pos);
            }
            let fty = Type::Func(vec![(**elem).clone(), (**elem).clone()], Box::new(Type::Bool));
            let f = self.expr_to(cx, &args[0], &fty)?;
            return Ok((
                format!(
                    "{{ let __f = {}; {}{}.sort_by(|a, b| if __f(a.clone(), b.clone()) {{ std::cmp::Ordering::Less }} else if __f(b.clone(), a.clone()) {{ std::cmp::Ordering::Greater }} else {{ std::cmp::Ordering::Equal }}); }}",
                    f, pre, place
                ),
                Type::Void,
            ));
        }
        if matches!(recv_ty, Type::Array(_)) && (name == "push" || name == "pop") {
            let mut n = 0;
            let (pre, place, t) = self.place(cx, recv, &mut n)?;
            let elem = match &t {
                Type::Array(i) => (**i).clone(),
                other => return err(format!("`{}` has no method `{}`", other, name), pos),
            };
            if name == "push" {
                if args.len() != 1 {
                    return err("`push` takes exactly 1 argument", pos);
                }
                let v = self.expr_to(cx, &args[0], &elem)?;
                return Ok((format!("{{ let __v = {}; {}{}.push(__v); }}", v, pre, place), Type::Void));
            }
            if !args.is_empty() {
                return err("`pop` takes no arguments", pos);
            }
            return Ok((format!("{{ {}{}.pop() }}", pre, place), Type::Optional(Box::new(elem))));
        }

        // Arguments are evaluated before the receiver is borrowed, so they may use it freely.
        let (setup, rcode, rt) = self.receiver(cx, recv)?;
        match &rt {
            Type::Array(_) | Type::Str | Type::Map(..) => {
                let (code, t) = self.value_method(cx, &rcode, &rt, name, args, pos)?;
                Ok((if setup.is_empty() { code } else { format!("{{ {}{} }}", setup, code) }, t))
            }
            Type::Generic(c, targs) => {
                let (owner, sig) = match self.m.find_method(c, name) {
                    Some(x) => x,
                    None => return err(format!("`{}` has no method `{}`", rt, name), pos),
                };
                self.check_method_access(cx, &owner, name, pos)?;
                let sig = subst_sig(&sig, &self.m.bindings(c, targs));
                let (binds, names) = self.args(cx, &sig, args, pos, name)?;
                Ok((format!("{{ {}{}let __r = &{}; let __t = __r.borrow_mut().{}({}); __t }}", setup, binds, rcode, rid(name), names), sig.ret))
            }
            Type::Class(c) => {
                let sig = match self.m.find_method(c, name) {
                    Some((owner, s)) => {
                        self.check_method_access(cx, &owner, name, pos)?;
                        s
                    }
                    None if self.m.find_static(c, name).is_some() => {
                        return err(format!("`{}` is a `static fn`; call it as `{}.{}(...)`", name, c, name), pos)
                    }
                    None => {
                        if let Some(Type::Func(ps, r)) = self.func_field(c, name) {
                            self.field_type(cx, c, name, pos)?;
                            let read = if self.m.has_subclasses(c) {
                                format!("{{ {}let __t = {}.borrow().__get_{}(); __t }}", setup, rcode, name)
                            } else {
                                format!("{{ {}let __t = {}.borrow().{}.clone(); __t }}", setup, rcode, rid(name))
                            };
                            return self.call_value(cx, read, &ps, &r, args, pos, name);
                        }
                        return err(format!("`{}` has no method `{}`", c, name), pos);
                    }
                };
                let (binds, names) = self.args(cx, &sig, args, pos, name)?;
                let call = if self.m.has_subclasses(c) {
                    let tr = self.m.dispatch_trait(c, name);
                    let sep = if names.is_empty() { "" } else { ", " };
                    format!("{}::{}(&mut *__r.borrow_mut(){}{})", tr, rid(name), sep, names)
                } else {
                    format!("__r.borrow_mut().{}({})", rid(name), names)
                };
                Ok((format!("{{ {}{}let __r = &{}; let __t = {}; __t }}", setup, binds, rcode, call), sig.ret))
            }
            Type::Interface(i) => {
                let sig = match self.m.interface_method(i, name) {
                    Some(s) => s,
                    None => return err(format!("interface `{}` has no method `{}`", i, name), pos),
                };
                let (binds, names) = self.args(cx, &sig, args, pos, name)?;
                let sep = if names.is_empty() { "" } else { ", " };
                Ok((
                    format!(
                        "{{ {}{}let __r = &{}; let __t = {}::{}(&mut *__r.borrow_mut(){}{}); __t }}",
                        setup,
                        binds,
                        rcode,
                        self.m.declaring_interface(i, name).unwrap(),
                        rid(name),
                        sep,
                        names
                    ),
                    sig.ret,
                ))
            }
            other => err(format!("`{}` has no method `{}`", other, name), pos),
        }
    }

    /// Non-mutating methods on arrays and strings.
    fn value_method(&self, cx: &mut Ctx, rcode: &str, t: &Type, name: &str, args: &[Expr], pos: Pos) -> Result<(String, Type)> {
        let arity = |n: usize| -> Result<()> {
            if args.len() != n { err(format!("`{}` takes {} argument(s)", name, n), pos) } else { Ok(()) }
        };
        match (t, name) {
            (Type::Array(_), "len") => {
                arity(0)?;
                Ok((format!("({}.len() as i64)", rcode), Type::Int))
            }
            (Type::Array(elem), "map") => {
                arity(1)?;
                let hint = Type::Func(vec![(**elem).clone()], Box::new(Type::Unknown));
                let (f, ft) = self.expr(cx, &args[0], Some(&hint))?;
                let out = match &ft {
                    Type::Func(ps, r) if ps.len() == 1 && self.m.assignable(elem, &ps[0]) && **r != Type::Void => (**r).clone(),
                    _ => return err(format!("`map` needs a function `fn({}) -> T`, found `{}`", elem, ft), args[0].pos),
                };
                let arg = self.coerce("__e.clone()".into(), elem, &match &ft { Type::Func(ps, _) => ps[0].clone(), _ => unreachable!() }, pos)?;
                Ok((format!("{{ let __f = {}; {}.iter().map(|__e| __f({})).collect::<Vec<_>>() }}", f, rcode, arg), Type::Array(Box::new(out))))
            }
            (Type::Array(elem), "filter" | "find") => {
                arity(1)?;
                let fty = Type::Func(vec![(**elem).clone()], Box::new(Type::Bool));
                let f = self.expr_to(cx, &args[0], &fty)?;
                if name == "filter" {
                    Ok((format!("{{ let __f = {}; {}.iter().filter(|__e| __f((*__e).clone())).cloned().collect::<Vec<_>>() }}", f, rcode), t.clone()))
                } else {
                    Ok((format!("{{ let __f = {}; {}.iter().find(|__e| __f((*__e).clone())).cloned() }}", f, rcode), Type::Optional(elem.clone())))
                }
            }
            (Type::Array(elem), "contains") => {
                arity(1)?;
                if !self.m.is_plain(elem) {
                    return err("`contains` only works on arrays of simple values", pos);
                }
                let v = self.expr_to(cx, &args[0], elem)?;
                Ok((format!("{}.contains(&{})", rcode, v), Type::Bool))
            }
            (Type::Map(_, _), "len") => {
                arity(0)?;
                Ok((format!("({}.len() as i64)", rcode), Type::Int))
            }
            (Type::Map(k, v), "get") => {
                arity(1)?;
                let kc = self.expr_to(cx, &args[0], k)?;
                Ok((format!("{}.get(&{}).cloned()", rcode, kc), Type::Optional(v.clone())))
            }
            (Type::Map(k, _), "has") => {
                arity(1)?;
                let kc = self.expr_to(cx, &args[0], k)?;
                Ok((format!("{}.contains_key(&{})", rcode, kc), Type::Bool))
            }
            (Type::Map(k, _), "keys") => {
                arity(0)?;
                Ok((format!("{}.keys().cloned().collect::<Vec<_>>()", rcode), Type::Array(k.clone())))
            }
            (Type::Map(_, v), "values") => {
                arity(0)?;
                Ok((format!("{}.values().cloned().collect::<Vec<_>>()", rcode), Type::Array(v.clone())))
            }
            (Type::Str, "len") => {
                arity(0)?;
                Ok((format!("({}.chars().count() as i64)", rcode), Type::Int))
            }
            (Type::Str, "upper") => {
                arity(0)?;
                Ok((format!("{}.to_uppercase()", rcode), Type::Str))
            }
            (Type::Str, "lower") => {
                arity(0)?;
                Ok((format!("{}.to_lowercase()", rcode), Type::Str))
            }
            (Type::Str, "trim") => {
                arity(0)?;
                Ok((format!("{}.trim().to_string()", rcode), Type::Str))
            }
            (Type::Str, "contains") => {
                arity(1)?;
                let v = self.expr_to(cx, &args[0], &Type::Str)?;
                Ok((format!("{}.contains(({}).as_str())", rcode, v), Type::Bool))
            }
            _ => err(format!("`{}` has no method `{}`", t, name), pos),
        }
    }
}

/// Module name for a `use rust` file: its file name, made into a Rust identifier.
fn rust_module_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("native")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

/// `<T: Clone + 'static, U: ...>` for a Rust generic item (empty when not generic).
fn generic_params(tparams: &[String]) -> String {
    if tparams.is_empty() {
        String::new()
    } else {
        format!("<{}>", tparams.iter().map(|t| format!("{}: Clone + 'static", t)).collect::<Vec<_>>().join(", "))
    }
}

/// Replaces type parameters with `Unknown`, for use as an expected-type hint.
fn unknown_params(t: &Type) -> Type {
    match t {
        Type::Param(_) => Type::Unknown,
        Type::Array(i) => Type::Array(Box::new(unknown_params(i))),
        Type::Optional(i) => Type::Optional(Box::new(unknown_params(i))),
        Type::Map(k, v) => Type::Map(Box::new(unknown_params(k)), Box::new(unknown_params(v))),
        Type::Func(ps, r) => Type::Func(ps.iter().map(unknown_params).collect(), Box::new(unknown_params(r))),
        Type::Generic(n, args) => Type::Generic(n.clone(), args.iter().map(unknown_params).collect()),
        other => other.clone(),
    }
}

fn op_sym(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Set => "",
        AssignOp::Add => "+",
        AssignOp::Sub => "-",
        AssignOp::Mul => "*",
        AssignOp::Div => "/",
        AssignOp::Rem => "%",
    }
}

/// Conservative check that a body returns on every path.
fn always_returns(stmts: &[Stmt]) -> bool {
    match stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Return(_)) => true,
        Some(StmtKind::If { then, otherwise: Some(o), .. }) | Some(StmtKind::IfLet { then, otherwise: Some(o), .. }) => {
            always_returns(then) && always_returns(o)
        }
        Some(StmtKind::While { cond: Expr { kind: ExprKind::Bool(true), .. }, body }) => !has_break(body),
        // Non-exhaustive matches are rejected during code generation.
        Some(StmtKind::Match { arms, .. }) => arms.iter().all(|a| always_returns(&a.body)),
        _ => false,
    }
}

/// Does a loop body contain a `break` for this loop (not nested loops)?
fn has_break(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Break => true,
        StmtKind::If { then, otherwise, .. } | StmtKind::IfLet { then, otherwise, .. } => {
            has_break(then) || otherwise.as_deref().is_some_and(has_break)
        }
        StmtKind::Match { arms, .. } => arms.iter().any(|a| has_break(&a.body)),
        _ => false,
    })
}

const PRELUDE: &str = r#"// Generated by the Kalinga compiler. Do not edit.
#![allow(warnings)]
#![allow(arithmetic_overflow, unconditional_panic)]
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

trait __KlShow { fn __show(&self) -> String; }
impl __KlShow for i64 { fn __show(&self) -> String { self.to_string() } }
impl __KlShow for f64 {
    fn __show(&self) -> String {
        if self.is_finite() && self.fract() == 0.0 && self.abs() < 1e16 { format!("{:.1}", self) } else { self.to_string() }
    }
}
impl __KlShow for bool { fn __show(&self) -> String { self.to_string() } }
impl __KlShow for String { fn __show(&self) -> String { self.clone() } }
impl<T: __KlShow> __KlShow for Vec<T> {
    fn __show(&self) -> String {
        let parts: Vec<String> = self.iter().map(|v| v.__show()).collect();
        format!("[{}]", parts.join(", "))
    }
}
impl<K: __KlShow, V: __KlShow> __KlShow for BTreeMap<K, V> {
    fn __show(&self) -> String {
        let parts: Vec<String> = self.iter().map(|(k, v)| format!("{}: {}", k.__show(), v.__show())).collect();
        format!("{{{}}}", parts.join(", "))
    }
}
impl<T: __KlShow> __KlShow for Option<T> {
    fn __show(&self) -> String { match self { Some(v) => v.__show(), None => "none".to_string() } }
}

#[inline(always)]
fn __idx(i: i64) -> usize {
    if i < 0 { panic!("index {} is negative", i) }
    i as usize
}
fn __map_get<K: Ord + __KlShow, V: Clone>(m: &BTreeMap<K, V>, k: &K) -> V {
    match m.get(k) { Some(v) => v.clone(), None => panic!("key not found: {}", k.__show()) }
}
fn __map_slot<K: Ord + __KlShow, V>(m: &mut BTreeMap<K, V>, k: K) -> &mut V {
    let shown = k.__show();
    match m.get_mut(&k) { Some(v) => v, None => panic!("key not found: {}", shown) }
}
fn __clamp<T: PartialOrd>(x: T, lo: T, hi: T) -> T {
    if x < lo { lo } else if x > hi { hi } else { x }
}

// Random: xorshift64*, seeded from the clock (fast, not for cryptography).
thread_local! {
    static __RNG: std::cell::Cell<u64> = std::cell::Cell::new({
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        (t ^ 0x9E37_79B9_7F4A_7C15) | 1
    });
}
fn __rand_u64() -> u64 {
    __RNG.with(|s| {
        let mut x = s.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}
fn __rand_seed(n: i64) {
    __RNG.with(|s| s.set(((n as u64) ^ 0x9E37_79B9_7F4A_7C15) | 1));
}
fn __rand_int(lo: i64, hi: i64) -> i64 {
    if hi <= lo { panic!("Random.int({}, {}): the second number must be greater than the first", lo, hi) }
    let span = (hi as i128 - lo as i128) as u128;
    (lo as i128 + (__rand_u64() as u128 % span) as i128) as i64
}
fn __rand_float() -> f64 {
    (__rand_u64() >> 11) as f64 / (1u64 << 53) as f64
}
fn __shuffle<T>(xs: &mut Vec<T>) {
    for i in (1..xs.len()).rev() {
        let j = __rand_int(0, i as i64 + 1) as usize;
        xs.swap(i, j);
    }
}

// Time
static __START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
fn __time_now() -> f64 {
    __START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}
fn __time_millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
fn __time_sleep(seconds: f64) {
    if seconds > 0.0 { std::thread::sleep(std::time::Duration::from_secs_f64(seconds)) }
}

fn __unset<T>(v: Option<T>, name: &str) -> T {
    match v { Some(x) => x, None => panic!("field `{}` was read before it was set", name) }
}
fn __parse_int(s: &str) -> i64 {
    match s.trim().parse::<i64>() { Ok(v) => v, Err(_) => panic!("cannot convert \"{}\" to int", s) }
}
fn __parse_float(s: &str) -> f64 {
    match s.trim().parse::<f64>() { Ok(v) => v, Err(_) => panic!("cannot convert \"{}\" to float", s) }
}
fn __input(prompt: &str) -> String {
    use std::io::Write;
    print!("{}", prompt);
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).ok();
    s.trim_end_matches(['\r', '\n']).to_string()
}
fn __kl_run(f: fn()) {
    __START.get_or_init(std::time::Instant::now);
    std::panic::set_hook(Box::new(|info| {
        let p = info.payload();
        let msg = if let Some(s) = p.downcast_ref::<&str>() { s.to_string() }
            else if let Some(s) = p.downcast_ref::<String>() { s.clone() }
            else { "unknown error".to_string() };
        let msg = if msg.contains("already borrowed") || msg.contains("already mutably borrowed") {
            format!("{} (an object was used while one of its own methods was running)", msg)
        } else { msg };
        eprintln!("runtime error: {}", msg);
    }));
    let worker = std::thread::Builder::new().stack_size(256 * 1024 * 1024).spawn(f).expect("failed to start");
    if worker.join().is_err() { std::process::exit(101); }
}
"#;
