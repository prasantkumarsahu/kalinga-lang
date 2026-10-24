//! Builds the declaration model from a parsed program and checks the declaration-level
//! rules: names, inheritance, overrides, interface conformance and constructors.
//!
//! `Model::build` runs a fixed sequence of passes; each reports the first problem it
//! finds, in source order.

use std::collections::{HashMap, HashSet};

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Diagnostic, Result, Span, err};

use crate::model::*;
use crate::types::{Signature, Type, subst_sig};

impl Model {
    /// `require_main` is false when only checking a file, which may be a library
    /// meant to be imported by another file. The error is the first problem found.
    pub fn build(prog: &Program, require_main: bool) -> Result<Model> {
        let (model, errors) = Model::build_all(prog, require_main);
        match (model, errors.into_iter().next()) {
            (_, Some(e)) => Err(e),
            (Some(m), None) => Ok(m),
            (None, None) => err("the declarations could not be read", Span::default()),
        }
    }

    /// Like [`Model::build`], for editors: the model is also returned when only rules
    /// about classes, `main` or constants are broken (one problem per class), since its
    /// declarations are still complete.
    pub fn build_all(prog: &Program, require_main: bool) -> (Option<Model>, Vec<Diagnostic>) {
        let mut rules = vec![];
        let declared = (|| {
            let mut m = Model::empty(type_names(prog)?);
            m.declare_enums(prog)?;
            m.declare_structs(prog)?;
            m.check_value_types(prog)?;
            m.declare_interfaces(prog)?;
            m.declare_functions(prog)?;
            m.declare_consts(prog)?;
            rules.extend(m.check_main(prog, require_main).err());
            rules.extend(check_tests(prog).err());
            m.declare_classes(prog)?;
            m.order_classes(prog)?;
            Ok(m)
        })();
        let m = match declared {
            Ok(m) => m,
            Err(e) => {
                rules.push(e);
                return (None, rules);
            }
        };
        for n in &m.class_order {
            rules.extend(m.check_class(n).err());
        }
        rules.extend(m.check_static_cycles(prog).err());
        (Some(m), rules)
    }

    fn empty(names: TypeNames) -> Model {
        Model {
            classes: HashMap::new(),
            class_order: vec![],
            interfaces: HashMap::new(),
            interface_order: vec![],
            functions: HashMap::new(),
            extern_fns: HashMap::new(),
            fn_tparams: HashMap::new(),
            fn_bounds: HashMap::new(),
            async_fns: HashSet::new(),
            enums: HashMap::new(),
            enum_order: vec![],
            plain_enums: HashSet::new(),
            consts: HashMap::new(),
            structs: HashMap::new(),
            struct_order: vec![],
            plain_structs: HashSet::new(),
            names,
        }
    }

    /// Type parameters must be uppercase, distinct and not shadow a type.
    fn check_tparams(&self, tps: &[String], pos: Span) -> Result<()> {
        for (i, t) in tps.iter().enumerate() {
            if !t.starts_with(|c: char| c.is_uppercase()) {
                return err(format!("type parameters must start with an uppercase letter: `{}`", t), pos);
            }
            if self.names.contains(t) || RESERVED_TYPES.contains(&t.as_str()) {
                return err(format!("type parameter `{}` has the same name as a type", t), pos);
            }
            if tps[..i].contains(t) {
                return err(format!("type parameter `{}` listed twice", t), pos);
            }
        }
        Ok(())
    }

    fn declare_enums(&mut self, prog: &Program) -> Result<()> {
        for e in &prog.enums {
            if e.variants.is_empty() {
                return err(format!("enum `{}` needs at least one variant", e.name), e.pos);
            }
            self.check_tparams(&e.tparams, e.pos)?;
            let mut variants: Vec<(String, Vec<(String, Type)>)> = Vec::new();
            for v in &e.variants {
                if !v.name.starts_with(|c: char| c.is_uppercase()) || v.name == "Self" {
                    return err(format!("variant names must start with an uppercase letter: `{}`", v.name), v.pos);
                }
                if variants.iter().any(|(n, _)| n == &v.name) {
                    return err(format!("variant `{}` declared twice in `{}`", v.name, e.name), v.pos);
                }
                let sig = signature(&v.fields, &TypeExpr::Void, &|t| resolve_type(t, &self.names, &e.tparams))?;
                variants.push((v.name.clone(), sig.params));
            }
            let info = EnumInfo {
                tparams: e.tparams.clone(),
                bounds: resolve_bounds(&e.bounds, &self.names, &e.tparams)?,
                variants,
                ..EnumInfo::default()
            };
            self.enums.insert(e.name.clone(), info);
            self.enum_order.push(e.name.clone());
        }
        Ok(())
    }

    fn declare_structs(&mut self, prog: &Program) -> Result<()> {
        for st in &prog.structs {
            self.check_tparams(&st.tparams, st.pos)?;
            let info = struct_info(st, &self.names)?;
            self.structs.insert(st.name.clone(), info);
            self.struct_order.push(st.name.clone());
        }
        Ok(())
    }

    /// Enum fields that lead back to their enum are boxed; structs must not contain
    /// themselves by value. Enums and structs are "plain" (printable, comparable) when all
    /// their parts are.
    fn check_value_types(&mut self, prog: &Program) -> Result<()> {
        self.find_boxed_fields();
        for st in &prog.structs {
            if self.value_contains(&st.name, &st.name, &mut HashSet::new()) {
                return err(
                    format!(
                        "struct `{}` contains itself; put the recursive part in an array (`[{}]`), an enum or a class",
                        st.name, st.name
                    ),
                    st.pos,
                );
            }
        }
        self.find_plain_types();
        Ok(())
    }

    /// Decided for every enum before any is marked, so the result does not depend on order.
    fn find_boxed_fields(&mut self) {
        let boxed: Vec<(String, HashSet<(String, usize)>)> = self
            .enums
            .iter()
            .map(|(name, info)| {
                let fields =
                    info.variants.iter().flat_map(|(v, fs)| fs.iter().enumerate().map(move |(i, f)| (v, i, f)));
                let set = fields.filter(|(_, _, (_, t))| self.type_contains(t, name)).map(|(v, i, _)| (v.clone(), i));
                (name.clone(), set.collect())
            })
            .collect();
        for (name, set) in boxed {
            if let Some(info) = self.enums.get_mut(&name) {
                info.boxed = set;
            }
        }
    }

    /// Iterates to a fixpoint, since enums and structs can contain each other.
    fn find_plain_types(&mut self) {
        self.plain_enums = self.enums.keys().cloned().collect();
        self.plain_structs = self.structs.keys().cloned().collect();
        loop {
            let before = self.plain_enums.len() + self.plain_structs.len();
            let bad_enums: Vec<String> = self
                .plain_enums
                .iter()
                .filter(|n| {
                    // As for generic structs, type parameters count as plain here.
                    self.enums[*n].variants.iter().any(|(_, fs)| fs.iter().any(|(_, t)| !self.is_plain_params_ok(t)))
                })
                .cloned()
                .collect();
            // For generic structs, type parameters count as plain here; uses then also need
            // plain type arguments (see `is_plain`).
            let bad_structs: Vec<String> = self
                .plain_structs
                .iter()
                .filter(|n| self.structs[*n].fields.iter().any(|(_, t)| !self.is_plain_params_ok(t)))
                .cloned()
                .collect();
            for b in bad_enums {
                self.plain_enums.remove(&b);
            }
            for b in bad_structs {
                self.plain_structs.remove(&b);
            }
            if self.plain_enums.len() + self.plain_structs.len() == before {
                break;
            }
        }
    }

    fn declare_interfaces(&mut self, prog: &Program) -> Result<()> {
        for i in &prog.interfaces {
            self.check_tparams(&i.tparams, i.pos)?;
            let resolve = |t: &TypeExpr| resolve_type(t, &self.names, &i.tparams);
            let mut methods: Vec<(String, Signature)> = Vec::new();
            for s in &i.methods {
                if methods.iter().any(|(n, _)| n == &s.name) {
                    return err(format!("method `{}` declared twice in `{}`", s.name, i.name), s.pos);
                }
                methods.push((s.name.clone(), signature(&s.params, &s.ret, &resolve)?));
            }
            let mut supers = Vec::new();
            let mut super_args = HashMap::new();
            for ((b, pos), args) in i.bases.iter().zip(&i.base_args) {
                if !self.names.ifaces.contains(b) {
                    return err(format!("`{}` is not an interface (interfaces can only extend interfaces)", b), *pos);
                }
                if supers.contains(b) {
                    return err(format!("interface `{}` listed twice", b), *pos);
                }
                supers.push(b.clone());
                if let Type::Interface(_, targs) = resolve(&TypeExpr::Named(b.clone(), args.clone(), *pos))? {
                    super_args.insert(b.clone(), targs);
                }
            }
            let info = InterfaceInfo { tparams: i.tparams.clone(), methods, supers, super_args };
            self.interfaces.insert(i.name.clone(), info);
            self.interface_order.push(i.name.clone());
        }
        for i in &prog.interfaces {
            self.check_interface(i)?;
        }
        Ok(())
    }

    /// No cycles, and a method inherited twice must have one signature.
    fn check_interface(&self, i: &InterfaceDecl) -> Result<()> {
        if self.iface_closure_of(&i.name).iter().skip(1).any(|s| s == &i.name) {
            return err(format!("interface `{}` extends itself", i.name), i.pos);
        }
        let mut seen: HashMap<String, (String, Signature)> = HashMap::new();
        for s in self.iface_closure(&i.name) {
            for (mname, sig) in &self.interfaces[&s].methods {
                if let Some((other, osig)) = seen.get(mname)
                    && other != &s
                    && osig != sig
                {
                    return err(
                        format!(
                            "`{}` gets method `{}` from both `{}` and `{}` with different signatures",
                            i.name, mname, other, s
                        ),
                        i.pos,
                    );
                }
                seen.insert(mname.clone(), (s.clone(), sig.clone()));
            }
        }
        Ok(())
    }

    fn declare_consts(&mut self, prog: &Program) -> Result<()> {
        for c in &prog.consts {
            if self.consts.contains_key(&c.name) {
                return err(format!("constant `{}` declared twice", c.name), c.pos);
            }
            if self.names.contains(&c.name) || self.functions.contains_key(&c.name) {
                return err(format!("`{}` is already the name of a type or function", c.name), c.pos);
            }
            let ty = resolve_type(&c.ty, &self.names, &[]).map_err(|e| at_pos(e, c.pos))?;
            self.consts.insert(c.name.clone(), ty);
        }
        Ok(())
    }

    fn declare_functions(&mut self, prog: &Program) -> Result<()> {
        for f in &prog.functions {
            if BUILTINS.contains(&f.name.as_str()) {
                return err(format!("`{}` is a built-in function and cannot be redefined", f.name), f.pos);
            }
            if self.names.contains(&f.name) {
                return err(format!("function `{}` has the same name as a type", f.name), f.pos);
            }
            if self.functions.contains_key(&f.name) {
                return err(format!("function `{}` is defined more than once", f.name), f.pos);
            }
            if f.is_override || f.is_private || f.is_static || f.is_abstract {
                return err("`override`, `private`, `static` and `abstract` are only allowed on class members", f.pos);
            }
            self.check_tparams(&f.tparams, f.pos)?;
            if f.name == "main" && !f.tparams.is_empty() {
                return err("`main` cannot be generic", f.pos);
            }
            let sig = signature(&f.params, &f.ret, &|t| resolve_type(t, &self.names, &f.tparams))?;
            self.functions.insert(f.name.clone(), sig);
            self.fn_tparams.insert(f.name.clone(), f.tparams.clone());
            self.fn_bounds.insert(f.name.clone(), resolve_bounds(&f.bounds, &self.names, &f.tparams)?);
            if f.is_async {
                self.async_fns.insert(f.name.clone());
            }
        }
        for x in &prog.extern_fns {
            if BUILTINS.contains(&x.name.as_str()) || self.names.contains(&x.name) {
                return err(format!("`{}` is already a built-in function or a type", x.name), x.pos);
            }
            if self.functions.contains_key(&x.name) || self.extern_fns.contains_key(&x.name) {
                return err(format!("function `{}` is defined more than once", x.name), x.pos);
            }
            let sig = signature(&x.params, &x.ret, &|t| resolve_type(t, &self.names, &[]))?;
            let path = match (&x.path, &x.body) {
                (_, Some(_)) => format!("__x_{}", x.name),
                (Some(p), None) => p.clone(),
                (None, None) => x.name.clone(),
            };
            self.extern_fns.insert(x.name.clone(), (sig, path));
        }
        Ok(())
    }

    fn check_main(&self, prog: &Program, require_main: bool) -> Result<()> {
        match self.functions.get("main") {
            None if require_main => err("program has no `fn main()`", Span::at(1, 1, 0)),
            Some(sig)
                if !sig.params.is_empty()
                    || !matches!(&sig.ret, Type::Void)
                        && !matches!(&sig.ret, Type::Result(t, e) if **t == Type::Void && self.is_plain(e)) =>
            {
                let pos = prog.functions.iter().find(|f| f.name == "main").map_or(Span::default(), |f| f.pos);
                err(
                    "`main` must take no parameters and return nothing (or `Result<void, E>` with a printable `E`)",
                    pos,
                )
            }
            _ => Ok(()),
        }
    }

    fn declare_classes(&mut self, prog: &Program) -> Result<()> {
        for c in &prog.classes {
            self.check_tparams(&c.tparams, c.pos)?;
            if !c.tparams.is_empty() {
                if !c.bases.is_empty() {
                    return err("generic classes cannot extend classes or implement interfaces (yet)", c.bases[0].1);
                }
                if c.is_abstract || c.methods.iter().any(|m| m.is_static) {
                    return err("generic classes cannot be abstract or have `static fn`s (yet)", c.pos);
                }
            }
            let info = class_info(c, &self.names)?;
            self.classes.insert(c.name.clone(), info);
        }
        Ok(())
    }

    /// Rejects inheritance cycles; orders classes parents first and links children.
    fn order_classes(&mut self, prog: &Program) -> Result<()> {
        for c in &prog.classes {
            let mut seen = HashSet::new();
            let mut cur = Some(c.name.clone());
            while let Some(n) = cur {
                if !seen.insert(n.clone()) {
                    return err(format!("inheritance cycle involving `{}`", c.name), c.pos);
                }
                cur = self.classes[&n].parent.clone();
            }
        }
        let mut sorted: Vec<&String> = prog.classes.iter().map(|c| &c.name).collect();
        sorted.sort_by_key(|n| self.ancestors(n).len());
        self.class_order = sorted.into_iter().cloned().collect();
        for n in self.class_order.clone() {
            if let Some(p) = self.classes[&n].parent.clone()
                && let Some(parent) = self.classes.get_mut(&p)
            {
                parent.children.push(n);
            }
        }
        Ok(())
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
        self.check_overrides(name)?;
        self.check_conformance(name)?;
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
            let calls_super =
                init.body.iter().any(|s| matches!(&s.kind, StmtKind::Expr(Expr { kind: ExprKind::SuperInit(_), .. })));
            if let Some((pname, _)) = parent_init
                && !calls_super
            {
                return err(format!("`init` must call `super(...)` to initialize base class `{}`", pname), init.pos);
            }
        }
        Ok(())
    }

    /// A method that comes from anything after `:` (a base class or an interface) must be
    /// `override fn`; interface signatures are checked by `check_conformance`.
    fn check_overrides(&self, name: &str) -> Result<()> {
        let info = &self.classes[name];
        let interfaces = self.all_interfaces(name);
        for meth in info.decl.methods.iter().filter(|m| !m.is_static) {
            let sig = &info.methods[&meth.name];
            let inherited = info.parent.as_ref().and_then(|p| self.find_method(p, &meth.name));
            if inherited.is_none()
                && let Some(iname) =
                    interfaces.iter().find(|i| self.interfaces[*i].methods.iter().any(|(n, _)| n == &meth.name))
            {
                if !meth.is_override && !meth.is_abstract {
                    return err(
                        format!("method `{}` implements `{}.{}`; mark it `override fn`", meth.name, iname, meth.name),
                        meth.pos,
                    );
                }
                continue;
            }
            match (inherited, meth.is_override) {
                (Some((owner, psig)), true) if !psig.same_shape(sig) => {
                    return err(
                        format!("override of `{}` must match `{}.{}{}`", meth.name, owner, meth.name, psig.describe()),
                        meth.pos,
                    );
                }
                (Some((owner, _)), false) => {
                    return err(
                        format!("method `{}` overrides `{}.{}`; mark it `override fn`", meth.name, owner, meth.name),
                        meth.pos,
                    );
                }
                (None, true) => {
                    return err(
                        format!("`{}` is marked override but no base class or interface has this method", meth.name),
                        meth.pos,
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Every interface method (including inherited ones and those of super-interfaces) is
    /// implemented with the same signature, and not `private`.
    fn check_conformance(&self, name: &str) -> Result<()> {
        let info = &self.classes[name];
        for direct in &info.interfaces {
            let dargs = info.interface_args.get(direct).cloned().unwrap_or_default();
            for iname in self.iface_closure(direct) {
                let iargs = self.iface_args(direct, &dargs, &iname).unwrap_or_default();
                let map = self.iface_bindings(&iname, &iargs);
                for (mname, isig) in &self.interfaces[&iname].methods {
                    let isig = &subst_sig(isig, &map);
                    match self.find_method(name, mname) {
                        None => {
                            return err(
                                format!("`{}` does not implement `{}.{}{}`", name, iname, mname, isig.describe()),
                                info.decl.pos,
                            );
                        }
                        Some((_, sig)) if !sig.same_shape(isig) => {
                            return err(
                                format!("`{}.{}` must match `{}.{}{}`", name, mname, iname, mname, isig.describe()),
                                info.decl.pos,
                            );
                        }
                        Some((owner, _)) if self.classes[&owner].private_methods.contains(mname) => {
                            return err(
                                format!("`{}.{}` implements `{}` so it cannot be `private`", owner, mname, iname),
                                info.decl.pos,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

/// Registers every type name (each may be defined once) so declarations can refer to each
/// other in any order.
fn type_names(prog: &Program) -> Result<TypeNames> {
    let mut seen: HashSet<&str> = HashSet::new();
    let decls = prog
        .interfaces
        .iter()
        .map(|i| (&i.name, i.pos))
        .chain(prog.classes.iter().map(|c| (&c.name, c.pos)))
        .chain(prog.extern_types.iter().map(|(n, p)| (n, *p)))
        .chain(prog.structs.iter().map(|s| (&s.name, s.pos)))
        .chain(prog.enums.iter().map(|e| (&e.name, e.pos)));
    for (name, pos) in decls {
        check_type_name(name, pos)?;
        if !seen.insert(name) {
            return err(format!("`{}` is defined more than once", name), pos);
        }
    }
    Ok(TypeNames {
        classes: prog.classes.iter().map(|c| c.name.clone()).collect(),
        ifaces: prog.interfaces.iter().map(|i| i.name.clone()).collect(),
        enums: prog.enums.iter().map(|e| e.name.clone()).collect(),
        structs: prog.structs.iter().map(|s| s.name.clone()).collect(),
        struct_arity: prog
            .structs
            .iter()
            .filter(|s| !s.tparams.is_empty())
            .map(|s| (s.name.clone(), s.tparams.len()))
            .collect(),
        externs: prog.extern_types.iter().map(|(n, _)| n.clone()).collect(),
        generic: prog
            .classes
            .iter()
            .filter(|c| !c.tparams.is_empty())
            .map(|c| (c.name.clone(), c.tparams.len()))
            .collect(),
        enum_arity: prog
            .enums
            .iter()
            .filter(|e| !e.tparams.is_empty())
            .map(|e| (e.name.clone(), e.tparams.len()))
            .collect(),
        iface_arity: prog
            .interfaces
            .iter()
            .filter(|i| !i.tparams.is_empty())
            .map(|i| (i.name.clone(), i.tparams.len()))
            .collect(),
    })
}

/// The bounds written on type parameters: interfaces, or `Show`, `Eq`, `Ord`.
fn resolve_bounds(decls: &[BoundDecl], names: &TypeNames, tparams: &[String]) -> Result<Bounds> {
    let mut out = Bounds::new();
    for d in decls {
        let mut bounds = vec![];
        for (name, args, pos) in &d.names {
            let b = match name.as_str() {
                "Show" | "Eq" | "Ord" if !args.is_empty() => {
                    return err(format!("`{}` takes no type arguments", name), *pos);
                }
                "Show" => Bound::Show,
                "Eq" => Bound::Eq,
                "Ord" => Bound::Ord,
                _ if names.ifaces.contains(name) => {
                    match resolve_type(&TypeExpr::Named(name.clone(), args.clone(), *pos), names, tparams)? {
                        Type::Interface(i, targs) => Bound::Interface(i, targs),
                        _ => unreachable!("an interface name resolves to an interface"),
                    }
                }
                _ => {
                    let what = if names.contains(name) {
                        format!("`{}` is not an interface; a bound is an interface, `Show`, `Eq` or `Ord`", name)
                    } else {
                        format!("unknown bound `{}`: a bound is an interface, `Show`, `Eq` or `Ord`", name)
                    };
                    let known = names.ifaces.iter().map(String::as_str).chain(["Show", "Eq", "Ord"]);
                    return crate::suggest::unknown(what, *pos, name, known);
                }
            };
            if bounds.contains(&b) {
                return err(format!("`{}` is listed twice for `{}`", name, d.param), *pos);
            }
            bounds.push(b);
        }
        out.insert(d.param.clone(), bounds);
    }
    Ok(out)
}

fn struct_info(st: &StructDecl, names: &TypeNames) -> Result<StructInfo> {
    let resolve = |t: &TypeExpr| resolve_type(t, names, &st.tparams);
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
    let methods = struct_methods(st, &fields, &resolve)?;
    let ops = struct_ops(st, &resolve)?;
    let taken = |n: &str| fields.iter().any(|(f, _)| f == n) || methods.contains_key(n);
    let static_values = static_values(&st.name, &st.statics, &st.tparams, &taken, &resolve)?;
    Ok(StructInfo {
        decl: st.clone(),
        tparams: st.tparams.clone(),
        bounds: resolve_bounds(&st.bounds, names, &st.tparams)?,
        fields,
        private_fields: st.fields.iter().filter(|f| f.is_private).map(|f| f.name.clone()).collect(),
        methods,
        ops,
        static_values,
    })
}

fn struct_methods(
    st: &StructDecl,
    fields: &[(String, Type)],
    resolve: &dyn Fn(&TypeExpr) -> Result<Type>,
) -> Result<HashMap<String, StructMethod>> {
    let mut methods = HashMap::new();
    for meth in &st.methods {
        if methods.contains_key(&meth.name) || fields.iter().any(|(n, _)| n == &meth.name) {
            return err(format!("`{}` is defined twice in `{}`", meth.name, st.name), meth.pos);
        }
        if !meth.tparams.is_empty() {
            return err("struct methods cannot be generic (yet)", meth.pos);
        }
        if meth.is_async && !meth.is_static {
            return err("only `static async fn` is supported in structs (yet)", meth.pos);
        }
        let sig = signature(&meth.params, &meth.ret, resolve)?;
        methods.insert(
            meth.name.clone(),
            StructMethod {
                sig,
                is_mut: meth.is_mut,
                is_async: meth.is_async,
                is_static: meth.is_static,
                is_private: meth.is_private,
            },
        );
    }
    Ok(methods)
}

fn struct_ops(st: &StructDecl, resolve: &dyn Fn(&TypeExpr) -> Result<Type>) -> Result<Vec<StructOp>> {
    let mut ops: Vec<StructOp> = Vec::new();
    for (index, o) in st.ops.iter().enumerate() {
        let sig = signature(&o.params, &o.ret, resolve)?;
        let param = match (sig.params.len(), o.op) {
            (0, BinOp::Sub) => None,
            (1, _) => Some(sig.params[0].1.clone()),
            _ => {
                return err(
                    format!(
                        "`op {}` takes one parameter (the right-hand side); only `op -()` may take none",
                        o.op.rust()
                    ),
                    o.pos,
                );
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
    Ok(ops)
}

fn class_info(c: &ClassDecl, names: &TypeNames) -> Result<ClassInfo> {
    let resolve = |t: &TypeExpr| resolve_type(t, names, &c.tparams);
    let (parent, interfaces, interface_args) = class_bases(c, names, &resolve)?;
    let fields = class_fields(c, &resolve)?;
    let mut methods = HashMap::new();
    let mut statics = HashMap::new();
    for meth in &c.methods {
        if methods.contains_key(&meth.name) || statics.contains_key(&meth.name) {
            return err(format!("method `{}` defined twice in `{}`", meth.name, c.name), meth.pos);
        }
        check_method_modifiers(c, meth)?;
        let sig = signature(&meth.params, &meth.ret, &resolve)?;
        if meth.is_static {
            statics.insert(meth.name.clone(), sig);
        } else {
            methods.insert(meth.name.clone(), sig);
        }
    }
    let init = match &c.init {
        Some(i) => Some(signature(&i.params, &TypeExpr::Void, &resolve)?),
        None => None,
    };
    let taken = |n: &str| fields.iter().any(|(f, _)| f == n) || methods.contains_key(n) || statics.contains_key(n);
    let static_values = static_values(&c.name, &c.statics, &c.tparams, &taken, &resolve)?;
    let names_where = |p: fn(&FnDecl) -> bool| c.methods.iter().filter(|m| p(m)).map(|m| m.name.clone()).collect();
    Ok(ClassInfo {
        decl: c.clone(),
        parent,
        interfaces,
        interface_args,
        fields,
        methods,
        statics,
        private_fields: c.fields.iter().filter(|f| f.is_private).map(|f| f.name.clone()).collect(),
        private_methods: names_where(|m| m.is_private),
        abstract_methods: names_where(|m| m.is_abstract),
        async_statics: names_where(|m| m.is_async),
        init,
        children: vec![],
        tparams: c.tparams.clone(),
        bounds: resolve_bounds(&c.bounds, names, &c.tparams)?,
        static_values,
    })
}

/// The base class (at most one) and the interfaces listed after `:`.
/// The base class, the interfaces, and the type arguments of the generic interfaces.
#[allow(clippy::type_complexity)]
fn class_bases(
    c: &ClassDecl,
    names: &TypeNames,
    resolve: &dyn Fn(&TypeExpr) -> Result<Type>,
) -> Result<(Option<String>, Vec<String>, HashMap<String, Vec<Type>>)> {
    for ((b, pos), args) in c.bases.iter().zip(&c.base_args) {
        if names.generic.contains_key(b) || (names.classes.contains(b) && !args.is_empty()) {
            return err(format!("`{}` is generic and cannot be extended (yet)", b), *pos);
        }
    }
    let mut parent = None;
    let mut interfaces = Vec::new();
    let mut interface_args = HashMap::new();
    for ((b, pos), args) in c.bases.iter().zip(&c.base_args) {
        if names.classes.contains(b) {
            if parent.is_some() {
                return err(
                    format!(
                        "`{}` already has a base class; a class can extend only one class (but any number of interfaces)",
                        c.name
                    ),
                    *pos,
                );
            }
            if b == &c.name {
                return err("a class cannot extend itself", *pos);
            }
            parent = Some(b.clone());
        } else if names.ifaces.contains(b) {
            if interfaces.contains(b) {
                return err(format!("interface `{}` listed twice", b), *pos);
            }
            interfaces.push(b.clone());
            if let Type::Interface(_, targs) = resolve(&TypeExpr::Named(b.clone(), args.clone(), *pos))? {
                interface_args.insert(b.clone(), targs);
            }
        } else {
            return err(format!("unknown class or interface `{}`", b), *pos);
        }
    }
    Ok((parent, interfaces, interface_args))
}

fn class_fields(c: &ClassDecl, resolve: &dyn Fn(&TypeExpr) -> Result<Type>) -> Result<Vec<(String, Type)>> {
    let mut fields: Vec<(String, Type)> = Vec::new();
    for f in &c.fields {
        if fields.iter().any(|(n, _)| n == &f.name) {
            return err(format!("field `{}` declared twice", f.name), f.pos);
        }
        let ty = resolve(&f.ty).map_err(|e| at_pos(e, f.pos))?;
        if ty == Type::Void {
            return err("fields cannot have type void", f.pos);
        }
        let needs_default = ty.is_object()
            || matches!(
                ty,
                Type::Enum(..) | Type::Func(..) | Type::Struct(..) | Type::Extern(_) | Type::Result(..) | Type::Task(_)
            );
        if needs_default && f.default.is_none() {
            return err(
                format!("field `{}` of type `{}` needs a default value (or make it optional: `{}?`)", f.name, ty, ty),
                f.pos,
            );
        }
        fields.push((f.name.clone(), ty));
    }
    Ok(fields)
}

fn check_method_modifiers(c: &ClassDecl, meth: &FnDecl) -> Result<()> {
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
    if meth.is_async && !meth.is_static {
        return err(
            "only `static async fn` is supported in classes (yet); use an `async fn` that takes the object",
            meth.pos,
        );
    }
    Ok(())
}

fn check_type_name(name: &str, pos: Span) -> Result<()> {
    if RESERVED_TYPES.contains(&name) || BUILTINS.contains(&name) {
        return err(
            format!(
                "`{}` is a name Kalinga uses for its own types (or the Rust it generates); choose another name",
                name
            ),
            pos,
        );
    }
    if !name.starts_with(|c: char| c.is_uppercase()) {
        return err(format!("type names must start with an uppercase letter: `{}`", name), pos);
    }
    Ok(())
}

fn signature(params: &[Param], ret: &TypeExpr, resolve: &dyn Fn(&TypeExpr) -> Result<Type>) -> Result<Signature> {
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

/// `test fn`s take nothing and return nothing, and have different names (per file).
fn check_tests(prog: &Program) -> Result<()> {
    for (i, t) in prog.tests.iter().enumerate() {
        if !t.params.is_empty() || t.ret != TypeExpr::Void || !t.tparams.is_empty() || t.is_async {
            return err(format!("`test fn {}` must take no parameters and return nothing", t.name), t.pos);
        }
        if prog.tests[..i].iter().any(|o| o.name == t.name && o.pos.file == t.pos.file) {
            return err(format!("there are two tests named `{}`", t.name), t.pos);
        }
    }
    Ok(())
}

/// The `const` and `static var` members of `owner`; `taken` says which names its other
/// members use.
fn static_values(
    owner: &str,
    decls: &[StaticDecl],
    tparams: &[String],
    taken: &dyn Fn(&str) -> bool,
    resolve: &dyn Fn(&TypeExpr) -> Result<Type>,
) -> Result<Vec<(String, StaticInfo)>> {
    let mut out: Vec<(String, StaticInfo)> = Vec::new();
    for d in decls {
        if !tparams.is_empty() {
            return err(format!("generic types cannot have `const` or `static var` members (yet): `{}`", owner), d.pos);
        }
        if taken(&d.name) || out.iter().any(|(n, _)| n == &d.name) {
            return err(format!("`{}` is defined twice in `{}`", d.name, owner), d.pos);
        }
        let ty = resolve(&d.ty).map_err(|e| at_pos(e, d.pos))?;
        out.push((d.name.clone(), StaticInfo { ty, mutable: d.mutable }));
    }
    Ok(out)
}

/// A `const` or `static var`: (owning type, or empty at the top level; name).
type StaticKey = (String, String);

impl Model {
    /// The initial value of a `const` or `static var` must not depend on itself, directly
    /// or through other ones (`const A: int = B + 1` with `const B: int = A`).
    fn check_static_cycles(&self, prog: &Program) -> Result<()> {
        let owned = prog
            .classes
            .iter()
            .map(|c| (c.name.as_str(), &c.statics))
            .chain(prog.structs.iter().map(|s| (s.name.as_str(), &s.statics)))
            .chain(std::iter::once(("", &prog.consts)));
        let mut decls: Vec<(StaticKey, &StaticDecl)> = vec![];
        for (owner, list) in owned {
            decls.extend(list.iter().map(|d| ((owner.to_string(), d.name.clone()), d)));
        }
        let deps: HashMap<StaticKey, Vec<StaticKey>> = decls
            .iter()
            .map(|(key, d)| {
                let mut refs = vec![];
                self.static_refs(&d.value, &mut refs);
                (key.clone(), refs)
            })
            .collect();
        for (key, d) in &decls {
            let mut path = vec![key.clone()];
            if leads_back(&deps, key, &mut path, &mut HashSet::new()) {
                let names: Vec<String> = path.iter().map(show_static).collect();
                return err(
                    format!("`{}` is defined in terms of itself ({})", show_static(key), names.join(" -> ")),
                    d.pos,
                );
            }
        }
        Ok(())
    }

    /// The `const`s and `static var`s that evaluating `e` reads.
    fn static_refs(&self, e: &Expr, out: &mut Vec<StaticKey>) {
        match &e.kind {
            ExprKind::Ident(n) if self.consts.contains_key(n) => out.push((String::new(), n.clone())),
            ExprKind::Field(recv, f) => {
                if let ExprKind::Ident(owner) = &recv.kind
                    && self.static_value(owner, f).is_some()
                {
                    out.push((owner.clone(), f.clone()));
                }
            }
            _ => {}
        }
        for c in e.children() {
            self.static_refs(c, out);
        }
    }
}

/// Does following `deps` from the last key of `path` reach `path[0]` again? On `true`,
/// `path` ends with it.
fn leads_back(
    deps: &HashMap<StaticKey, Vec<StaticKey>>,
    start: &StaticKey,
    path: &mut Vec<StaticKey>,
    seen: &mut HashSet<StaticKey>,
) -> bool {
    let Some(at) = path.last().cloned() else { return false };
    for next in deps.get(&at).into_iter().flatten() {
        path.push(next.clone());
        if next == start || (seen.insert(next.clone()) && leads_back(deps, start, path, seen)) {
            return true;
        }
        path.pop();
    }
    false
}

fn show_static((owner, name): &StaticKey) -> String {
    if owner.is_empty() { name.clone() } else { format!("{}.{}", owner, name) }
}
