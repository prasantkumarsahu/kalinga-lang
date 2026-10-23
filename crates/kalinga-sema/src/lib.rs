//! Meaning of a parsed program: package namespaces, the declaration model (classes,
//! interfaces, structs, enums, functions and their types), the rules they must follow, and
//! type checking of every body into the typed tree.

// Library code reports problems instead of panicking.
#![warn(clippy::unwrap_used)]

pub mod builtins;
mod declare;
pub mod index;
pub mod model;
pub mod namespaces;
pub mod tast;
pub mod typeck;
pub mod types;

pub use model::*;
pub use types::*;
