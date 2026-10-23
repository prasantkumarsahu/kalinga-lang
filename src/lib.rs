//! The Kalinga compiler, shared by the `kalinga` and `utkal` commands.

pub mod ast;
pub mod checker;
pub mod codegen;
pub mod driver;
pub mod error;
pub mod lexer;
pub mod namespaces;
pub mod parser;
pub mod project;
pub mod stdlib;
pub mod token;
