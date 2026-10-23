//! The Kalinga language server (`kalinga lsp`): diagnostics, hover, completion, signature
//! help, go to definition, find references, rename, document symbols and quick fixes, all
//! answered by the compiler itself.
//!
//! Each document is analysed as a whole program (with its imports, reading the editor's
//! text of open files). Type checking records what every name refers to
//! (`kalinga_sema::index`), which hover and navigation look up. Completion puts a probe
//! identifier where the user is typing and asks the type checker what it sees there.

// Library code reports problems instead of panicking.
#![warn(clippy::unwrap_used)]

pub mod actions;
pub mod completion;
mod describe;
pub mod hover;
pub mod navigation;
mod server;
pub mod signature;
mod snapshot;
mod text;
pub mod world;

pub use server::{capabilities, run};
