//! Kalinga source text to syntax tree: tokens, the lexer, the AST and the parser, plus
//! source positions and compile errors.

// Library code reports problems instead of panicking.
#![warn(clippy::unwrap_used)]

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod token;
