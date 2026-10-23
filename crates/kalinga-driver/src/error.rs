//! What can go wrong while compiling, building or running a program.

use std::fmt;
use std::path::PathBuf;

use kalinga_project::ProjectError;
use kalinga_sema::namespaces::DisplayNames;
use kalinga_syntax::error::Diagnostic;

/// Every source file taking part in a compilation: (display name, text).
/// `Span::file` indexes into it.
#[derive(Debug, Default)]
pub struct SourceMap {
    pub(crate) files: Vec<(String, String)>,
    /// Renamed declarations (`Vec2__mathx`) and how to show them (`mathx.Vec2`).
    pub(crate) display: DisplayNames,
}

impl SourceMap {
    /// The display name and text of file `id`.
    pub fn file(&self, id: u32) -> (&str, &str) {
        let (name, text) = &self.files[id as usize];
        (name, text)
    }

    /// The message with its source line, using package names (`mathx.Vec2`) rather than
    /// internal ones.
    pub fn render(&self, d: &Diagnostic) -> String {
        let (name, text) = self.file(d.span.file);
        let mut msg = d.render(name, text);
        for (mangled, shown) in &self.display {
            msg = msg.replace(mangled.as_str(), shown);
        }
        msg
    }
}

/// How many errors are shown at most; later ones are often caused by earlier ones.
pub const MAX_ERRORS: usize = 20;

/// Errors in the program itself, with the sources they point into.
#[derive(Debug)]
pub struct CompileErrors {
    pub sources: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for CompileErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered: Vec<String> = self.diagnostics.iter().map(|d| self.sources.render(d)).collect();
        f.write_str(&rendered.join("\n"))
    }
}

#[derive(Debug)]
pub enum DriverError {
    /// The program has errors.
    Compile(Box<CompileErrors>),
    /// The utkal project or its packages could not be read.
    Project(ProjectError),
    /// A file operation failed; `action` is "read", "write" or "create".
    Io { action: &'static str, path: PathBuf, source: std::io::Error },
    /// Copying the built executable to its destination failed.
    Copy { from: PathBuf, to: PathBuf, source: std::io::Error },
    /// `rustc` or `cargo` could not be started.
    ToolMissing { tool: &'static str, source: std::io::Error },
    /// The built program could not be started.
    Run { exe: PathBuf, source: std::io::Error },
    /// `rustc -vV` did not say which platform it builds for.
    UnknownHost,
    /// cargo could not download the crates the program uses.
    CratesUnavailable { stderr: String },
    /// The generated Rust did not compile. With `uses_rust`, the program's own Rust code or
    /// `extern` declarations are the likely cause; otherwise it is a compiler bug.
    RustRejected { generated: PathBuf, stderr: String, uses_rust: bool },
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::Compile(errors) => errors.fmt(f),
            DriverError::Project(e) => e.fmt(f),
            DriverError::Io { action, path, source } => write!(f, "cannot {} {}: {}", action, path.display(), source),
            DriverError::Copy { from, to, source } => {
                write!(f, "cannot copy {} to {}: {}", from.display(), to.display(), source)
            }
            DriverError::ToolMissing { tool, source } => {
                write!(f, "cannot run `{}` (is Rust installed and on PATH?): {}", tool, source)
            }
            DriverError::Run { exe, source } => write!(f, "cannot run {}: {}", exe.display(), source),
            DriverError::UnknownHost => f.write_str("cannot tell the Rust target platform (`rustc -vV`)"),
            DriverError::CratesUnavailable { stderr } => write!(
                f,
                "error: cargo could not get the crates this program uses (check the names, versions and your internet connection)\n{}",
                stderr
            ),
            DriverError::RustRejected { generated, stderr, uses_rust: true } => write!(
                f,
                "error: the Rust compiler rejected the program.\nThis program uses Rust code (`use rust`, `use crate` or `extern fn`): check that each \
                 `extern` declaration matches the Rust code it names.\nGenerated file: {}\n{}",
                generated.display(),
                stderr
            ),
            DriverError::RustRejected { generated, stderr, uses_rust: false } => write!(
                f,
                "internal compiler error: generated Rust failed to compile.\nGenerated file: {}\n{}",
                generated.display(),
                stderr
            ),
        }
    }
}

impl std::error::Error for DriverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DriverError::Project(e) => Some(e),
            DriverError::Io { source, .. }
            | DriverError::Copy { source, .. }
            | DriverError::ToolMissing { source, .. }
            | DriverError::Run { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ProjectError> for DriverError {
    fn from(e: ProjectError) -> Self {
        DriverError::Project(e)
    }
}

impl DriverError {
    pub(crate) fn io(action: &'static str, path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| DriverError::Io { action, path, source }
    }

    pub(crate) fn tool_missing(tool: &'static str) -> impl FnOnce(std::io::Error) -> Self {
        move |source| DriverError::ToolMissing { tool, source }
    }

    pub(crate) fn compile(sources: SourceMap, diagnostic: Diagnostic) -> Self {
        Self::compile_all(sources, vec![diagnostic])
    }

    /// Errors in the program, in source order; at most `MAX_ERRORS` are kept.
    pub(crate) fn compile_all(sources: SourceMap, mut diagnostics: Vec<Diagnostic>) -> Self {
        diagnostics.sort_by_key(|d| (d.span.file, d.span.line, d.span.col));
        diagnostics.truncate(MAX_ERRORS);
        DriverError::Compile(Box::new(CompileErrors { sources, diagnostics }))
    }
}

pub type Result<T> = std::result::Result<T, DriverError>;
