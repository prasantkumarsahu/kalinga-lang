//! What the names in a program refer to, recorded while type checking. Editors use it for
//! hover, go to definition, find references, rename and completion.

use kalinga_syntax::error::Span;

use crate::types::Type;

/// An identifier that editors put where the user is typing to ask the type checker what
/// could go there (see [`Probe`]). No program declares it.
pub const PROBE: &str = "kalinga_completion_probe";

/// The thing a name refers to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Symbol {
    /// A local variable or parameter, identified by where it is declared; `mutable` for
    /// a `var`.
    Local {
        name: String,
        decl: Span,
        mutable: bool,
    },
    Function(String),
    /// A class, struct, interface, enum or extern type.
    Type(String),
    /// A field, method, `static fn`, `const`, `static var` or enum variant of `owner`.
    Member {
        owner: String,
        name: String,
    },
    /// A top-level `const`.
    Const(String),
    /// `Math.sqrt`, `Thread.spawn`, `Math.PI`, ...
    Library {
        lib: String,
        name: String,
    },
    /// A built-in function (`receiver` is `None`), or a method of a built-in type, with
    /// the receiver as the built-ins registry writes it (`str`, `[T]`, `Result<T, E>`).
    Builtin {
        receiver: Option<&'static str>,
        name: String,
    },
}

/// One use (or declaration) of a name.
#[derive(Debug, Clone)]
pub struct Reference {
    /// Where the expression or declaration starts; the name is the first identifier
    /// `name` at or after it.
    pub span: Span,
    pub name: String,
    pub symbol: Symbol,
    /// The type of the expression (or of the declared variable).
    pub ty: Type,
    /// The declaration of a local, not a use.
    pub is_decl: bool,
}

/// What the type checker saw at [`PROBE`].
#[derive(Debug, Clone)]
pub enum Probe {
    /// A name is being typed: the variables in scope, and where the code is.
    Scope {
        locals: Vec<(String, Type)>,
        /// The class or struct whose method this is.
        owner: Option<String>,
        is_static: bool,
        /// The type the context wants, when it says.
        expected: Option<Type>,
    },
    /// After `value.`: the members of `ty`; `on_self` for `self.`, where private members
    /// are visible too.
    Members { ty: Type, on_self: bool },
    /// After `Type.`: an enum's variants, or a class's or struct's `static fn`s, constants
    /// and static variables.
    TypeMembers(String),
    /// After `Math.` and the other libraries.
    Library(String),
}

/// Everything type checking recorded for editors.
#[derive(Debug, Default)]
pub struct Index {
    pub refs: Vec<Reference>,
    pub probe: Option<Probe>,
}

/// The receiver of a built-in method as the built-ins registry writes it.
pub fn registry_receiver(t: &Type) -> Option<&'static str> {
    Some(match t {
        Type::Str => "str",
        Type::Array(_) => "[T]",
        Type::Map(..) => "{K: V}",
        Type::Result(..) => "Result<T, E>",
        Type::Task(_) => "Task<T>",
        Type::Channel(_) => "Channel<T>",
        Type::Shared(_) => "Shared<T>",
        _ => return None,
    })
}
