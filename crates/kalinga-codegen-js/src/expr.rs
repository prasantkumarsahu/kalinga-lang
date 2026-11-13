//! Expressions, calls and built-in operations.

use kalinga_sema::Type;
use kalinga_sema::tast::*;
use kalinga_syntax::ast::BinOp;

use crate::{Gen, clone_if, fn_id, jid, js_str, method_id, type_id};

/// Element types whose values compare with JavaScript's `===` / `includes`.
fn is_primitive(t: &Type) -> bool {
    matches!(t, Type::Int | Type::Float | Type::Bool | Type::Str)
}

impl Gen<'_> {
    pub(crate) fn expr(&self, e: &TExpr) -> String {
        match &e.kind {
            TExprKind::Int(v) => v.to_string(),
            TExprKind::Float(v) => float_literal(*v),
            TExprKind::Str(s) => js_str(s),
            TExprKind::Bool(b) => b.to_string(),
            TExprKind::None => "null".into(),
            TExprKind::Local(name) | TExprKind::SharedLocal(name) => clone_if(jid(name), &e.ty),
            TExprKind::FnRef(name) => fn_id(name),
            TExprKind::StructSelf => "$kl.clone(this)".into(),
            TExprKind::ObjectSelf => "this".into(),
            // Upcasts, optional wrapping and element-wise conversions change nothing here.
            TExprKind::Convert(inner) => self.expr(inner),
            TExprKind::Array(items) | TExprKind::Tuple(items) => format!("[{}]", self.exprs(items)),
            TExprKind::TupleField { recv, index } => clone_if(format!("{}[{}]", self.recv(recv), index), &e.ty),
            TExprKind::Map(entries) => {
                let parts: Vec<String> =
                    entries.iter().map(|(k, v)| format!("[{}, {}]", self.expr(k), self.expr(v))).collect();
                format!("$kl.map([{}])", parts.join(", "))
            }
            TExprKind::Interp(parts) => self.interp(parts),
            TExprKind::Neg(v) => format!("(-{})", self.expr(v)),
            TExprKind::Not(v) => format!("(!{})", self.expr(v)),
            TExprKind::Binary(op, l, r) => self.binary(*op, l, r),
            TExprKind::IsNone { value, negated } => {
                format!("({} {} null)", self.expr(value), if *negated { "!==" } else { "===" })
            }
            TExprKind::StructOp { index, lhs, rhs, swapped } => match rhs {
                None => format!("({}).op${}()", self.expr(lhs), index),
                Some(r) if *swapped => {
                    format!("(($l, $r) => $r.op${}($l))({}, {})", index, self.expr(lhs), self.expr(r))
                }
                Some(r) => format!("({}).op${}({})", self.expr(lhs), index, self.expr(r)),
            },
            TExprKind::Downcast { value, target, via, test_only } => {
                let code = self.expr(value);
                match (via, test_only) {
                    (Cast::Same, false) => code,
                    (Cast::Same, true) => format!("({}, true)", code),
                    (Cast::Hook(_), true) => format!("({} instanceof {})", code, type_id(target)),
                    (Cast::Hook(_), false) => {
                        format!("(($v) => $v instanceof {} ? $v : null)({})", type_id(target), code)
                    }
                }
            }
            TExprKind::Lambda(l) => self.lambda(l),
            TExprKind::If { binding, cond, then, otherwise } => {
                self.if_value(binding.as_deref(), cond, then, otherwise)
            }
            TExprKind::Match { value, arms } => self.match_value(value, arms),
            TExprKind::Await(v) => match v.ty {
                Type::Task(_) => "$kl.unsupported(\"await on a thread's Task\")".into(),
                _ => format!("(await $kl.run({}))", self.expr(v)),
            },
            TExprKind::Try(v) => match v.ty {
                Type::Optional(_) => format!("$kl.tryO({})", self.expr(v)),
                _ => format!("$kl.tryR({})", self.expr(v)),
            },
            TExprKind::Field { recv, field } => {
                let read = format!("{}.{}", self.recv(recv), field);
                let read = match &recv.ty {
                    Type::Generic(c, _) if self.m.is_wrapped_field(c, field) => {
                        format!("$kl.unset({}, {})", read, js_str(field))
                    }
                    _ => read,
                };
                clone_if(read, &e.ty)
            }
            TExprKind::SelfField { field, wrapped: true } => {
                clone_if(format!("$kl.unset(this.{}, {})", field, js_str(field)), &e.ty)
            }
            TExprKind::SelfField { field, .. } => clone_if(format!("this.{}", field), &e.ty),
            TExprKind::Static { owner, name, .. } if owner.is_empty() => format!("k${}()", name),
            TExprKind::Static { owner, name, mutable: true } => clone_if(format!("s${}${}", owner, name), &e.ty),
            TExprKind::Static { owner, name, mutable: false } => format!("c${}${}()", owner, name),
            TExprKind::MathConst(name) => match name.as_str() {
                "PI" => "Math.PI".into(),
                "TAU" => "(2 * Math.PI)".into(),
                "E" => "Math.E".into(),
                _ => "Infinity".into(),
            },
            TExprKind::Variant { variant, args, .. } => {
                format!("new $kl.E({}, [{}])", js_str(variant), self.exprs(args))
            }
            TExprKind::Index { base, index } => match &base.ty {
                Type::Map(k, _) => {
                    format!("$kl.mget({}, {}, {})", self.recv(base), self.expr(index), self.shower(k))
                }
                Type::Str => format!("$kl.charAt({}, {})", self.recv(base), self.expr(index)),
                _ => format!("$kl.at({}, {})", self.recv(base), self.expr(index)),
            },
            TExprKind::Slice { base, from, to } => {
                let bound = |b: &Option<Box<TExpr>>| b.as_deref().map_or("null".into(), |x| self.expr(x));
                let f = if base.ty == Type::Str { "$kl.sliceS" } else { "$kl.sliceA" };
                format!("{}({}, {}, {})", f, self.recv(base), bound(from), bound(to))
            }
            TExprKind::Call { callee, args } => self.call(callee, args),
            TExprKind::Builtin { op, args } => self.builtin(op, args, e),
            TExprKind::BuiltinMethod { op, recv, args } => self.builtin_method(op, recv, args),
            TExprKind::Mutate { place, op, args } => self.mutate(place, op, args),
        }
    }

    pub(crate) fn exprs(&self, es: &[TExpr]) -> String {
        es.iter().map(|e| self.expr(e)).collect::<Vec<_>>().join(", ")
    }

    /// A value read without copying it, for reading a part of it or calling a method that
    /// does not change it: variables, `self.field` and struct fields or array elements of
    /// those are read in place.
    pub(crate) fn recv(&self, e: &TExpr) -> String {
        match &e.kind {
            TExprKind::Local(name) | TExprKind::SharedLocal(name) => jid(name),
            TExprKind::SelfField { field, wrapped: false } => format!("this.{}", field),
            TExprKind::StructSelf => "this".into(),
            TExprKind::Field { recv, field } if matches!(recv.ty, Type::Struct(..)) => {
                format!("{}.{}", self.recv(recv), field)
            }
            TExprKind::Index { base, index } if matches!(base.ty, Type::Array(_)) => {
                format!("$kl.aref({}, {})", self.recv(base), self.expr(index))
            }
            _ => format!("({})", self.expr(e)),
        }
    }

    fn interp(&self, parts: &[TInterp]) -> String {
        let pieces: Vec<String> = parts
            .iter()
            .map(|p| match p {
                TInterp::Lit(s) => js_str(s),
                TInterp::Expr(x) => self.show(&self.expr(x), &x.ty),
            })
            .collect();
        match pieces.len() {
            0 => "\"\"".into(),
            1 if matches!(parts[0], TInterp::Lit(_)) => pieces[0].clone(),
            _ => format!("(\"\" + {})", pieces.join(" + ")),
        }
    }

    fn binary(&self, op: BinOp, l: &TExpr, r: &TExpr) -> String {
        let (a, b) = (self.expr(l), self.expr(r));
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => arith(op, &l.ty, &a, &b),
            BinOp::Eq | BinOp::Ne => {
                let none = matches!(l.ty, Type::NoneLit) || matches!(r.ty, Type::NoneLit);
                let test = if none || (is_primitive(&l.ty) && is_primitive(&r.ty)) {
                    format!("({} === {})", a, b)
                } else {
                    format!("$kl.eq({}, {})", a, b)
                };
                if op == BinOp::Ne { format!("(!{})", test) } else { test }
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let sym = op.rust();
                if is_primitive(&l.ty) {
                    format!("({} {} {})", a, sym, b)
                } else {
                    format!("($kl.cmp({}, {}) {} 0)", a, b, sym)
                }
            }
            BinOp::And => format!("({} && {})", a, b),
            BinOp::Or => format!("({} || {})", a, b),
        }
    }

    fn lambda(&self, l: &TLambda) -> String {
        let was_async = self.in_async.replace(false);
        // Each lambda gets its own copy of the outer variables it uses, except a `var` that
        // a lambda changes, which is shared.
        let copies: Vec<(String, String)> = l
            .captures
            .iter()
            .filter(|(c, _)| c != "self" && !self.shared.borrow().contains(c))
            .map(|(c, t)| (jid(c), clone_if(jid(c), t)))
            .collect();
        let params: Vec<String> = l.params.iter().map(|(n, _)| jid(n)).collect();
        let body = match &l.body {
            TLambdaBody::Expr(e) if e.ty == Type::Void => format!("{{ {}; }}", self.expr(e)),
            TLambdaBody::Expr(e) => {
                let v = self.expr(e);
                if v.contains("$kl.try") { format!("{{ return {}; }}", v) } else { format!("({})", v) }
            }
            TLambdaBody::Block(stmts) => format!("{{\n{}}}", self.block_inner(stmts, 2)),
        };
        let body = if body.starts_with('{') { self.catch_try(body) } else { body };
        self.in_async.set(was_async);
        let arrow = format!("({}) => {}", params.join(", "), body);
        if copies.is_empty() {
            format!("({})", arrow)
        } else {
            let (names, values): (Vec<String>, Vec<String>) = copies.into_iter().unzip();
            format!("(({}) => {})({})", names.join(", "), arrow, values.join(", "))
        }
    }

    /// `body` (in braces), catching the early return of `value?` when it uses one.
    pub(crate) fn catch_try(&self, body: String) -> String {
        if !body.contains("$kl.try") {
            return body;
        }
        format!("{{ try {} catch ($e) {{ if ($e instanceof $kl.Ret) return $e.v; throw $e; }} }}", body)
    }

    /// An expression that needs statements: an arrow function called at once (awaited in
    /// async code, so `await` inside it keeps working).
    pub(crate) fn iife(&self, params: &str, body: &str, args: &str) -> String {
        if self.in_async.get() {
            format!("(await (async ({}) => {{\n{}}})({}))", params, body, args)
        } else {
            format!("(({}) => {{\n{}}})({})", params, body, args)
        }
    }

    /// A value block as the statements of a function that returns its value.
    pub(crate) fn value_body(&self, b: &TValueBlock) -> String {
        let stmts = self.block_inner(&b.stmts, 2);
        let value = self.expr(&b.value);
        format!("{}    return {};\n", stmts, value)
    }

    fn if_value(&self, binding: Option<&str>, cond: &TExpr, then: &TValueBlock, otherwise: &TValueBlock) -> String {
        let simple = then.stmts.is_empty() && otherwise.stmts.is_empty();
        match binding {
            None if simple => {
                format!("({} ? {} : {})", self.expr(cond), self.expr(&then.value), self.expr(&otherwise.value))
            }
            None => {
                let body = format!(
                    "  if ({}) {{\n{}  }}\n{}",
                    self.expr(cond),
                    self.value_body(then),
                    self.value_body(otherwise)
                );
                self.iife("", &body, "")
            }
            Some(name) => {
                let t = self.temp();
                let value = self.expr(cond);
                let bound = jid(name);
                let then_body = self.value_body(then);
                let body = format!(
                    "  if ({t} !== null) {{\n    const {bound} = {t};\n{then_body}  }}\n{}",
                    self.value_body(otherwise)
                );
                self.iife(&t, &body, &value)
            }
        }
    }

    fn match_value(&self, value: &TExpr, arms: &[(TPattern, TValueBlock)]) -> String {
        let m = self.temp();
        let scrutinee = self.expr(value);
        let mut body = String::new();
        for (pattern, block) in arms {
            let (test, binds) = self.pattern(&m, pattern);
            let inner = format!("{}{}", binds, self.value_body(block));
            match test {
                Some(t) => body += &format!("  if ({}) {{\n{}  }}\n", t, inner),
                None => body += &format!("  {{\n{}  }}\n", inner),
            }
        }
        self.iife(&m, &body, &scrutinee)
    }

    /// The test for a `match` pattern on the value in `m` (none for `_`), and the `const`
    /// lines binding its fields.
    pub(crate) fn pattern(&self, m: &str, pattern: &TPattern) -> (Option<String>, String) {
        match pattern {
            TPattern::Wildcard => (None, String::new()),
            TPattern::Int(n) => (Some(format!("{} === {}", m, n)), String::new()),
            TPattern::Str(s) => (Some(format!("{} === {}", m, js_str(s))), String::new()),
            TPattern::Bool(b) => (Some(format!("{} === {}", m, b)), String::new()),
            TPattern::Variant { variant, binds, .. } => {
                let mut lines = String::new();
                for (i, b) in binds.iter().enumerate() {
                    if let Some(b) = b {
                        lines += &format!("    const {} = {}.f[{}];\n", jid(b), m, i);
                    }
                }
                (Some(format!("{}.t === {}", m, js_str(variant))), lines)
            }
        }
    }

    fn call(&self, callee: &Callee, args: &[TExpr]) -> String {
        let a = self.exprs(args);
        match callee {
            Callee::Function { name, .. } => format!("{}({})", fn_id(name), a),
            Callee::Extern { path } => match (path.strip_prefix("__x_"), std_glue(path)) {
                (Some(name), _) => format!("x${}({})", name, a),
                (None, Some(js)) => js.to_string(),
                (None, None) => format!("$kl.unsupported({})", js_str(&format!("the Rust function `{}`", path))),
            },
            Callee::Value(f) => format!("{}({})", self.recv(f), a),
            Callee::New { class, .. } => format!("{}.$new({})", type_id(class), a),
            Callee::StructNew { strukt, .. } => format!("new {}({})", type_id(strukt), a),
            Callee::Static { owner, name, .. } => format!("{}.{}({})", type_id(owner), method_id(name), a),
            Callee::SelfMethod(name) => format!("this.{}({})", method_id(name), a),
            Callee::SuperMethod { owner, name } => {
                let sep = if a.is_empty() { "" } else { ", " };
                format!("{}.prototype.{}.call(this{}{})", type_id(owner), method_id(name), sep, a)
            }
            Callee::Method { recv, name, .. } | Callee::StructMethod { recv, name } => {
                format!("{}.{}({})", self.recv(recv), method_id(name), a)
            }
            Callee::Ok if args.is_empty() => "$kl.ok(undefined)".into(),
            Callee::Ok => format!("$kl.ok({})", a),
            Callee::Err => format!("$kl.err({})", a),
        }
    }

    fn mutate(&self, place: &TPlace, op: &Mutation, args: &[TExpr]) -> String {
        let p = self.read_place(place);
        let arg = |i: usize| self.expr(&args[i]);
        match op {
            Mutation::Push => format!("{}.push({})", p, arg(0)),
            Mutation::Pop => format!("$kl.pop({})", p),
            Mutation::Sort => {
                let cmp = match place.ty() {
                    Type::Array(elem) if is_primitive(elem) => "$kl.numCmp",
                    _ => "$kl.cmp",
                };
                format!("$kl.sort({}, {})", p, cmp)
            }
            Mutation::SortBy => format!("$kl.sortBy({}, {})", p, arg(0)),
            Mutation::Remove => format!("$kl.mremove({}, {})", p, arg(0)),
            Mutation::Shuffle => format!("$kl.shuffle({})", p),
            Mutation::StructMethod(name) => format!("{}.{}({})", p, method_id(name), self.exprs(args)),
        }
    }

    /// `assert(...)` and `assert_eq(...)`: a failure stops the program (or the test) with
    /// where it happened.
    fn assertion(&self, op: &Builtin, args: &[TExpr], e: &TExpr) -> String {
        let at = self.place_of(e.pos);
        match (op, args) {
            (Builtin::AssertEq, [value, expected]) => {
                let eq = if is_primitive(&value.ty) { "(a, b) => a === b" } else { "$kl.eq" };
                format!(
                    "$kl.assertEq({}, {}, {}, {}, {})",
                    self.expr(value),
                    self.expr(expected),
                    eq,
                    self.shower(&value.ty),
                    js_str(&format!("assert_eq failed at {}", at))
                )
            }
            (_, [cond, message]) => format!(
                "$kl.assert({}, {}, {})",
                self.expr(cond),
                js_str(&format!("assertion failed at {}", at)),
                self.expr(message)
            ),
            (_, [cond, ..]) => {
                format!("$kl.assert({}, {}, null)", self.expr(cond), js_str(&format!("assertion failed at {}", at)))
            }
            _ => "undefined".into(),
        }
    }

    fn builtin(&self, op: &Builtin, args: &[TExpr], e: &TExpr) -> String {
        let a = |i: usize| self.expr(&args[i]);
        use Builtin::*;
        match op {
            Assert | AssertEq => self.assertion(op, args, e),
            Print | Println => {
                let parts: Vec<String> = args.iter().map(|x| self.show(&self.expr(x), &x.ty)).collect();
                let text = if parts.is_empty() { "\"\"".to_string() } else { parts.join(" + \" \" + ") };
                format!("$kl.{}({})", if *op == Println { "println" } else { "print" }, text)
            }
            Len => match args[0].ty {
                Type::Str => format!("$kl.slen({})", self.recv(&args[0])),
                Type::Map(..) => format!("{}.size", self.recv(&args[0])),
                _ => format!("{}.length", self.recv(&args[0])),
            },
            ToStr => self.show(&a(0), &args[0].ty),
            ToInt => match args[0].ty {
                Type::Str => format!("$kl.toIntS({})", a(0)),
                Type::Float => format!("$kl.toInt({})", a(0)),
                _ => a(0),
            },
            ToFloat => match args[0].ty {
                Type::Str => format!("$kl.toFloatS({})", a(0)),
                _ => a(0),
            },
            Input => "$kl.unsupported(\"input()\")".into(),
            Chr => format!("$kl.chr({})", a(0)),
            Args => "$kl.args()".into(),
            MathUnary(f) => {
                let f = match f.as_str() {
                    "ln" => "Math.log".to_string(),
                    "round" => "$kl.round".to_string(),
                    other => format!("Math.{}", other),
                };
                format!("{}({})", f, a(0))
            }
            MathBinary(f) => {
                let f = if f == "powf" { "pow" } else { f.as_str() };
                format!("Math.{}({}, {})", f, a(0), a(1))
            }
            Lerp => format!("$kl.lerp({}, {}, {})", a(0), a(1), a(2)),
            Abs => format!("Math.abs({})", a(0)),
            Sign if args[0].ty == Type::Float => format!("$kl.fsign({})", a(0)),
            Sign => format!("Math.sign({})", a(0)),
            Min => format!("Math.min({}, {})", a(0), a(1)),
            Max => format!("Math.max({}, {})", a(0), a(1)),
            Clamp => format!("$kl.clamp({}, {}, {})", a(0), a(1), a(2)),
            RandInt => format!("$kl.randInt({}, {})", a(0), a(1)),
            RandFloat => "$kl.randFloat()".into(),
            Chance => format!("($kl.randFloat() < {})", a(0)),
            Seed => format!("$kl.seed({})", a(0)),
            Pick => format!("$kl.pick({})", self.recv(&args[0])),
            Spawn => "$kl.unsupported(\"Thread.spawn\")".into(),
            Cores => "$kl.unsupported(\"Thread.cores\")".into(),
            NewChannel => "$kl.unsupported(\"Channel\")".into(),
            NewShared => "$kl.unsupported(\"Shared\")".into(),
            Wait => format!("$kl.wait({})", a(0)),
            AsyncSpawn => format!("$kl.spawn({})", a(0)),
            AsyncRun if self.block_on.get() => {
                self.used_block_on.set(true);
                format!("(await $kl.run({}))", a(0))
            }
            AsyncRun => "$kl.unsupported(\"Async.run outside `main`\")".into(),
            AsyncAll => format!("$kl.all({})", a(0)),
            Now => "$kl.now()".into(),
            Millis => "$kl.millis()".into(),
            Sleep => format!("$kl.sleep({})", a(0)),
        }
    }

    /// Methods of built-in types (arrays, maps, strings, `Result`).
    fn builtin_method(&self, op: &BuiltinMethod, recv: &TExpr, args: &[TExpr]) -> String {
        let r = self.recv(recv);
        let a = |i: usize| self.expr(&args[i]);
        use BuiltinMethod::*;
        match op {
            Join | IsDone | Receive | TryReceive | Close | Get | Set | Update | Send | ParMap => {
                format!("$kl.unsupported({})", js_str(&format!("`{:?}` on threads, channels and shared values", op)))
            }
            IsOk => format!("({}.t === \"Ok\")", r),
            IsErr => format!("({}.t === \"Err\")", r),
            OkValue => format!("$kl.okValue({})", r),
            ErrValue => format!("$kl.errValue({})", r),
            UnwrapOr => format!("$kl.unwrapOr({}, {})", r, a(0)),
            Unwrap => {
                let show = match &recv.ty {
                    Type::Result(_, e) if self.m.is_plain(e) => self.shower(e),
                    _ => "(() => \"an error\")".into(),
                };
                format!("$kl.clone($kl.unwrap({}, {}))", r, show)
            }
            ArrayLen => format!("{}.length", r),
            MapLen => format!("{}.size", r),
            ArrayMap => format!("$kl.amap({}, {})", r, a(0)),
            Filter => format!("$kl.afilter({}, {})", r, a(0)),
            Find => format!("$kl.afind({}, {})", r, a(0)),
            Contains if is_primitive(&args[0].ty) => format!("{}.includes({})", r, a(0)),
            Contains => format!("$kl.contains({}, {})", r, a(0)),
            MapGet => format!("$kl.mopt({}, {})", r, a(0)),
            MapHas => format!("{}.m.has({})", r, a(0)),
            Keys => format!("{}.keys().slice()", r),
            Values => format!("$kl.mvalues({})", r),
            StrLen => format!("$kl.slen({})", r),
            Upper => format!("{}.toUpperCase()", r),
            Lower => format!("{}.toLowerCase()", r),
            Trim => format!("{}.trim()", r),
            StrContains => format!("{}.includes({})", r, a(0)),
            Split => format!("$kl.split({}, {})", r, a(0)),
            Replace => format!("{}.replaceAll({}, {})", r, a(0), a(1)),
            StartsWith => format!("{}.startsWith({})", r, a(0)),
            EndsWith => format!("{}.endsWith({})", r, a(0)),
            StrFind => format!("$kl.find({}, {})", r, a(0)),
            CodeAt => format!("$kl.codeAt({}, {})", r, a(0)),
            Chars => format!("$kl.chars({}).slice()", r),
            ParseInt => format!("$kl.parseInt({})", r),
            ParseFloat => format!("$kl.parseFloat({})", r),
            ArrayJoin => format!("{}.join({})", r, a(0)),
        }
    }
}

/// JavaScript for the Rust functions the standard library's own modules call (their
/// `*_glue.rs` files), where a web build has an equivalent.
fn std_glue(path: &str) -> Option<&'static str> {
    match path {
        "reactive_disposed" => Some("$kl.fail(\"this reactive value was disposed (its scope has ended)\")"),
        _ => None,
    }
}

/// `a op b` for numbers and strings; int arithmetic is checked.
pub(crate) fn arith(op: BinOp, ty: &Type, a: &str, b: &str) -> String {
    match (ty, op) {
        (Type::Int, BinOp::Add) => format!("$kl.add({}, {})", a, b),
        (Type::Int, BinOp::Sub) => format!("$kl.sub({}, {})", a, b),
        (Type::Int, BinOp::Mul) => format!("$kl.mul({}, {})", a, b),
        (Type::Int, BinOp::Div) => format!("$kl.idiv({}, {})", a, b),
        (Type::Int, BinOp::Rem) => format!("$kl.irem({}, {})", a, b),
        _ => format!("({} {} {})", a, op.rust(), b),
    }
}

fn float_literal(v: f64) -> String {
    if v.is_infinite() {
        if v > 0.0 { "Infinity".into() } else { "(-Infinity)".into() }
    } else if v.is_nan() {
        "NaN".into()
    } else {
        format!("{:?}", v)
    }
}
