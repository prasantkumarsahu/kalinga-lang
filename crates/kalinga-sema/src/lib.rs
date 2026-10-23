//! Meaning of a parsed program: package namespaces, the declaration model (classes,
//! interfaces, structs, enums, functions and their types) and the rules they must follow.

mod declare;
pub mod model;
pub mod namespaces;
pub mod types;

pub use model::*;
pub use types::*;
