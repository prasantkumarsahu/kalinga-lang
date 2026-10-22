//! Type-checks function and method bodies and emits Rust source.
//!
//! Object model (see README): every object is `Rc<RefCell<Struct>>`. A class with
//! subclasses is referred to through `Rc<RefCell<dyn __I_Class>>` so calls dispatch
//! dynamically; leaf classes use the concrete struct for static dispatch.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::ast::*;
use crate::checker::{class_trait, Model, Signature, Type, BUILTINS};
use crate::error::{err, Pos, Result};

pub fn generate(prog: &Program, model: &Model) -> Result<String> {
    let g = Gen { m: model };
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

struct Var {
    ty: Type,
    mutable: bool,
    rust: String,
}

struct Ctx {
    scopes: Vec<HashMap<String, Var>>,
    /// Class whose method/init body is being generated.
    class: Option<String>,
    ret: Type,
    in_init: bool,
    loops: usize,
}

impl Ctx {
    fn new(class: Option<String>, ret: Type, in_init: bool) -> Ctx {
        Ctx { scopes: vec![HashMap::new()], class, ret, in_init, loops: 0 }
    }

    fn lookup(&self, name: &str) -> Option<&Var> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn declare(&mut self, name: &str, ty: Type, mutable: bool, pos: Pos) -> Result<String> {
        let scope = self.scopes.last_mut().unwrap();
        if scope.contains_key(name) {
            return err(format!("`{}` is already declared in this scope", name), pos);
        }
        let rust = rid(name);
        scope.insert(name.to_string(), Var { ty, mutable, rust: rust.clone() });
        Ok(rust)
    }
}

struct Gen<'a> {
    m: &'a Model,
}

fn clone_if(code: String, ty: &Type) -> String {
    if ty.is_copy() { code } else { format!("{}.clone()", code) }
}

fn showable(t: &Type) -> bool {
    match t {
        Type::Int | Type::Float | Type::Bool | Type::Str => true,
        Type::Array(i) | Type::Optional(i) => showable(i),
        _ => false,
    }
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
            Type::Optional(i) => format!("Option<{}>", self.rty(i)),
            Type::Class(c) => {
                if self.m.has_subclasses(c) {
                    format!("Rc<RefCell<dyn {}>>", class_trait(c))
                } else {
                    format!("Rc<RefCell<{}>>", c)
                }
            }
            Type::Interface(i) => format!("Rc<RefCell<dyn {}>>", i),
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
            (Type::Class(_), Type::Class(_)) | (Type::Class(_), Type::Interface(_)) => {
                format!("({} as {})", code, self.rty(to))
            }
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
        let mut out = String::new();
        out.push_str(PRELUDE);

        for iname in &self.m.interface_order {
            let info = &self.m.interfaces[iname];
            writeln!(out, "\n// interface {}", iname).unwrap();
            writeln!(out, "trait {} {{", iname).unwrap();
            for (mname, sig) in &info.methods {
                writeln!(out, "    fn {}(&mut self{}){};", rid(mname), self.comma_params(sig), self.ret_suffix(&sig.ret)).unwrap();
            }
            out.push_str("}\n");
        }

        // Bodies are generated once per defining class and pasted into every subclass.
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
                        self.expr_to(&mut cx, e, &ty)?
                    }
                    None => self.default_value(&ty),
                };
                field_defaults.insert((cname.clone(), f.name.clone()), code);
            }
            for meth in &info.decl.methods {
                let sig = info.methods[&meth.name].clone();
                let body = self.body(Some(cname.clone()), &sig, &meth.body, false, meth.pos, &meth.name, 2)?;
                method_bodies.insert((cname.clone(), meth.name.clone()), body);
            }
            if let Some(init) = &info.decl.init {
                let sig = info.init.clone().unwrap();
                let body = self.body(Some(cname.clone()), &sig, &init.body, true, init.pos, "init", 2)?;
                init_bodies.insert(cname.clone(), body);
            }
        }

        for cname in &self.m.class_order {
            self.class(&mut out, cname, &method_bodies, &init_bodies, &field_defaults);
        }

        for f in &prog.functions {
            let sig = self.m.functions[&f.name].clone();
            let body = self.body(None, &sig, &f.body, false, f.pos, &f.name, 1)?;
            writeln!(
                out,
                "\nfn {}({}){} {{\n{}}}",
                fn_id(&f.name),
                self.params_decl(&sig),
                self.ret_suffix(&sig.ret),
                body
            )
            .unwrap();
        }

        writeln!(out, "\nfn main() {{\n    __kl_run({});\n}}", fn_id("main")).unwrap();
        Ok(out)
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

        writeln!(out, "\n// ===== class {} =====", cname).unwrap();
        // struct
        writeln!(out, "struct {} {{", cname).unwrap();
        for (f, t, _) in &fields {
            writeln!(out, "    {}: {},", rid(f), self.rty(t)).unwrap();
        }
        out.push_str("}\n");

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
        out.push_str("}\n");

        // inherent impl
        writeln!(out, "impl {} {{", cname).unwrap();
        let ctor = m.find_init(cname);
        let ctor_params = ctor.as_ref().map(|(_, s)| self.params_decl(s)).unwrap_or_default();
        writeln!(out, "    fn __new({}) -> Rc<RefCell<{}>> {{", ctor_params, cname).unwrap();
        writeln!(out, "        let mut __o = {} {{", cname).unwrap();
        for (f, _, owner) in &fields {
            writeln!(out, "            {}: {},", rid(f), field_defaults[&(owner.clone(), f.clone())]).unwrap();
        }
        out.push_str("        };\n");
        if let Some((owner, sig)) = &ctor {
            writeln!(out, "        __o.__{}__init({});", owner, self.params_names(sig)).unwrap();
        }
        out.push_str("        Rc::new(RefCell::new(__o))\n    }\n");
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
            out.push_str("}\n");
        }
        for iname in m.all_interfaces(cname) {
            writeln!(out, "impl {} for {} {{", iname, cname).unwrap();
            for (mname, sig) in &m.interfaces[&iname].methods {
                forward(out, mname, sig);
            }
            out.push_str("}\n");
        }
    }

    /// Generates the statements of a function, method or init body.
    #[allow(clippy::too_many_arguments)]
    fn body(
        &self,
        class: Option<String>,
        sig: &Signature,
        stmts: &[Stmt],
        in_init: bool,
        pos: Pos,
        name: &str,
        depth: usize,
    ) -> Result<String> {
        let mut cx = Ctx::new(class, sig.ret.clone(), in_init);
        for (p, t) in &sig.params {
            cx.declare(p, t.clone(), false, pos)?;
        }
        if sig.ret != Type::Void && !always_returns(stmts) {
            return err(format!("`{}` must return a `{}` on every path", name, sig.ret), pos);
        }
        self.block_inner(&mut cx, stmts, depth)
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
                        let t = self.m.resolve(te).map_err(|mut e| {
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
                        let ft = self.field_type(c, f, target.pos)?;
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
        let v = {
            let mut n = 0;
            let (pre, place, ty) = self.place(cx, target, &mut n)?;
            let v = self.assign_value(cx, op, &ty, value)?;
            let stmt = match (op, &ty) {
                (AssignOp::Set, _) => format!("{} = __v;", place),
                (AssignOp::Add, Type::Str) => format!("{}.push_str(&__v);", place),
                _ => format!("{} {}= __v;", place, op_sym(op)),
            };
            format!("{{ let __v = {}; {}{} }}", v, pre, stmt)
        };
        Ok(v)
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
                Some(_) => err(format!("cannot change `{}`: it was declared with `let` (use `var` for a mutable variable)", name), e.pos),
                None => err(format!("unknown variable `{}`", name), e.pos),
            },
            ExprKind::Field(recv, f) => {
                if matches!(recv.kind, ExprKind::SelfRef) {
                    let class = self.self_class(cx, recv.pos)?;
                    let t = self.field_type(&class, f, e.pos)?;
                    return Ok((String::new(), format!("self.{}", rid(f)), t));
                }
                let (rcode, rt) = self.recv(cx, recv)?;
                match &rt {
                    Type::Class(c) if !self.m.has_subclasses(c) => {
                        let t = self.field_type(c, f, e.pos)?;
                        Ok((String::new(), format!("{}.borrow_mut().{}", rcode, rid(f)), t))
                    }
                    Type::Class(_) => err("cannot modify part of a field through a base-class reference; add a method to the class instead", e.pos),
                    other => err(format!("`{}` has no fields", other), e.pos),
                }
            }
            ExprKind::Index(base, idx) => {
                let (pre, bplace, bt) = self.place(cx, base, n)?;
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
        cx.class.clone().ok_or_else(|| crate::error::CompileError::new("`self` is only available inside class methods", pos))
    }

    fn field_type(&self, class: &str, field: &str, pos: Pos) -> Result<Type> {
        match self.m.all_fields(class).into_iter().find(|(n, _, _)| n == field) {
            Some((_, t, _)) => Ok(t),
            None => err(format!("`{}` has no field `{}`", class, field), pos),
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
            ExprKind::SelfRef => err("`self` cannot be used as a value here (use `self.field` or `self.method()`)", e.pos),
            ExprKind::Ident(name) => match cx.lookup(name) {
                Some(v) => Ok((clone_if(v.rust.clone(), &v.ty), v.ty.clone())),
                None if self.m.functions.contains_key(name) => err(format!("`{}` is a function; call it with `{}(...)`", name, name), e.pos),
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
            ExprKind::Unary(op, inner) => {
                let (code, t) = self.expr(cx, inner, None)?;
                match (op, &t) {
                    (UnOp::Neg, Type::Int | Type::Float) => Ok((format!("(-{})", code), t)),
                    (UnOp::Not, Type::Bool) => Ok((format!("(!{})", code), t)),
                    (UnOp::Neg, _) => err(format!("cannot negate `{}`", t), e.pos),
                    (UnOp::Not, _) => err(format!("`!` needs a `bool`, found `{}`", t), e.pos),
                }
            }
            ExprKind::Binary(op, l, r) => self.binary(cx, *op, l, r, e.pos),
            ExprKind::Call(name, args) => self.call(cx, name, args, e.pos),
            ExprKind::MethodCall(recv, name, args) => self.method_call(cx, recv, name, args, e.pos),
            ExprKind::Field(recv, f) => {
                if matches!(recv.kind, ExprKind::SelfRef) {
                    let class = self.self_class(cx, recv.pos)?;
                    let t = self.field_type(&class, f, e.pos)?;
                    return Ok((clone_if(format!("self.{}", rid(f)), &t), t));
                }
                let (rcode, rt) = self.recv(cx, recv)?;
                match &rt {
                    Type::Class(c) => {
                        let t = self.field_type(c, f, e.pos)?;
                        let read = if self.m.has_subclasses(c) {
                            format!("{}.borrow().__get_{}()", rcode, f)
                        } else {
                            clone_if(format!("{}.borrow().{}", rcode, rid(f)), &t)
                        };
                        Ok((format!("{{ let __t = {}; __t }}", read), t))
                    }
                    Type::Array(_) | Type::Str if f == "len" => err("`len` is a method: use `.len()`", e.pos),
                    Type::Interface(i) => err(format!("interface `{}` has no fields; only methods can be used", i), e.pos),
                    other => err(format!("`{}` has no field `{}`", other, f), e.pos),
                }
            }
            ExprKind::Index(base, idx) => {
                let (bcode, bt) = self.recv(cx, base)?;
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
                let (binds, names) = self.args(cx, &sig, args, e.pos, name)?;
                Ok((format!("{{ {}self.__{}__{}({}) }}", binds, owner, name, names), sig.ret))
            }
        }
    }

    fn binary(&self, cx: &mut Ctx, op: BinOp, l: &Expr, r: &Expr, pos: Pos) -> Result<(String, Type)> {
        let (lc, lt) = self.expr(cx, l, None)?;
        let (rc, rt) = self.expr(cx, r, None)?;
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
                if !showable(&lt) {
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

    fn call(&self, cx: &mut Ctx, name: &str, args: &[Expr], pos: Pos) -> Result<(String, Type)> {
        if BUILTINS.contains(&name) {
            return self.builtin(cx, name, args, pos);
        }
        if self.m.classes.contains_key(name) {
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
                    if !showable(&t) {
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
                    Type::Array(_) => Ok((format!("({}.len() as i64)", c), Type::Int)),
                    Type::Str => Ok((format!("({}.chars().count() as i64)", c), Type::Int)),
                    other => err(format!("`len` needs an array or string, found `{}`", other), pos),
                }
            }
            "str" => {
                let (c, t) = one(cx)?;
                if !showable(&t) {
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
        if matches!(recv.kind, ExprKind::SelfRef) {
            let class = self.self_class(cx, recv.pos)?;
            let sig = match self.m.find_method(&class, name) {
                Some((_, s)) => s,
                None => return err(format!("`{}` has no method `{}`", class, name), pos),
            };
            let (binds, names) = self.args(cx, &sig, args, pos, name)?;
            return Ok((format!("{{ {}self.{}({}) }}", binds, rid(name), names), sig.ret));
        }

        // Mutating array methods need a mutable place.
        if name == "push" || name == "pop" {
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

        let (rcode, rt) = self.recv(cx, recv)?;
        match &rt {
            Type::Array(_) | Type::Str => self.value_method(cx, &rcode, &rt, name, args, pos),
            Type::Class(c) => {
                let sig = match self.m.find_method(c, name) {
                    Some((_, s)) => s,
                    None => return err(format!("`{}` has no method `{}`", c, name), pos),
                };
                let (binds, names) = self.args(cx, &sig, args, pos, name)?;
                let call = if self.m.has_subclasses(c) {
                    let tr = self.m.dispatch_trait(c, name);
                    let sep = if names.is_empty() { "" } else { ", " };
                    format!("{}::{}(&mut *__r.borrow_mut(){}{})", tr, rid(name), sep, names)
                } else {
                    format!("__r.borrow_mut().{}({})", rid(name), names)
                };
                Ok((format!("{{ let __r = &{}; {}let __t = {}; __t }}", rcode, binds, call), sig.ret))
            }
            Type::Interface(i) => {
                let sig = match self.m.interface_method(i, name) {
                    Some(s) => s,
                    None => return err(format!("interface `{}` has no method `{}`", i, name), pos),
                };
                let (binds, names) = self.args(cx, &sig, args, pos, name)?;
                let sep = if names.is_empty() { "" } else { ", " };
                Ok((
                    format!("{{ let __r = &{}; {}let __t = {}::{}(&mut *__r.borrow_mut(){}{}); __t }}", rcode, binds, i, rid(name), sep, names),
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
            (Type::Array(elem), "contains") => {
                arity(1)?;
                if !showable(elem) {
                    return err("`contains` only works on arrays of simple values", pos);
                }
                let v = self.expr_to(cx, &args[0], elem)?;
                Ok((format!("{}.contains(&{})", rcode, v), Type::Bool))
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
        _ => false,
    })
}

const PRELUDE: &str = r#"// Generated by the Kalinga compiler. Do not edit.
#![allow(warnings)]
#![allow(arithmetic_overflow, unconditional_panic)]
use std::cell::RefCell;
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
impl<T: __KlShow> __KlShow for Option<T> {
    fn __show(&self) -> String { match self { Some(v) => v.__show(), None => "none".to_string() } }
}

#[inline(always)]
fn __idx(i: i64) -> usize {
    if i < 0 { panic!("index {} is negative", i) }
    i as usize
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
