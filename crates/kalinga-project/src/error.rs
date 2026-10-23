//! What can go wrong while reading a project or fetching its packages.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ProjectError {
    /// A file or folder operation failed; `action` is "read", "write" or "create".
    Io { action: &'static str, path: PathBuf, source: std::io::Error },
    /// A `utkal.toml` is not valid.
    Manifest(String),
    /// An error found while reading the manifest at `path`.
    InManifest { path: PathBuf, source: Box<ProjectError> },
    /// Two incompatible versions of one Rust crate.
    CrateVersions { name: String, first: String, second: String },
    /// A path dependency's folder does not exist.
    MissingFolder(PathBuf),
    /// Two requirements for one package that no single version meets.
    Conflict { name: String, first: String, first_by: String, second: String, second_by: String },
    /// A package's manifest names a different package than the one asked for.
    WrongName { requirer: String, wanted: String, manifest: PathBuf, found: String },
    /// A problem with package `name`, which `required_by` asked for.
    InPackage { name: String, required_by: String, source: Box<ProjectError> },
    /// `git` could not be started.
    GitMissing(std::io::Error),
    /// A `git` command failed.
    Git { args: String, stderr: String },
    /// In offline mode, a package (or its locked commit) is not in the cache.
    NotDownloaded { commit: Option<String> },
    /// A fetched commit has no `utkal.toml`.
    NoManifest { url: String, commit: String },
    /// The repository has no commits at all.
    EmptyRepository,
    /// No tag, branch or commit with that name; `kind` is "tag", "branch" or "commit".
    NoSuchRef { kind: &'static str, name: String },
    /// No tag matches a version requirement.
    NoMatchingTag(String),
    /// A version requirement that cannot be parsed.
    BadRequirement(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::Io { action, path, source } => write!(f, "cannot {} {}: {}", action, path.display(), source),
            ProjectError::Manifest(problem) => f.write_str(problem),
            ProjectError::InManifest { path, source } => write!(f, "{}: {}", path.display(), source),
            ProjectError::CrateVersions { name, first, second } => {
                write!(f, "crate `{}` is used with two versions: \"{}\" and \"{}\"", name, first, second)
            }
            ProjectError::MissingFolder(path) => write!(f, "folder {} not found", path.display()),
            ProjectError::Conflict { name, first, first_by, second, second_by } => write!(
                f,
                "package `{}` is required as `{}` by {} but as `{}` by {}; make them agree",
                name, first, first_by, second, second_by
            ),
            ProjectError::WrongName { requirer, wanted, manifest, found } => {
                write!(f, "{} asks for package `{}`, but {} is named `{}`", requirer, wanted, manifest.display(), found)
            }
            ProjectError::InPackage { name, required_by, source } => {
                write!(f, "package `{}` (required by {}): {}", name, required_by, source)
            }
            ProjectError::GitMissing(e) => write!(f, "cannot run `git` (is it installed and on PATH?): {}", e),
            ProjectError::Git { args, stderr } => write!(f, "`git {}` failed: {}", args, stderr),
            ProjectError::NotDownloaded { commit } => {
                if let Some(c) = commit {
                    write!(f, "commit {} ", c)?;
                }
                f.write_str("not downloaded yet; run `utkal build` in the project to fetch it")
            }
            ProjectError::NoManifest { url, commit } => {
                write!(f, "{} (commit {}) has no {}", url, commit, crate::MANIFEST)
            }
            ProjectError::EmptyRepository => f.write_str("the repository has no commits"),
            ProjectError::NoSuchRef { kind, name } => write!(f, "no {} `{}`", kind, name),
            ProjectError::NoMatchingTag(req) => {
                write!(f, "no tag matches version `{}` (tags look like `v1.2.3`)", req)
            }
            ProjectError::BadRequirement(req) => write!(f, "`{}` is not a version requirement", req),
        }
    }
}

impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProjectError::Io { source, .. } | ProjectError::GitMissing(source) => Some(source),
            ProjectError::InManifest { source, .. } | ProjectError::InPackage { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl ProjectError {
    pub(crate) fn io(action: &'static str, path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| ProjectError::Io { action, path, source }
    }

    pub(crate) fn in_manifest(path: PathBuf) -> impl FnOnce(Self) -> Self {
        move |source| ProjectError::InManifest { path, source: Box::new(source) }
    }

    pub(crate) fn in_package(name: &str, required_by: &str) -> impl FnOnce(Self) -> Self {
        let (name, required_by) = (name.to_string(), required_by.to_string());
        move |source| ProjectError::InPackage { name, required_by, source: Box::new(source) }
    }
}

pub type Result<T> = std::result::Result<T, ProjectError>;
