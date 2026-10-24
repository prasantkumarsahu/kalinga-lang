//! Kalinga types and signatures, and the generic-type helpers that work on them.

use std::collections::HashMap;
use std::fmt;

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
    /// An interface, with its type arguments when generic (`Store<int>`).
    Interface(String, Vec<Type>),
    /// An enum, with its type arguments when generic (`Tree<int>`).
    Enum(String, Vec<Type>),
    /// A value-type struct, with type arguments if it is generic (`Grid<int>`).
    Struct(String, Vec<Type>),
    /// An opaque Rust type declared with `extern type`.
    Extern(String),
    /// `Result<T, E>`: `Ok(T)` or `Err(E)`.
    Result(Box<Type>, Box<Type>),
    /// `Channel<T>`: a queue that threads use to pass values.
    Channel(Box<Type>),
    /// `Shared<T>`: a value several threads can read and change (behind a lock).
    Shared(Box<Type>),
    /// `Task<T>`: a running thread that will produce a `T`.
    Task(Box<Type>),
    /// `Async<T>`: the result of calling an `async fn`; `await` gives the `T`.
    Async(Box<Type>),
    /// `fn(A, B) -> R`
    Func(Vec<Type>, Box<Type>),
    /// `(int, str)`: two to six parts.
    Tuple(Vec<Type>),
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
        matches!(self, Type::Class(_) | Type::Interface(..) | Type::Generic(..))
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
            Type::Class(n) | Type::Extern(n) => write!(f, "{}", n),
            Type::Struct(n, args) | Type::Enum(n, args) | Type::Interface(n, args) if args.is_empty() => {
                write!(f, "{}", n)
            }
            Type::Struct(n, args) | Type::Enum(n, args) | Type::Interface(n, args) => {
                let a: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                write!(f, "{}<{}>", n, a.join(", "))
            }
            Type::NoneLit => write!(f, "none"),
            Type::Unknown => write!(f, "_"),
            Type::Param(n) => write!(f, "{}", n),
            Type::Result(t, e) => write!(f, "Result<{}, {}>", t, e),
            Type::Channel(t) => write!(f, "Channel<{}>", t),
            Type::Shared(t) => write!(f, "Shared<{}>", t),
            Type::Task(t) => write!(f, "Task<{}>", t),
            Type::Async(t) => write!(f, "Async<{}>", t),
            Type::Generic(n, args) => {
                let a: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                write!(f, "{}<{}>", n, a.join(", "))
            }
            Type::Tuple(parts) => {
                let ps: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
                write!(f, "({})", ps.join(", "))
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
    pub(crate) fn same_shape(&self, other: &Signature) -> bool {
        self.ret == other.ret
            && self.params.len() == other.params.len()
            && self.params.iter().zip(&other.params).all(|(a, b)| a.1 == b.1)
    }

    pub fn describe(&self) -> String {
        let ps: Vec<String> = self.params.iter().map(|(n, t)| format!("{}: {}", n, t)).collect();
        format!("({}) -> {}", ps.join(", "), self.ret)
    }
}

pub fn is_map_key(t: &Type) -> bool {
    matches!(t, Type::Int | Type::Str | Type::Bool)
}

/// Replaces type parameters using `map` (unknown parameters are left as they are).
pub fn subst(t: &Type, map: &HashMap<String, Type>) -> Type {
    match t {
        Type::Param(n) => map.get(n).cloned().unwrap_or_else(|| t.clone()),
        Type::Array(i) => Type::Array(Box::new(subst(i, map))),
        Type::Optional(i) => Type::Optional(Box::new(subst(i, map))),
        Type::Map(k, v) => Type::Map(Box::new(subst(k, map)), Box::new(subst(v, map))),
        Type::Func(ps, r) => Type::Func(ps.iter().map(|p| subst(p, map)).collect(), Box::new(subst(r, map))),
        Type::Tuple(parts) => Type::Tuple(parts.iter().map(|p| subst(p, map)).collect()),
        Type::Generic(n, args) => Type::Generic(n.clone(), args.iter().map(|a| subst(a, map)).collect()),
        Type::Struct(n, args) => Type::Struct(n.clone(), args.iter().map(|a| subst(a, map)).collect()),
        Type::Enum(n, args) => Type::Enum(n.clone(), args.iter().map(|a| subst(a, map)).collect()),
        Type::Interface(n, args) => Type::Interface(n.clone(), args.iter().map(|a| subst(a, map)).collect()),
        Type::Result(t, e) => Type::Result(Box::new(subst(t, map)), Box::new(subst(e, map))),
        Type::Channel(t) => Type::Channel(Box::new(subst(t, map))),
        Type::Shared(t) => Type::Shared(Box::new(subst(t, map))),
        Type::Task(t) => Type::Task(Box::new(subst(t, map))),
        Type::Async(t) => Type::Async(Box::new(subst(t, map))),
        other => other.clone(),
    }
}

pub fn subst_sig(sig: &Signature, map: &HashMap<String, Type>) -> Signature {
    Signature {
        params: sig.params.iter().map(|(n, t)| (n.clone(), subst(t, map))).collect(),
        ret: subst(&sig.ret, map),
    }
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
        (Type::Tuple(ps), Type::Tuple(as_)) if ps.len() == as_.len() => {
            for (p, a) in ps.iter().zip(as_) {
                unify(p, a, map);
            }
        }
        (Type::Generic(pn, pa), Type::Generic(an, aa))
        | (Type::Struct(pn, pa), Type::Struct(an, aa))
        | (Type::Enum(pn, pa), Type::Enum(an, aa))
        | (Type::Interface(pn, pa), Type::Interface(an, aa))
            if pn == an =>
        {
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
        Type::Tuple(parts) => parts.iter().any(has_params),
        Type::Generic(_, args) | Type::Struct(_, args) | Type::Enum(_, args) | Type::Interface(_, args) => {
            args.iter().any(has_params)
        }
        Type::Result(t, e) => has_params(t) || has_params(e),
        _ => false,
    }
}
