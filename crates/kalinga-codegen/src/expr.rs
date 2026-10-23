//! Expressions, calls and built-in operations.

use kalinga_sema::Type;
use kalinga_sema::tast::*;

use crate::{Gen, clone_if, fn_id, rid};

impl Gen<'_> {
    pub(crate) fn expr(&self, e: &TExpr) -> String {
        match &e.kind {
            TExprKind::Int(v) => format!("{}i64", v),
            TExprKind::Float(v) => format!("{:?}f64", v),
            TExprKind::Str(s) => format!("String::from({:?})", s),
            TExprKind::Bool(b) => b.to_string(),
            TExprKind::None => "None".into(),
            TExprKind::Local(name) => clone_if(rid(name), &e.ty),
            TExprKind::FnRef(name) => format!("(Rc::new({}) as {})", fn_id(name), self.rty(&e.ty)),
            TExprKind::StructSelf => "self.clone()".into(),
            TExprKind::ObjectSelf => format!("(self.__self.upgrade().unwrap() as {})", self.rty(&e.ty)),
            TExprKind::Convert(inner) => self.convert(self.expr(inner), &inner.ty, &e.ty),
            TExprKind::Array(items) => format!("vec![{}]", self.exprs(items)),
            TExprKind::Map(entries) => self.map_literal(entries),
            TExprKind::Interp(parts) => self.interp(parts),
            TExprKind::Neg(v) => format!("(-{})", self.expr(v)),
            TExprKind::Not(v) => format!("(!{})", self.expr(v)),
            TExprKind::Binary(op, l, r) => {
                if l.ty == Type::Str && *op == kalinga_syntax::ast::BinOp::Add {
                    format!("format!(\"{{}}{{}}\", {}, {})", self.expr(l), self.expr(r))
                } else {
                    format!("({} {} {})", self.expr(l), op.rust(), self.expr(r))
                }
            }
            TExprKind::IsNone { value, negated } => {
                format!("{}({}).is_none()", if *negated { "!" } else { "" }, self.expr(value))
            }
            TExprKind::StructOp { index, lhs, rhs, swapped } => self.struct_op(*index, lhs, rhs.as_deref(), *swapped),
            TExprKind::Downcast { value, target, via, test_only } => self.downcast(value, target, via, *test_only),
            TExprKind::Lambda(l) => self.lambda(l, &e.ty),
            TExprKind::If { cond, then, otherwise } => {
                format!(
                    "(if {} {} else {})",
                    self.expr(cond),
                    self.value_block(then, ""),
                    self.value_block(otherwise, "")
                )
            }
            TExprKind::Match { value, arms } => self.match_expr(value, arms),
            TExprKind::Await(v) => match v.ty {
                Type::Task(_) => format!("__task_wait({}).await", self.expr(v)),
                _ => format!("({}).await", self.expr(v)),
            },
            TExprKind::Try(v) => format!("({})?", self.expr(v)),
            TExprKind::Field { recv, field } => self.field(recv, field, &e.ty),
            TExprKind::SelfField { field, wrapped: true } => {
                format!("__unset(self.{}.clone(), {:?})", rid(field), field)
            }
            TExprKind::SelfField { field, .. } => clone_if(format!("self.{}", rid(field)), &e.ty),
            TExprKind::Static { owner, name, .. } if owner.is_empty() => format!("__k_{}()", name),
            TExprKind::Static { owner, name, mutable: true } => format!("{}::__s_{}().borrow().clone()", owner, name),
            TExprKind::Static { owner, name, mutable: false } => format!("{}::__c_{}()", owner, name),
            TExprKind::MathConst(name) => match name.as_str() {
                "PI" => "std::f64::consts::PI".into(),
                "TAU" => "std::f64::consts::TAU".into(),
                "E" => "std::f64::consts::E".into(),
                _ => "f64::INFINITY".into(),
            },
            TExprKind::Variant { enum_name, variant, args } if args.is_empty() => format!("{}::{}", enum_name, variant),
            TExprKind::Variant { enum_name, variant, args } => self.variant(enum_name, variant, args),
            TExprKind::Index { base, index } => self.index(base, index, &e.ty),
            TExprKind::Slice { base, from, to } => self.slice(base, from.as_deref(), to.as_deref()),
            TExprKind::Call { callee, args } => self.call(callee, args, &e.ty),
            TExprKind::Builtin { op, args } => self.builtin(op, args, &e.ty),
            TExprKind::BuiltinMethod { op, recv, args } => self.builtin_method(op, recv, args),
            TExprKind::Mutate { place, op, args } => self.mutate(place, op, args),
        }
    }

    /// `Enum.Variant(args)`, boxing the fields that lead back to the enum.
    fn variant(&self, enum_name: &str, variant: &str, args: &[TExpr]) -> String {
        let (binds, _) = self.bind_args(args);
        let names: Vec<String> = (0..args.len())
            .map(|i| match self.m.enums.get(enum_name) {
                Some(info) if info.is_boxed(variant, i) => format!("Box::new(__a{})", i),
                _ => format!("__a{}", i),
            })
            .collect();
        format!("{{ {}{}::{}({}) }}", binds, enum_name, variant, names.join(", "))
    }

    fn map_literal(&self, entries: &[(TExpr, TExpr)]) -> String {
        let inserts: String =
            entries.iter().map(|(k, v)| format!("__m.insert({}, {}); ", self.expr(k), self.expr(v))).collect();
        format!("{{ let mut __m = BTreeMap::new(); {}__m }}", inserts)
    }

    /// `"text {x}"` as `format!`, showing each value the Kalinga way.
    fn interp(&self, parts: &[TInterp]) -> String {
        let mut fmt = String::new();
        let mut args = vec![];
        for p in parts {
            match p {
                TInterp::Lit(s) => fmt.push_str(&s.replace('{', "{{").replace('}', "}}")),
                TInterp::Expr(x) => {
                    fmt.push_str("{}");
                    args.push(format!("__KlShow::__show(&{})", self.expr(x)));
                }
            }
        }
        format!("format!({:?}, {})", fmt, args.join(", "))
    }

    fn struct_op(&self, index: usize, lhs: &TExpr, rhs: Option<&TExpr>, swapped: bool) -> String {
        let Some(rhs) = rhs else { return format!("{{ let __l = {}; __l.__op_{}() }}", self.expr(lhs), index) };
        let call = if swapped { format!("__r.__op_{}(__l)", index) } else { format!("__l.__op_{}(__r)", index) };
        format!("{{ let __l = {}; let __r = {}; {} }}", self.expr(lhs), self.expr(rhs), call)
    }

    fn downcast(&self, value: &TExpr, target: &str, via: &Cast, test_only: bool) -> String {
        let code = self.expr(value);
        let call = match via {
            Cast::Same => format!("Some({})", code),
            Cast::Hook(tr) => {
                format!("{{ let __x = {}; let __t = {}::__as_{}(&*__x.borrow()); __t }}", code, tr, target)
            }
        };
        if test_only { format!("({}).is_some()", call) } else { call }
    }

    fn index(&self, base: &TExpr, index: &TExpr, ty: &Type) -> String {
        let b = self.recv(base);
        match base.ty {
            Type::Map(..) => format!("{{ let __k = {}; __map_get(&{}, &__k) }}", self.expr(index), b),
            Type::Str => format!("__char_at(&{}, {})", b, self.expr(index)),
            _ => format!("{{ let __i = __idx({}); {} }}", self.expr(index), clone_if(format!("{}[__i]", b), ty)),
        }
    }

    fn slice(&self, base: &TExpr, from: Option<&TExpr>, to: Option<&TExpr>) -> String {
        let bound = |b: Option<&TExpr>| b.map_or("None".into(), |x| format!("Some({})", self.expr(x)));
        let f = if base.ty == Type::Str { "__slice_str" } else { "__slice_vec" };
        format!("{}(&{}, {}, {})", f, self.recv(base), bound(from), bound(to))
    }

    fn exprs(&self, es: &[TExpr]) -> String {
        es.iter().map(|e| self.expr(e)).collect::<Vec<_>>().join(", ")
    }

    /// Arguments evaluated into `__aN` temporaries: (let-bindings, names).
    pub(crate) fn bind_args(&self, args: &[TExpr]) -> (String, String) {
        let binds: String =
            args.iter().enumerate().map(|(i, a)| format!("let __a{} = {}; ", i, self.expr(a))).collect();
        let names: Vec<String> = (0..args.len()).map(|i| format!("__a{}", i)).collect();
        (binds, names.join(", "))
    }

    /// A value used without taking it: a variable is borrowed in place, anything else is
    /// evaluated (in parentheses).
    pub(crate) fn recv(&self, e: &TExpr) -> String {
        match &e.kind {
            TExprKind::Local(name) => rid(name),
            _ => format!("({})", self.expr(e)),
        }
    }

    /// Receiver for a method call or field access: (setup statements, Rust expression).
    /// Locals, `self.field` of a struct and indexing into those are borrowed in place, so
    /// calling a method on `shapes[i]` does not copy the object reference. Anything else is
    /// evaluated once into `__rv`.
    fn receiver(&self, e: &TExpr) -> (String, String) {
        self.recv_place(e, &mut 0).unwrap_or_else(|| (format!("let __rv = {}; ", self.expr(e)), "__rv".into()))
    }

    /// A place that can be borrowed without cloning, or `None`.
    fn recv_place(&self, e: &TExpr, n: &mut usize) -> Option<(String, String)> {
        match &e.kind {
            TExprKind::Local(name) => Some((String::new(), rid(name))),
            TExprKind::SelfField { field, wrapped: false } => Some((String::new(), format!("self.{}", rid(field)))),
            TExprKind::Field { recv, field } if matches!(recv.ty, Type::Struct(..)) => {
                let (pre, b) = self.recv_place(recv, n)?;
                Some((pre, format!("{}.{}", b, rid(field))))
            }
            TExprKind::Index { base, index } if matches!(base.ty, Type::Array(_)) => {
                let (pre, b) = self.recv_place(base, n)?;
                let at = *n;
                *n += 1;
                Some((format!("{}let __ri{} = __idx({}); ", pre, at, self.expr(index)), format!("{}[__ri{}]", b, at)))
            }
            _ => None,
        }
    }

    fn field(&self, recv: &TExpr, field: &str, ty: &Type) -> String {
        let (setup, r) = self.receiver(recv);
        let read = match &recv.ty {
            Type::Struct(..) => {
                let read = clone_if(format!("{}.{}", r, rid(field)), ty);
                return if setup.is_empty() { read } else { format!("{{ {}let __t = {}; __t }}", setup, read) };
            }
            Type::Generic(c, _) if self.m.is_wrapped_field(c, field) => {
                format!("__unset({}.borrow().{}.clone(), {:?})", r, rid(field), field)
            }
            Type::Class(c) if self.m.has_subclasses(c) => format!("{}.borrow().__get_{}()", r, field),
            _ => clone_if(format!("{}.borrow().{}", r, rid(field)), ty),
        };
        format!("{{ {}let __t = {}; __t }}", setup, read)
    }

    fn lambda(&self, l: &TLambda, ty: &Type) -> String {
        let (caps, params, ret, body) = self.lambda_parts(l);
        format!("{{ {}let __f: {} = Rc::new(move |{}|{} {}); __f }}", caps, self.rty(ty), params, ret, body)
    }

    /// A lambda as a plain `move` closure, for threads.
    fn thread_closure(&self, e: &TExpr) -> String {
        let TExprKind::Lambda(l) = &e.kind else { return self.expr(e) };
        let (caps, params, ret, body) = self.lambda_parts(l);
        format!("{{ {}move |{}|{} {} }}", caps, params, ret, body)
    }

    /// (capture copies, parameters, ` -> Ret`, body) of a lambda.
    fn lambda_parts(&self, l: &TLambda) -> (String, String, String, String) {
        let caps: String = l.captures.iter().map(|(c, _)| format!("let {} = {}.clone(); ", rid(c), rid(c))).collect();
        let params: Vec<String> = l.params.iter().map(|(n, t)| format!("{}: {}", rid(n), self.rty(t))).collect();
        let ret = if l.ret == Type::Void { String::new() } else { format!(" -> {}", self.rty(&l.ret)) };
        let body = match &l.body {
            TLambdaBody::Expr(e) if e.ty == Type::Void => format!("{{ {}; }}", self.expr(e)),
            TLambdaBody::Expr(e) => self.expr(e),
            TLambdaBody::Block(stmts) => format!("{{\n{}}}", self.block_inner(stmts, 2)),
        };
        let body = if body.starts_with('{') { body } else { format!("{{ {} }}", body) };
        (caps, params.join(", "), ret, body)
    }

    fn call(&self, callee: &Callee, args: &[TExpr], ty: &Type) -> String {
        let (binds, names) = self.bind_args(args);
        match callee {
            Callee::Function { name, targs } => {
                format!("{{ {}{}{}({}) }}", binds, fn_id(name), self.turbofish(targs), names)
            }
            Callee::Extern { path } => format!("{{ {}{}({}) }}", binds, path, names),
            Callee::Value(f) => {
                let f = match &f.kind {
                    TExprKind::Local(n) => format!("&{}", rid(n)),
                    _ => self.expr(f),
                };
                format!("{{ let __f = {}; {}__f({}) }}", f, binds, names)
            }
            Callee::New { class, targs } if targs.is_empty() => {
                let code = format!("{}::__new({})", class, self.exprs(args));
                if self.m.has_subclasses(class) { format!("({} as {})", code, self.rty(ty)) } else { code }
            }
            Callee::New { class, targs } => {
                format!("{{ {}{}{}::__new({}) }}", binds, class, self.turbofish(targs), names)
            }
            Callee::StructNew { strukt, targs } => {
                let fields = &self.m.structs[strukt].fields;
                let inits: Vec<String> =
                    fields.iter().enumerate().map(|(i, (f, _))| format!("{}: __a{}", rid(f), i)).collect();
                format!("{{ {}{}{} {{ {} }} }}", binds, strukt, self.turbofish(targs), inits.join(", "))
            }
            Callee::Static { owner, name, targs } => {
                format!("{{ {}{}{}::__s_{}({}) }}", binds, owner, self.turbofish(targs), name, names)
            }
            Callee::SelfMethod(name) => format!("{{ {}self.{}({}) }}", binds, rid(name), names),
            Callee::SuperMethod { owner, name } => format!("{{ {}self.__{}__{}({}) }}", binds, owner, name, names),
            Callee::Method { recv, name, dispatch } => {
                let (setup, r) = self.receiver(recv);
                let call = match dispatch {
                    Some(tr) => {
                        let sep = if names.is_empty() { "" } else { ", " };
                        format!("{}::{}(&mut *__r.borrow_mut(){}{})", tr, rid(name), sep, names)
                    }
                    None => format!("__r.borrow_mut().{}({})", rid(name), names),
                };
                format!("{{ {}{}let __r = &{}; let __t = {}; __t }}", setup, binds, r, call)
            }
            Callee::StructMethod { recv, name } if matches!(recv.kind, TExprKind::StructSelf) => {
                format!("{{ {}self.{}({}) }}", binds, rid(name), names)
            }
            Callee::StructMethod { recv, name } => {
                let (setup, r) = self.receiver(recv);
                format!("{{ {}{}let __t = {}.{}({}); __t }}", setup, binds, r, rid(name), names)
            }
            Callee::Ok if args.is_empty() => "Ok(())".into(),
            Callee::Ok => format!("Ok({})", self.expr(&args[0])),
            Callee::Err => format!("Err({})", self.expr(&args[0])),
        }
    }

    fn mutate(&self, place: &TPlace, op: &Mutation, args: &[TExpr]) -> String {
        let (pre, p) = self.place(place, &mut 0);
        let arg = |i: usize| self.expr(&args[i]);
        match op {
            Mutation::Push => format!("{{ let __v = {}; {}{}.push(__v); }}", arg(0), pre, p),
            Mutation::Pop => format!("{{ {}{}.pop() }}", pre, p),
            Mutation::Sort => match place.ty() {
                Type::Array(elem) if **elem == Type::Float => {
                    format!("{{ {}{}.sort_by(|a, b| a.total_cmp(b)); }}", pre, p)
                }
                _ => format!("{{ {}{}.sort(); }}", pre, p),
            },
            Mutation::SortBy => format!(
                "{{ let __f = {}; {}{}.sort_by(|a, b| if __f(a.clone(), b.clone()) {{ std::cmp::Ordering::Less }} else if __f(b.clone(), a.clone()) {{ std::cmp::Ordering::Greater }} else {{ std::cmp::Ordering::Equal }}); }}",
                arg(0),
                pre,
                p
            ),
            Mutation::Remove => format!("{{ let __k = {}; {}{}.remove(&__k) }}", arg(0), pre, p),
            Mutation::Shuffle => format!("{{ {}__shuffle(&mut {}); }}", pre, p),
            Mutation::StructMethod(name) => {
                let (binds, names) = self.bind_args(args);
                format!("{{ {}{}let __t = {}.{}({}); __t }}", pre, binds, p, rid(name), names)
            }
        }
    }

    fn builtin(&self, op: &Builtin, args: &[TExpr], ty: &Type) -> String {
        let a = |i: usize| self.expr(&args[i]);
        use Builtin::*;
        match op {
            Print | Println => {
                let parts: Vec<String> = args.iter().map(|x| format!("__KlShow::__show(&{})", self.expr(x))).collect();
                match (op, parts.is_empty()) {
                    (Println, true) => "println!()".into(),
                    (Println, false) => format!("println!(\"{{}}\", [{}].join(\" \"))", parts.join(", ")),
                    (_, true) => "()".into(),
                    _ => format!("__print([{}].join(\" \"))", parts.join(", ")),
                }
            }
            Len => match args[0].ty {
                Type::Str => format!("({}.chars().count() as i64)", self.recv(&args[0])),
                _ => format!("({}.len() as i64)", self.recv(&args[0])),
            },
            ToStr => format!("__KlShow::__show(&{})", a(0)),
            ToInt => match args[0].ty {
                Type::Int => a(0),
                Type::Str => format!("__parse_int(&{})", a(0)),
                _ => format!("(({}) as i64)", a(0)),
            },
            ToFloat => match args[0].ty {
                Type::Float => a(0),
                Type::Str => format!("__parse_float(&{})", a(0)),
                _ => format!("(({}) as f64)", a(0)),
            },
            Input => format!("__input(&{})", if args.is_empty() { "String::new()".to_string() } else { a(0) }),
            Chr => format!("__chr({})", a(0)),
            Args => "std::env::args().skip(1).collect::<Vec<String>>()".into(),
            MathUnary(f) => format!("({}).{}()", a(0), f),
            MathBinary(f) => format!("({}).{}({})", a(0), f, a(1)),
            Lerp => format!("{{ let __a = {}; __a + ({} - __a) * {} }}", a(0), a(1), a(2)),
            Abs => format!("({}).abs()", a(0)),
            Sign => format!("({}).signum()", a(0)),
            Min => format!("({}).min({})", a(0), a(1)),
            Max => format!("({}).max({})", a(0), a(1)),
            Clamp => format!("__clamp({}, {}, {})", a(0), a(1), a(2)),
            RandInt => format!("__rand_int({}, {})", a(0), a(1)),
            RandFloat => "__rand_float()".into(),
            Chance => format!("(__rand_float() < {})", a(0)),
            Seed => format!("__rand_seed({})", a(0)),
            Pick => {
                let (setup, r) = self.receiver(&args[0]);
                format!(
                    "{{ {}let __xs = &{}; if __xs.is_empty() {{ None }} else {{ Some(__xs[__rand_int(0, __xs.len() as i64) as usize].clone()) }} }}",
                    setup, r
                )
            }
            Spawn => format!("__Task::spawn({})", self.thread_closure(&args[0])),
            Cores => "__cores()".into(),
            Wait => format!("__Async::new(__sleep({}))", a(0)),
            AsyncSpawn => format!("__async_spawn({})", a(0)),
            AsyncRun => format!("__block_on({})", a(0)),
            AsyncAll => format!("__async_all({})", a(0)),
            Now => "__time_now()".into(),
            Millis => "__time_millis()".into(),
            Sleep => format!("__time_sleep({})", a(0)),
            NewChannel => match ty {
                Type::Channel(t) => format!("__Channel::<{}>::new()", self.rty(t)),
                _ => "__Channel::new()".into(),
            },
            NewShared => format!("__Shared::new({})", a(0)),
        }
    }

    /// Methods of built-in types (arrays, maps, strings, `Result`, tasks, channels, shared values).
    fn builtin_method(&self, op: &BuiltinMethod, recv: &TExpr, args: &[TExpr]) -> String {
        let (setup, r) = self.receiver(recv);
        let a = |i: usize| self.expr(&args[i]);
        let wrap = |code: String| if setup.is_empty() { code } else { format!("{{ {}{} }}", setup, code) };
        let block = |code: String| format!("{{ {}{} }}", setup, code);
        use BuiltinMethod::*;
        match op {
            Join => block(format!("{}.join()", r)),
            IsDone => block(format!("{}.is_done()", r)),
            Receive => block(format!("{}.receive()", r)),
            TryReceive => block(format!("{}.try_receive()", r)),
            Close => block(format!("{}.close()", r)),
            Get => block(format!("{}.get()", r)),
            Send => format!("{{ let __v = {}; {}{}.send(__v) }}", a(0), setup, r),
            Set => format!("{{ let __v = {}; {}{}.set(__v) }}", a(0), setup, r),
            Update => format!("{{ let __f = {}; {}{}.update(|__v| __f(__v)) }}", a(0), setup, r),
            ParMap => format!("{{ let __f = {}; {}__par_map(&{}, &__f) }}", self.thread_closure(&args[0]), setup, r),
            IsOk => block(format!("{}.is_ok()", r)),
            IsErr => block(format!("{}.is_err()", r)),
            OkValue => block(format!("{}.clone().ok()", r)),
            ErrValue => block(format!("{}.clone().err()", r)),
            UnwrapOr => block(format!("{}.clone().unwrap_or({})", r, a(0))),
            Unwrap => {
                let show = match &recv.ty {
                    Type::Result(_, e) if self.m.is_plain(e) => "__KlShow::__show(&__e)",
                    _ => "String::from(\"an error\")",
                };
                block(format!(
                    "match {}.clone() {{ Ok(__v) => __v, Err(__e) => panic!(\"called unwrap() on an error: {{}}\", {}) }}",
                    r, show
                ))
            }
            ArrayLen | MapLen => wrap(format!("({}.len() as i64)", r)),
            ArrayMap => {
                let (elem, param) = match (&recv.ty, &args[0].ty) {
                    (Type::Array(elem), Type::Func(ps, _)) => ((**elem).clone(), ps[0].clone()),
                    _ => (Type::Unknown, Type::Unknown),
                };
                let arg = self.convert("__e.clone()".into(), &elem, &param);
                wrap(format!("{{ let __f = {}; {}.iter().map(|__e| __f({})).collect::<Vec<_>>() }}", a(0), r, arg))
            }
            Filter => wrap(format!(
                "{{ let __f = {}; {}.iter().filter(|__e| __f((*__e).clone())).cloned().collect::<Vec<_>>() }}",
                a(0),
                r
            )),
            Find => wrap(format!("{{ let __f = {}; {}.iter().find(|__e| __f((*__e).clone())).cloned() }}", a(0), r)),
            Contains => wrap(format!("{}.contains(&{})", r, a(0))),
            MapGet => wrap(format!("{}.get(&{}).cloned()", r, a(0))),
            MapHas => wrap(format!("{}.contains_key(&{})", r, a(0))),
            Keys => wrap(format!("{}.keys().cloned().collect::<Vec<_>>()", r)),
            Values => wrap(format!("{}.values().cloned().collect::<Vec<_>>()", r)),
            StrLen => wrap(format!("({}.chars().count() as i64)", r)),
            Upper => wrap(format!("{}.to_uppercase()", r)),
            Lower => wrap(format!("{}.to_lowercase()", r)),
            Trim => wrap(format!("{}.trim().to_string()", r)),
            StrContains => wrap(format!("{}.contains(({}).as_str())", r, a(0))),
            Split => wrap(format!("__split(&{}, &{})", r, a(0))),
            Replace => wrap(format!("{}.replace(({}).as_str(), ({}).as_str())", r, a(0), a(1))),
            StartsWith => wrap(format!("{}.starts_with(({}).as_str())", r, a(0))),
            EndsWith => wrap(format!("{}.ends_with(({}).as_str())", r, a(0))),
            StrFind => wrap(format!("__str_find(&{}, &{})", r, a(0))),
            CodeAt => wrap(format!("__code_at(&{}, {})", r, a(0))),
            Chars => wrap(format!("{}.chars().map(|__c| __c.to_string()).collect::<Vec<_>>()", r)),
            ParseInt => wrap(format!("{}.trim().parse::<i64>().ok()", r)),
            ParseFloat => wrap(format!("{}.trim().parse::<f64>().ok()", r)),
            ArrayJoin => wrap(format!("{}.join(({}).as_str())", r, a(0))),
        }
    }
}
