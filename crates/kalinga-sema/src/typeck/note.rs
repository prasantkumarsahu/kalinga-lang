//! Recording what names refer to (`crate::index`), and answering completion probes.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx};
use crate::builtins::LIBRARIES;
use crate::index::{Probe, Reference, Symbol, registry_receiver};
use crate::tast::*;
use crate::types::Type;

impl Checker<'_> {
    fn note(&self, span: Span, name: &str, symbol: Symbol, ty: &Type, is_decl: bool) {
        let r = Reference { span, name: name.to_string(), symbol, ty: ty.clone(), is_decl };
        self.index.borrow_mut().refs.push(r);
    }

    /// Declares a local variable or parameter and records the declaration.
    pub(super) fn declare(&self, cx: &mut Ctx, name: &str, ty: Type, mutable: bool, pos: Span) -> Result<()> {
        cx.declare(name, ty.clone(), mutable, pos)?;
        self.note(pos, name, Symbol::Local { name: name.to_string(), decl: pos, mutable }, &ty, true);
        Ok(())
    }

    /// Records the name `e` uses, now that it is checked as `t`.
    pub(super) fn note_expr(&self, cx: &Ctx, e: &Expr, t: &TExpr) {
        let (name, recv) = match &e.kind {
            ExprKind::Ident(n) | ExprKind::Call(n, _, _) | ExprKind::SuperCall(n, _) => (n, None),
            ExprKind::MethodCall(recv, n, _) | ExprKind::Field(recv, n) => (n, Some(&**recv)),
            _ => return,
        };
        // `Shape.Circle`, `Color.WHITE`, `Dog.create()`: the type name is a use of the type.
        if let Some(ExprKind::Ident(owner)) = recv.map(|r| &r.kind)
            && cx.peek(owner).is_none()
            && self.m.names.contains(owner)
        {
            self.note(recv.map_or(e.pos, |r| r.pos), owner, Symbol::Type(owner.clone()), &t.ty, false);
        }
        let library = match recv.map(|r| &r.kind) {
            Some(ExprKind::Ident(lib)) if LIBRARIES.contains(&lib.as_str()) && cx.peek(lib).is_none() => Some(lib),
            _ => None,
        };
        if let Some(symbol) = self.symbol_of(cx, &t.kind, name, library) {
            self.note(e.pos, name, symbol, &t.ty, false);
        }
    }

    fn symbol_of(&self, cx: &Ctx, kind: &TExprKind, name: &str, library: Option<&String>) -> Option<Symbol> {
        let member = |owner: String| Some(Symbol::Member { owner, name: name.to_string() });
        let builtin =
            |recv: &TExpr| Some(Symbol::Builtin { receiver: registry_receiver(&recv.ty), name: name.to_string() });
        match kind {
            TExprKind::Local(n) => {
                cx.peek(n).map(|v| Symbol::Local { name: n.clone(), decl: v.decl, mutable: v.mutable })
            }
            TExprKind::FnRef(n) => Some(Symbol::Function(n.clone())),
            TExprKind::Static { owner, name, .. } if owner.is_empty() => Some(Symbol::Const(name.clone())),
            TExprKind::Static { owner, .. } | TExprKind::Variant { enum_name: owner, .. } => member(owner.clone()),
            TExprKind::Field { recv, field } => self.field_owner(&recv.ty, field).and_then(member),
            TExprKind::SelfField { field, .. } => self.self_field_owner(cx, field).and_then(member),
            TExprKind::MathConst(_) | TExprKind::Builtin { .. } if library.is_some() => {
                library.map(|lib| Symbol::Library { lib: lib.clone(), name: name.to_string() })
            }
            TExprKind::Builtin { .. } => Some(Symbol::Builtin { receiver: None, name: name.to_string() }),
            TExprKind::BuiltinMethod { recv, .. } => builtin(recv),
            TExprKind::Mutate { place, op, .. } => match (op, place.ty()) {
                (Mutation::StructMethod(_), Type::Struct(st, _)) => member(st.clone()),
                (_, ty) => Some(Symbol::Builtin { receiver: registry_receiver(ty), name: name.to_string() }),
            },
            TExprKind::Call { callee, .. } => self.callee_symbol(cx, callee, name),
            _ => None,
        }
    }

    fn callee_symbol(&self, cx: &Ctx, callee: &Callee, name: &str) -> Option<Symbol> {
        let member = |owner: String| Some(Symbol::Member { owner, name: name.to_string() });
        match callee {
            Callee::Function { name, .. } => Some(Symbol::Function(name.clone())),
            Callee::Extern { .. } => Some(Symbol::Function(name.to_string())),
            Callee::Value(f) => self.symbol_of(cx, &f.kind, name, None),
            Callee::New { class: owner, .. } | Callee::StructNew { strukt: owner, .. } => {
                Some(Symbol::Type(owner.clone()))
            }
            Callee::Static { owner, .. } | Callee::SuperMethod { owner, .. } => member(owner.clone()),
            Callee::SelfMethod(_) => {
                cx.class.as_ref().and_then(|c| self.m.find_method(c, name)).and_then(|(o, _)| member(o))
            }
            Callee::Method { recv, .. } => self.method_owner(&recv.ty, name).and_then(member),
            Callee::StructMethod { recv, .. } => match &recv.ty {
                Type::Struct(st, _) => member(st.clone()),
                _ => None,
            },
            Callee::Ok | Callee::Err => Some(Symbol::Builtin { receiver: None, name: name.to_string() }),
        }
    }

    /// Records the name an assignment stores into.
    pub(super) fn note_place(&self, cx: &Ctx, e: &Expr, p: &TPlace) {
        let (name, symbol) = match p {
            TPlace::Local { name, .. } => {
                (name, cx.peek(name).map(|v| Symbol::Local { name: name.clone(), decl: v.decl, mutable: v.mutable }))
            }
            TPlace::SelfField { field, .. } => (field, self.self_field_owner(cx, field).map(|o| member(o, field))),
            TPlace::ObjectField { recv, field, .. } => {
                (field, self.field_owner(&recv.ty, field).map(|o| member(o, field)))
            }
            TPlace::StructField { base, field, .. } => {
                (field, self.field_owner(base.ty(), field).map(|o| member(o, field)))
            }
            TPlace::Static { owner, name, .. } => (name, Some(member(owner.clone(), name))),
            TPlace::Index { .. } => return,
        };
        if let Some(symbol) = symbol {
            self.note(e.pos, name, symbol, p.ty(), false);
        }
    }

    /// Records a use of field `field` through a base-class reference of type `ty`.
    pub(super) fn note_field(&self, pos: Span, ty: &Type, field: &str, field_ty: &Type) {
        if let Some(owner) = self.field_owner(ty, field) {
            self.note(pos, field, member(owner, field), field_ty, false);
        }
    }

    /// Records the variant a `match` arm names.
    pub(super) fn note_variant(&self, pos: Span, enum_name: &str, variant: &str) {
        if enum_name != "Result" {
            self.note(pos, variant, member(enum_name.to_string(), variant), &Type::Enum(enum_name.to_string()), false);
        }
    }

    /// The class or struct that declares field `field` of a value of type `ty`.
    fn field_owner(&self, ty: &Type, field: &str) -> Option<String> {
        match ty {
            Type::Struct(st, _) => Some(st.clone()),
            Type::Class(c) | Type::Generic(c, _) => {
                self.m.all_fields(c).into_iter().find(|(n, _, _)| n == field).map(|(_, _, owner)| owner)
            }
            _ => None,
        }
    }

    fn self_field_owner(&self, cx: &Ctx, field: &str) -> Option<String> {
        match (&cx.strukt, &cx.class) {
            (Some(st), _) => Some(st.clone()),
            (None, Some(c)) => self.field_owner(&Type::Class(c.clone()), field),
            _ => None,
        }
    }

    /// The class or interface that declares method `name` of a value of type `ty`.
    fn method_owner(&self, ty: &Type, name: &str) -> Option<String> {
        match ty {
            Type::Class(c) | Type::Generic(c, _) => self.m.find_method(c, name).map(|(owner, _)| owner),
            Type::Interface(i) => self.m.declaring_interface(i, name),
            _ => None,
        }
    }

    /// At [`crate::index::PROBE`] used as a name: records what is in scope, then stops
    /// checking this body.
    pub(super) fn probe_scope<T>(&self, cx: &Ctx, expected: Option<&Type>, pos: Span) -> Result<T> {
        self.index.borrow_mut().probe = Some(Probe::Scope {
            locals: cx.visible(),
            owner: cx.class.clone().or_else(|| cx.strukt.clone()),
            is_static: cx.is_static,
            expected: expected.cloned(),
        });
        err("completion probe", pos)
    }

    /// At `recv.PROBE`: records what `recv` is, then stops checking this body.
    pub(super) fn probe_members<T>(&self, cx: &mut Ctx, recv: &Expr, pos: Span) -> Result<T> {
        let probe = match &recv.kind {
            ExprKind::Ident(n) if cx.peek(n).is_none() && LIBRARIES.contains(&n.as_str()) => Probe::Library(n.clone()),
            ExprKind::Ident(n)
                if cx.peek(n).is_none()
                    && (self.m.enums.contains_key(n)
                        || self.m.classes.contains_key(n)
                        || self.m.structs.contains_key(n)) =>
            {
                Probe::TypeMembers(n.clone())
            }
            ExprKind::SelfRef => Probe::Members { ty: self.self_value(cx, recv.pos)?.ty, on_self: true },
            _ => {
                let ty = self.expr(cx, recv, None)?.ty;
                // A type parameter offers the methods of its (first) bound interface.
                let ty = match &ty {
                    Type::Param(p) => {
                        self.param_interfaces(p).first().map_or(ty.clone(), |i| Type::Interface(i.clone()))
                    }
                    _ => ty,
                };
                Probe::Members { ty, on_self: false }
            }
        };
        self.index.borrow_mut().probe = Some(probe);
        err("completion probe", pos)
    }
}

fn member(owner: String, name: &str) -> Symbol {
    Symbol::Member { owner, name: name.to_string() }
}
