//! Meaning of a parsed program: package namespaces, the declaration model (classes,
//! interfaces, structs, enums, functions and their types), the rules they must follow, and
//! type checking of every body into the typed tree.

mod declare;
pub mod model;
pub mod namespaces;
pub mod tast;
pub mod typeck;
pub mod types;

pub use model::*;
pub use types::*;
