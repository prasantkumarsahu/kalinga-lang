//! Top-level items: interfaces, enums, structs, classes, functions and `main`.

use std::collections::HashMap;
use std::fmt::Write as _;

use kalinga_sema::tast::TBody;
use kalinga_sema::{Signature, Type, class_trait};
use kalinga_syntax::ast::Program;

use crate::{Gen, HEADER, RUNTIME, clone_if, fn_id, rid};

/// Generated method bodies by (class, method); static methods are keyed `static name`.
type Bodies = HashMap<(String, String), String>;

impl Gen<'_> {
    pub(crate) fn program(&self, prog: &Program) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push_str(RUNTIME);
        for (path, _) in &prog.rust_files {
            let module = format!("__rs_{}", rust_module_name(path));
            let _ = writeln!(out, "\n#[path = {:?}]\nmod {};\nuse {}::*;", path, module, module);
        }
        for x in prog.extern_fns.iter() {
            let Some(body) = &x.body else { continue };
            let (sig, _) = &self.m.extern_fns[&x.name];
            let _ = writeln!(
                out,
                "\n// extern fn {} (inline Rust)\nfn __x_{}({}){} {{{}}}",
                x.name,
                x.name,
                self.params_decl(sig),
                self.ret_suffix(&sig.ret),
                body
            );
        }
        for iname in &self.m.interface_order {
            self.interface_decl(&mut out, iname);
        }
        for ename in &self.m.enum_order {
            self.enum_decl(&mut out, ename);
        }
        for name in &self.m.struct_order {
            self.struct_decl(&mut out, name);
        }
        let (methods, inits, defaults) = self.class_bodies();
        for cname in &self.m.class_order {
            self.class(&mut out, cname, &methods, &inits, &defaults);
        }
        for f in &prog.functions {
            let Some(b) = self.t.functions.get(&f.name) else { continue };
            let _ = writeln!(
                out,
                "\nfn {}{}({}){} {{\n{}}}",
                fn_id(&f.name),
                generic_params(&f.tparams),
                self.params_decl(&b.sig),
                self.fn_ret(&b.sig.ret, b.is_async),
                self.body(b, 1)
            );
        }
        self.main_fn(&mut out, prog.main_thread);
        out
    }

    /// The statements of a function or method body, wrapped for `async fn`s.
    fn body(&self, b: &TBody, depth: usize) -> String {
        let body = self.block_inner(&b.stmts, depth);
        if b.is_async {
            format!("{}__Async::new(async move {{\n{}{}}})\n", crate::indent(depth), body, crate::indent(depth))
        } else {
            body
        }
    }

    fn main_fn(&self, out: &mut String, main_thread: bool) {
        let run = if main_thread { "__kl_run_main" } else { "__kl_run" };
        let main_async = self.m.async_fns.contains("main");
        let call_main =
            if main_async { format!("__block_on({}())", fn_id("main")) } else { format!("{}()", fn_id("main")) };
        if matches!(self.m.functions.get("main").map(|s| &s.ret), Some(Type::Result(..))) {
            let _ = writeln!(
                out,
                "\nfn __kl_main_result() {{\n    if let Err(e) = {} {{\n        eprintln!(\"error: {{}}\", __KlShow::__show(&e));\n        std::process::exit(1);\n    }}\n}}",
                call_main
            );
            let _ = writeln!(out, "\nfn main() {{\n    {}(__kl_main_result);\n}}", run);
        } else if main_async {
            let _ = writeln!(
                out,
                "\nfn __kl_main_async() {{\n    {};\n}}\n\nfn main() {{\n    {}(__kl_main_async);\n}}",
                call_main, run
            );
        } else {
            let _ = writeln!(out, "\nfn main() {{\n    {}({});\n}}", run, fn_id("main"));
        }
    }

    fn interface_decl(&self, out: &mut String, iname: &str) {
        let info = &self.m.interfaces[iname];
        let _ = writeln!(out, "\n// interface {}", iname);
        let bound = if info.supers.is_empty() { String::new() } else { format!(": {}", info.supers.join(" + ")) };
        let _ = writeln!(out, "trait {}{} {{", iname, bound);
        for (mname, sig) in &info.methods {
            let _ = writeln!(
                out,
                "    fn {}(&mut self{}){};",
                rid(mname),
                self.comma_params(sig),
                self.ret_suffix(&sig.ret)
            );
        }
        let targets = self.m.class_order.iter().filter(|c| self.m.all_interfaces(c).contains(&iname.to_string()));
        for target in targets.filter(|c| self.is_cast_target(c)) {
            let _ = writeln!(
                out,
                "    fn __as_{}(&self) -> Option<{}> {{ None }}",
                target,
                self.rty(&Type::Class(target.clone()))
            );
        }
        out.push_str("}\n");
    }

    fn enum_decl(&self, out: &mut String, name: &str) {
        let info = &self.m.enums[name];
        let plain = self.m.plain_enums.contains(name);
        let _ = writeln!(out, "\n// enum {}", name);
        let _ = writeln!(out, "#[derive(Clone{})]", if plain { ", PartialEq" } else { "" });
        let _ = writeln!(out, "enum {} {{", name);
        for (v, fields) in &info.variants {
            if fields.is_empty() {
                let _ = writeln!(out, "    {},", v);
            } else {
                let ts: Vec<String> = fields.iter().map(|(_, t)| self.rty(t)).collect();
                let _ = writeln!(out, "    {}({}),", v, ts.join(", "));
            }
        }
        out.push_str("}\n");
        if !plain {
            return;
        }
        let _ =
            writeln!(out, "impl __KlShow for {} {{\n    fn __show(&self) -> String {{\n        match self {{", name);
        for (v, fields) in &info.variants {
            if fields.is_empty() {
                let _ = writeln!(out, "            {}::{} => String::from({:?}),", name, v, v);
            } else {
                let binds: Vec<String> = (0..fields.len()).map(|i| format!("__f{}", i)).collect();
                let shows: Vec<String> = binds.iter().map(|b| format!("{}.__show()", b)).collect();
                let holes = vec!["{}"; fields.len()].join(", ");
                let _ = writeln!(
                    out,
                    "            {}::{}({}) => format!(\"{}({})\", {}),",
                    name,
                    v,
                    binds.join(", "),
                    v,
                    holes,
                    shows.join(", ")
                );
            }
        }
        out.push_str("        }\n    }\n}\n");
    }

    /// A struct: definition, methods, constants, operators and printing.
    fn struct_decl(&self, out: &mut String, name: &str) {
        let info = &self.m.structs[name];
        let plain = self.m.plain_structs.contains(name);
        let self_ty =
            if info.tparams.is_empty() { name.to_string() } else { format!("{}<{}>", name, info.tparams.join(", ")) };
        let _ = writeln!(out, "\n// ===== struct {} =====", name);
        let _ = writeln!(out, "#[derive(Clone{})]", if plain { ", PartialEq" } else { "" });
        let _ = writeln!(out, "struct {} {{", self_ty);
        for (f, t) in &info.fields {
            let _ = writeln!(out, "    {}: {},", rid(f), self.rty(t));
        }
        out.push_str("}\n");
        let _ = writeln!(out, "impl{} {} {{", generic_params(&info.tparams), self_ty);
        for meth in &info.decl.methods {
            let (m, Some(b)) =
                (&info.methods[&meth.name], self.t.struct_methods.get(&(name.to_string(), meth.name.clone())))
            else {
                continue;
            };
            let body = self.body(b, 2);
            if m.is_static {
                let _ = writeln!(
                    out,
                    "    fn __s_{}({}){} {{\n{}    }}",
                    meth.name,
                    self.params_decl(&m.sig),
                    self.fn_ret(&m.sig.ret, m.is_async),
                    body
                );
            } else {
                let recv = if m.is_mut { "&mut self" } else { "&self" };
                let _ = writeln!(
                    out,
                    "    fn {}({}{}){} {{\n{}    }}",
                    rid(&meth.name),
                    recv,
                    self.comma_params(&m.sig),
                    self.ret_suffix(&m.sig.ret),
                    body
                );
            }
        }
        for (c, t) in &info.consts {
            let Some(v) = self.t.struct_consts.get(&(name.to_string(), c.clone())) else { continue };
            let _ = writeln!(out, "    fn __c_{}() -> {} {{ {} }}", c, self.rty(t), self.expr(v));
        }
        for o in &info.ops {
            let Some(b) = self.t.struct_ops.get(&(name.to_string(), o.index)) else { continue };
            let _ = writeln!(
                out,
                "    fn __op_{}(&self{}){} {{\n{}    }}",
                o.index,
                self.comma_params(&b.sig),
                self.ret_suffix(&o.ret),
                self.body(b, 2)
            );
        }
        out.push_str("}\n");
        if plain {
            self.struct_show(out, name, &self_ty);
        }
    }

    fn struct_show(&self, out: &mut String, name: &str, self_ty: &str) {
        let info = &self.m.structs[name];
        let parts: Vec<String> = info.fields.iter().map(|(f, _)| format!("{}: {{}}", f)).collect();
        let shows: Vec<String> = info.fields.iter().map(|(f, _)| format!("self.{}.__show()", rid(f))).collect();
        let bounds = if info.tparams.is_empty() {
            String::new()
        } else {
            let b: Vec<String> = info.tparams.iter().map(|t| format!("{}: Clone + 'static + __KlShow", t)).collect();
            format!("<{}>", b.join(", "))
        };
        let _ = writeln!(
            out,
            "impl{} __KlShow for {} {{\n    fn __show(&self) -> String {{ format!(\"{}({})\"{}{}) }}\n}}",
            bounds,
            self_ty,
            name,
            parts.join(", "),
            if shows.is_empty() { "" } else { ", " },
            shows.join(", ")
        );
    }

    /// Method, init and field-default code for every class. Bodies are generated once per
    /// defining class and pasted into every subclass.
    fn class_bodies(&self) -> (Bodies, HashMap<String, String>, Bodies) {
        let mut methods = Bodies::new();
        let mut inits = HashMap::new();
        let mut defaults = Bodies::new();
        for cname in &self.m.class_order {
            let info = &self.m.classes[cname];
            for (f, t) in &info.fields {
                let key = (cname.clone(), f.clone());
                let code = match self.t.field_defaults.get(&key) {
                    Some(e) => self.expr(e),
                    None => default_value(t),
                };
                defaults.insert(key, code);
            }
            for meth in &info.decl.methods {
                let key = (cname.clone(), meth.name.clone());
                let code = match (meth.is_static, self.t.methods.get(&key), self.t.statics.get(&key)) {
                    (true, _, Some(b)) => self.body(b, 2),
                    (false, Some(Some(b)), _) => self.body(b, 2),
                    _ => "        unreachable!(\"abstract method\")\n".to_string(),
                };
                let key = if meth.is_static { (cname.clone(), format!("static {}", meth.name)) } else { key };
                methods.insert(key, code);
            }
            if let Some(b) = self.t.inits.get(cname) {
                inits.insert(cname.clone(), self.body(b, 2));
            }
        }
        (methods, inits, defaults)
    }

    fn is_cast_target(&self, class: &str) -> bool {
        self.t.cast_targets.contains(class)
    }

    /// Objects of `class` store a weak pointer to themselves only when something needs it:
    /// a method (here or inherited) that uses `self` as a value, or `as`/`is` to this class
    /// or one of its bases.
    fn needs_self_ptr(&self, class: &str) -> bool {
        self.m.ancestors(class).iter().any(|a| self.t.self_values.contains(a) || self.is_cast_target(a))
    }

    /// Classes sharing `root` as their topmost base class (including `root`).
    fn hierarchy(&self, root: &str) -> Vec<String> {
        self.m
            .class_order
            .iter()
            .filter(|c| self.m.ancestors(c).last().map(String::as_str) == Some(root))
            .cloned()
            .collect()
    }

    /// `fn __as_T(&self)` that succeeds, for an object whose class is `T` or derives from it.
    fn downcast_hook(&self, out: &mut String, target: &str) {
        let ty = self.rty(&Type::Class(target.to_string()));
        let _ = writeln!(
            out,
            "    fn __as_{}(&self) -> Option<{}> {{ self.__self.upgrade().map(|__r| __r as {}) }}",
            target, ty, ty
        );
    }

    fn class(
        &self,
        out: &mut String,
        cname: &str,
        methods: &Bodies,
        inits: &HashMap<String, String>,
        defaults: &Bodies,
    ) {
        let info = &self.m.classes[cname];
        let generic = !info.tparams.is_empty();
        let self_ty = if generic { format!("{}<{}>", cname, info.tparams.join(", ")) } else { cname.to_string() };
        let self_ptr = self.needs_self_ptr(cname);
        let _ = writeln!(out, "\n// ===== class {} =====", cname);
        let _ = writeln!(out, "struct {} {{", self_ty);
        if self_ptr {
            let _ = writeln!(out, "    __self: std::rc::Weak<RefCell<{}>>,", self_ty);
        }
        for (f, t, _) in &self.m.all_fields(cname) {
            let rt = if matches!(t, Type::Param(_)) { format!("Option<{}>", self.rty(t)) } else { self.rty(t) };
            let _ = writeln!(out, "    {}: {},", rid(f), rt);
        }
        out.push_str("}\n");
        if generic {
            let _ = writeln!(out, "impl{} {} {{", generic_params(&info.tparams), self_ty);
        } else {
            self.class_trait_decl(out, cname);
            let _ = writeln!(out, "impl {} {{", cname);
        }
        self.class_constructor(out, cname, &self_ty, self_ptr, defaults);
        self.class_methods(out, cname, methods, inits);
        out.push_str("}\n");
        if !generic {
            self.class_trait_impls(out, cname);
        }
    }

    /// The class's own Rust trait, for dynamic dispatch.
    fn class_trait_decl(&self, out: &mut String, cname: &str) {
        let info = &self.m.classes[cname];
        let mut supers: Vec<String> = info.parent.iter().map(|p| class_trait(p)).collect();
        supers.extend(info.interfaces.iter().cloned());
        let bound = if supers.is_empty() { String::new() } else { format!(": {}", supers.join(" + ")) };
        let _ = writeln!(out, "trait {}{} {{", class_trait(cname), bound);
        for (mname, sig) in self.m.new_trait_methods(cname) {
            let _ = writeln!(
                out,
                "    fn {}(&mut self{}){};",
                rid(&mname),
                self.comma_params(&sig),
                self.ret_suffix(&sig.ret)
            );
        }
        for (f, t) in &info.fields {
            let _ = writeln!(out, "    fn __get_{}(&self) -> {};", f, self.rty(t));
            let _ = writeln!(out, "    fn __set_{}(&mut self, v: {});", f, self.rty(t));
        }
        if info.parent.is_none() {
            for target in self.hierarchy(cname).iter().filter(|t| self.is_cast_target(t)) {
                let _ = writeln!(
                    out,
                    "    fn __as_{}(&self) -> Option<{}> {{ None }}",
                    target,
                    self.rty(&Type::Class(target.clone()))
                );
            }
        }
        out.push_str("}\n");
    }

    fn class_constructor(&self, out: &mut String, cname: &str, self_ty: &str, self_ptr: bool, defaults: &Bodies) {
        let ctor = self.m.find_init(cname);
        let ctor_params = ctor.as_ref().map(|(_, s)| self.params_decl(s)).unwrap_or_default();
        let _ = writeln!(out, "    fn __new({}) -> Rc<RefCell<{}>> {{", ctor_params, self_ty);
        out.push_str(if self_ptr { "        Rc::new_cyclic(|__w| {\n" } else { "        Rc::new({\n" });
        let _ = writeln!(out, "            let mut __o = {} {{", cname);
        if self_ptr {
            out.push_str("                __self: __w.clone(),\n");
        }
        for (f, _, owner) in &self.m.all_fields(cname) {
            let default = defaults.get(&(owner.clone(), f.clone())).map_or("Default::default()", String::as_str);
            let _ = writeln!(out, "                {}: {},", rid(f), default);
        }
        out.push_str("            };\n");
        if let Some((owner, sig)) = &ctor {
            let _ = writeln!(out, "            __o.__{}__init({});", owner, self.params_names(sig));
        }
        out.push_str("            RefCell::new(__o)\n        })\n    }\n");
    }

    /// Every ancestor's methods under their own names, statics, and virtual methods that
    /// resolve to the most-derived body.
    fn class_methods(&self, out: &mut String, cname: &str, methods: &Bodies, inits: &HashMap<String, String>) {
        let info = &self.m.classes[cname];
        for a in &self.m.ancestors(cname) {
            let ainfo = &self.m.classes[a];
            if let (Some(sig), Some(body)) = (&ainfo.init, inits.get(a)) {
                let _ = writeln!(out, "    fn __{}__init(&mut self{}) {{\n{}    }}", a, self.comma_params(sig), body);
            }
            let mut names: Vec<&String> = ainfo.methods.keys().collect();
            names.sort();
            for mname in names {
                let sig = &ainfo.methods[mname];
                let body = methods.get(&(a.clone(), mname.clone())).map_or("", String::as_str);
                let _ = writeln!(
                    out,
                    "    fn __{}__{}(&mut self{}){} {{\n{}    }}",
                    a,
                    mname,
                    self.comma_params(sig),
                    self.ret_suffix(&sig.ret),
                    body
                );
            }
        }
        let mut statics: Vec<(&String, &Signature)> = info.statics.iter().collect();
        statics.sort_by(|a, b| a.0.cmp(b.0));
        for (mname, sig) in statics {
            let body = methods.get(&(cname.to_string(), format!("static {}", mname))).map_or("", String::as_str);
            let _ = writeln!(
                out,
                "    fn __s_{}({}){} {{\n{}    }}",
                mname,
                self.params_decl(sig),
                self.fn_ret(&sig.ret, info.async_statics.contains(mname)),
                body
            );
        }
        for (mname, owner, sig) in self.m.method_table(cname) {
            let _ = writeln!(
                out,
                "    fn {}(&mut self{}){} {{ self.__{}__{}({}) }}",
                rid(&mname),
                self.comma_params(&sig),
                self.ret_suffix(&sig.ret),
                owner,
                mname,
                self.params_names(&sig)
            );
        }
    }

    /// The traits of every ancestor and every interface, forwarding to the methods.
    fn class_trait_impls(&self, out: &mut String, cname: &str) {
        let ancestors = self.m.ancestors(cname);
        let forward = |out: &mut String, mname: &str, sig: &Signature| {
            let names = self.params_names(sig);
            let args = if names.is_empty() { String::new() } else { format!(", {}", names) };
            let _ = writeln!(
                out,
                "    fn {}(&mut self{}){} {{ {}::{}(self{}) }}",
                rid(mname),
                self.comma_params(sig),
                self.ret_suffix(&sig.ret),
                cname,
                rid(mname),
                args
            );
        };
        for a in &ancestors {
            let _ = writeln!(out, "impl {} for {} {{", class_trait(a), cname);
            for (mname, sig) in self.m.new_trait_methods(a) {
                forward(out, &mname, &sig);
            }
            for (f, t) in &self.m.classes[a].fields {
                let get = clone_if(format!("self.{}", rid(f)), t);
                let _ = writeln!(out, "    fn __get_{}(&self) -> {} {{ {} }}", f, self.rty(t), get);
                let _ = writeln!(out, "    fn __set_{}(&mut self, v: {}) {{ self.{} = v; }}", f, self.rty(t), rid(f));
            }
            if self.m.classes[a].parent.is_none() {
                for target in ancestors.iter().filter(|t| self.is_cast_target(t)) {
                    self.downcast_hook(out, target);
                }
            }
            out.push_str("}\n");
        }
        for iname in self.m.all_interfaces(cname) {
            let _ = writeln!(out, "impl {} for {} {{", iname, cname);
            for (mname, sig) in &self.m.interfaces[&iname].methods {
                forward(out, mname, sig);
            }
            for target in
                ancestors.iter().filter(|t| self.m.all_interfaces(t).contains(&iname) && self.is_cast_target(t))
            {
                self.downcast_hook(out, target);
            }
            out.push_str("}\n");
        }
    }
}

/// The value a field starts with when it declares no default.
fn default_value(t: &Type) -> String {
    match t {
        Type::Int => "0i64".into(),
        Type::Float => "0.0f64".into(),
        Type::Bool => "false".into(),
        Type::Str => "String::new()".into(),
        Type::Array(_) => "Vec::new()".into(),
        Type::Map(..) => "BTreeMap::new()".into(),
        // Optionals, and a generic class's `T` fields (stored as `Option<T>`).
        _ => "None".into(),
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
