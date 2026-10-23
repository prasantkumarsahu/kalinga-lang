//! utkal projects: the `utkal.toml` manifest, the `utkal.lock` file, and fetching the
//! packages a project depends on (from folders or git repositories).
//!
//! Git packages are cloned once into `~/.utkal/cache/git/` and checked out per commit into
//! `~/.utkal/cache/src/<name>-<commit>`. `utkal.lock` pins each git package to a commit,
//! so builds are repeatable until `utkal update`.

// Library code reports problems instead of panicking.
#![warn(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use kalinga_syntax::ast::CrateDep;
use kalinga_syntax::error::Span;

mod error;
pub use error::{ProjectError, Result};

pub const MANIFEST: &str = "utkal.toml";
pub const LOCK: &str = "utkal.lock";

pub struct Manifest {
    pub name: String,
    pub version: String,
    pub deps: Vec<Dep>,
    pub rust_deps: Vec<CrateDep>,
}

pub struct Dep {
    pub name: String,
    pub source: Source,
}

pub enum Source {
    /// A folder, relative to the package that names it.
    Path(String),
    Git {
        url: String,
        pin: GitPin,
    },
}

pub enum GitPin {
    Head,
    Tag(String),
    Branch(String),
    Rev(String),
    /// A version requirement (`"0.4"`, `"^1.2"`, `"~1.2.3"`, `"=1.0.0"`, `">=0.3"`, `"*"`),
    /// matched against the repository's tags (`v1.2.3` or `1.2.3`).
    Version(String),
}

impl Source {
    /// How the source is written in messages and in `utkal.lock`.
    pub fn describe(&self) -> String {
        match self {
            Source::Path(p) => format!("path {}", p),
            Source::Git { url, pin } => match pin {
                GitPin::Head => format!("git {}", url),
                GitPin::Tag(t) => format!("git {} tag {}", url, t),
                GitPin::Branch(b) => format!("git {} branch {}", url, b),
                GitPin::Rev(r) => format!("git {} rev {}", url, r),
                GitPin::Version(v) => format!("git {} version {}", url, v),
            },
        }
    }
}

/// The folder holding `utkal.toml` for `start` (a file or folder), searching upwards.
pub fn find_project(start: &Path) -> Option<PathBuf> {
    // A file not saved yet is looked for from its folder.
    let start = std::fs::canonicalize(start).ok().or_else(|| std::fs::canonicalize(start.parent()?).ok())?;
    let mut dir = if start.is_dir() { start.as_path() } else { start.parent()? };
    loop {
        if dir.join(MANIFEST).is_file() {
            return Some(strip_verbatim(dir));
        }
        dir = dir.parent()?;
    }
}

/// Windows canonical paths start with `\\?\`, which other tools do not always accept.
fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.display().to_string();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

pub fn read_manifest(dir: &Path) -> Result<Manifest> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(ProjectError::io("read", &path))?;
    parse_manifest(&text).map_err(ProjectError::in_manifest(path))
}

/// A problem with the contents of a manifest.
fn bad(problem: impl Into<String>) -> ProjectError {
    ProjectError::Manifest(problem.into())
}

pub fn parse_manifest(text: &str) -> Result<Manifest> {
    let doc: toml::Table = text.parse().map_err(|e: toml::de::Error| bad(e.message()))?;
    let package = doc.get("package").and_then(|p| p.as_table()).ok_or_else(|| bad("missing a [package] section"))?;
    let name = package.get("name").and_then(|n| n.as_str()).ok_or_else(|| bad("[package] needs a `name`"))?.to_string();
    if !valid_package_name(&name) {
        return Err(bad(format!(
            "`{}` is not a valid package name (use letters, digits, `-` and `_`, starting with a letter)",
            name
        )));
    }
    let version = package.get("version").and_then(|v| v.as_str()).unwrap_or("0.1.0").to_string();

    let mut deps = vec![];
    if let Some(table) = doc.get("dependencies") {
        let table = table.as_table().ok_or_else(|| bad("[dependencies] must be a table"))?;
        for (dname, spec) in table {
            if !valid_package_name(dname) {
                return Err(bad(format!("`{}` is not a valid package name", dname)));
            }
            let spec = spec
                .as_table()
                .ok_or_else(|| bad(format!("dependency `{}` needs `git = \"...\"` or `path = \"...\"`", dname)))?;
            let get = |k: &str| spec.get(k).and_then(|v| v.as_str()).map(str::to_string);
            let source = match (get("path"), get("git")) {
                (Some(p), None) => Source::Path(p),
                (None, Some(url)) => {
                    let pins: Vec<GitPin> = [
                        get("tag").map(GitPin::Tag),
                        get("branch").map(GitPin::Branch),
                        get("rev").map(GitPin::Rev),
                        get("version").map(GitPin::Version),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    if pins.len() > 1 {
                        return Err(bad(format!(
                            "dependency `{}`: use only one of `tag`, `branch`, `rev` and `version`",
                            dname
                        )));
                    }
                    if let Some(GitPin::Version(v)) = pins.first() {
                        Req::parse(v).ok_or_else(|| {
                            bad(format!("dependency `{}`: `{}` is not a version requirement", dname, v))
                        })?;
                    }
                    Source::Git { url, pin: pins.into_iter().next().unwrap_or(GitPin::Head) }
                }
                _ => {
                    return Err(bad(format!(
                        "dependency `{}` needs exactly one of `git = \"...\"` and `path = \"...\"`",
                        dname
                    )));
                }
            };
            deps.push(Dep { name: dname.clone(), source });
        }
    }

    let mut rust_deps = vec![];
    if let Some(table) = doc.get("rust-dependencies") {
        let table = table.as_table().ok_or_else(|| bad("[rust-dependencies] must be a table"))?;
        for (cname, spec) in table {
            let (version, features) = match spec {
                toml::Value::String(v) => (v.clone(), vec![]),
                toml::Value::Table(t) => {
                    let v = t
                        .get("version")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| bad(format!("crate `{}` needs a `version`", cname)))?;
                    let fs = match t.get("features") {
                        None => vec![],
                        Some(f) => f
                            .as_array()
                            .and_then(|a| a.iter().map(|x| x.as_str().map(str::to_string)).collect::<Option<Vec<_>>>())
                            .ok_or_else(|| bad(format!("crate `{}`: `features` must be a list of strings", cname)))?,
                    };
                    (v.to_string(), fs)
                }
                _ => return Err(bad(format!("crate `{}` needs a version, e.g. {} = \"1.0\"", cname, cname))),
            };
            rust_deps.push(CrateDep { name: cname.clone(), version, features, pos: Span::default() });
        }
    }
    Ok(Manifest { name, version, deps, rust_deps })
}

pub fn valid_package_name(n: &str) -> bool {
    n.starts_with(|c: char| c.is_ascii_alphabetic())
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The namespace (unit) name of a package: `my-lib` → `my_lib`.
pub fn unit_name(package: &str) -> String {
    package.replace('-', "_")
}

/// Adds a crate to `list`, merging it with an entry of the same name. Two versions merge
/// when they are compatible (`"0.4"` and `"0.4.16"`); the higher one is kept and the
/// features are combined.
pub fn merge_crate(list: &mut Vec<CrateDep>, dep: &CrateDep) -> Result<()> {
    let Some(existing) = list.iter_mut().find(|c| c.name == dep.name) else {
        list.push(dep.clone());
        return Ok(());
    };
    if existing.version != dep.version {
        match (Req::parse(&existing.version), Req::parse(&dep.version)) {
            (Some(a), Some(b)) if a.op == Op::Caret && b.op == Op::Caret && a.upper() == b.upper() => {
                if b.lower > a.lower {
                    existing.version = dep.version.clone();
                }
            }
            _ => {
                return Err(ProjectError::CrateVersions {
                    name: dep.name.clone(),
                    first: existing.version.clone(),
                    second: dep.version.clone(),
                });
            }
        }
    }
    for f in &dep.features {
        if !existing.features.contains(f) {
            existing.features.push(f.clone());
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ versions

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    /// `"1.2.3"`, `"v1.2"`, `"1"`; pre-releases (`1.0.0-beta`) are not versions here.
    fn parse(s: &str) -> Option<(Version, usize)> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let parts: Vec<&str> = s.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        let nums: Vec<u64> = parts.iter().map(|p| p.parse().ok()).collect::<Option<_>>()?;
        Some((Version(nums[0], *nums.get(1).unwrap_or(&0), *nums.get(2).unwrap_or(&0)), nums.len()))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Caret,
    Tilde,
    Exact,
    AtLeast,
    Any,
}

/// A version requirement; a bare version means `^version`, as in Cargo.
#[derive(Debug, Clone, Copy)]
pub struct Req {
    op: Op,
    lower: Version,
    /// How many parts were written (`0.4` → 2).
    parts: usize,
}

impl Req {
    pub fn parse(s: &str) -> Option<Req> {
        let s = s.trim();
        if s == "*" {
            return Some(Req { op: Op::Any, lower: Version(0, 0, 0), parts: 0 });
        }
        let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
            (Op::AtLeast, r)
        } else if let Some(r) = s.strip_prefix('^') {
            (Op::Caret, r)
        } else if let Some(r) = s.strip_prefix('~') {
            (Op::Tilde, r)
        } else if let Some(r) = s.strip_prefix('=') {
            (Op::Exact, r)
        } else {
            (Op::Caret, s)
        };
        let (lower, parts) = Version::parse(rest.trim())?;
        Some(Req { op, lower, parts })
    }

    /// The first version that no longer matches (for caret and tilde).
    fn upper(&self) -> Version {
        let Version(major, minor, patch) = self.lower;
        match self.op {
            Op::Caret if major > 0 || self.parts == 1 => Version(major + 1, 0, 0),
            Op::Caret if minor > 0 || self.parts == 2 => Version(0, minor + 1, 0),
            Op::Caret => Version(0, 0, patch + 1),
            Op::Tilde if self.parts >= 2 => Version(major, minor + 1, 0),
            Op::Tilde => Version(major + 1, 0, 0),
            _ => Version(u64::MAX, 0, 0),
        }
    }

    pub fn matches(&self, v: Version) -> bool {
        match self.op {
            Op::Any => true,
            Op::Exact => v == self.lower,
            Op::AtLeast => v >= self.lower,
            Op::Caret | Op::Tilde => v >= self.lower && v < self.upper(),
        }
    }
}

// ------------------------------------------------------------------ the lock file

pub struct Locked {
    pub name: String,
    pub source: String,
    pub commit: String,
}

pub fn read_lock(project: &Path) -> Vec<Locked> {
    let Ok(text) = std::fs::read_to_string(project.join(LOCK)) else { return vec![] };
    let Ok(doc) = text.parse::<toml::Table>() else { return vec![] };
    let Some(list) = doc.get("package").and_then(|p| p.as_array()) else { return vec![] };
    list.iter()
        .filter_map(|p| {
            let t = p.as_table()?;
            let s = |k: &str| t.get(k).and_then(|v| v.as_str()).map(str::to_string);
            Some(Locked { name: s("name")?, source: s("source")?, commit: s("commit")? })
        })
        .collect()
}

fn lock_text(entries: &[Locked]) -> String {
    let mut out = String::from("# Written by utkal: the exact commit of every git package. Do not edit by hand.\n");
    for e in entries {
        out.push_str(&format!(
            "\n[[package]]\nname = {}\nsource = {}\ncommit = {}\n",
            toml::Value::String(e.name.clone()),
            toml::Value::String(e.source.clone()),
            toml::Value::String(e.commit.clone())
        ));
    }
    out
}

// ------------------------------------------------------------------ resolving

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Use the lock file and the local cache only; never touch the network (the compiler,
    /// editors).
    Offline,
    /// Use the lock file, fetching what is missing (`utkal build`).
    Fetch,
    /// Ignore the lock file and pick the newest matching commits (`utkal update`).
    Update,
}

/// The packages of a project, ready to compile.
#[derive(Default)]
pub struct Resolved {
    /// Package name → its folder (holding `utkal.toml` and `src/`).
    pub packages: BTreeMap<String, PathBuf>,
    /// `[rust-dependencies]` of the project and all its packages, merged.
    pub rust_deps: Vec<CrateDep>,
}

/// Where a dependency's files come from, once its path folder (if any) has been found.
enum Place<'a> {
    Folder(PathBuf),
    Git { url: &'a str, pin: &'a GitPin },
}

/// Finds (and in `Fetch` / `Update` mode downloads) every package `project` depends on,
/// directly or through other packages, and rewrites `utkal.lock` when it changed.
pub fn resolve(project: &Path, mode: Mode) -> Result<Resolved> {
    let root = read_manifest(project)?;
    let lock = if mode == Mode::Update { vec![] } else { read_lock(project) };
    let mut out = Resolved::default();
    for c in &root.rust_deps {
        merge_crate(&mut out.rust_deps, c).map_err(ProjectError::in_manifest(project.join(MANIFEST)))?;
    }

    // name → (source description, who asked first, version of the chosen commit)
    let mut chosen: BTreeMap<String, (String, String, Option<Version>)> = BTreeMap::new();
    let mut new_lock: Vec<Locked> = vec![];
    let mut queue: Vec<(Dep, PathBuf, String)> =
        root.deps.into_iter().map(|d| (d, project.to_path_buf(), format!("`{}`", root.name))).collect();
    while !queue.is_empty() {
        let (dep, base, requirer) = queue.remove(0);
        let in_package = || ProjectError::in_package(&dep.name, &requirer);
        let place = match &dep.source {
            Source::Path(p) => Place::Folder(
                std::fs::canonicalize(base.join(p))
                    .map(|d| strip_verbatim(&d))
                    .map_err(|_| in_package()(ProjectError::MissingFolder(base.join(p))))?,
            ),
            Source::Git { url, pin } => Place::Git { url, pin },
        };
        let desc = match &place {
            Place::Folder(dir) => format!("path {}", dir.display()),
            Place::Git { .. } => dep.source.describe(),
        };
        if let Some((first_desc, first_requirer, version)) = chosen.get(&dep.name) {
            let compatible = *first_desc == desc
                || match (&dep.source, version) {
                    (Source::Git { url, pin: GitPin::Version(req) }, Some(v)) => {
                        first_desc.starts_with(&format!("git {} ", url))
                            && Req::parse(req).is_some_and(|r| r.matches(*v))
                    }
                    _ => false,
                };
            if compatible {
                continue;
            }
            return Err(ProjectError::Conflict {
                name: dep.name.clone(),
                first: first_desc.clone(),
                first_by: first_requirer.clone(),
                second: desc,
                second_by: requirer,
            });
        }

        let (dir, version) = match place {
            Place::Folder(dir) => (dir, None),
            Place::Git { url, pin } => {
                let locked = lock.iter().find(|l| l.name == dep.name && l.source == desc).map(|l| l.commit.clone());
                let (dir, commit, version) = fetch_git(&dep.name, url, pin, locked, mode).map_err(in_package())?;
                new_lock.push(Locked { name: dep.name.clone(), source: desc.clone(), commit });
                (dir, version)
            }
        };
        let manifest = read_manifest(&dir).map_err(in_package())?;
        if manifest.name != dep.name {
            return Err(ProjectError::WrongName {
                requirer,
                wanted: dep.name.clone(),
                manifest: dir.join(MANIFEST),
                found: manifest.name,
            });
        }
        for c in &manifest.rust_deps {
            merge_crate(&mut out.rust_deps, c).map_err(ProjectError::in_manifest(dir.join(MANIFEST)))?;
        }
        for d in manifest.deps {
            queue.push((d, dir.clone(), format!("`{}`", dep.name)));
        }
        chosen.insert(dep.name.clone(), (desc, requirer, version));
        out.packages.insert(dep.name.clone(), dir);
    }

    if mode != Mode::Offline {
        new_lock.sort_by(|a, b| a.name.cmp(&b.name));
        let text = lock_text(&new_lock);
        let path = project.join(LOCK);
        let old = std::fs::read_to_string(&path).ok();
        if new_lock.is_empty() && old.is_none() {
            // No git packages: no lock file needed.
        } else if old.as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).map_err(ProjectError::io("write", &path))?;
        }
    }
    Ok(out)
}

/// `~/.utkal`, or `$UTKAL_HOME`.
pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("UTKAL_HOME") {
        return PathBuf::from(h);
    }
    let user = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(user).join(".utkal")
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.arg("-C").arg(d);
    }
    let out = cmd.args(args).output().map_err(ProjectError::GitMissing)?;
    if !out.status.success() {
        return Err(ProjectError::Git {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Returns the checkout folder, its commit and (when tagged) its version.
fn fetch_git(
    name: &str,
    url: &str,
    pin: &GitPin,
    locked: Option<String>,
    mode: Mode,
) -> Result<(PathBuf, String, Option<Version>)> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    let cache = home().join("cache");
    let git_cache = cache.join("git");
    let db = git_cache.join(format!("{}-{:08x}", name, h.finish() as u32));
    let not_downloaded = || ProjectError::NotDownloaded { commit: None };

    let mut fetched = false;
    if !db.join("HEAD").exists() {
        if mode == Mode::Offline {
            return Err(not_downloaded());
        }
        std::fs::create_dir_all(&git_cache).map_err(ProjectError::io("create", &db))?;
        eprintln!("fetching {} from {}", name, url);
        git(None, &["clone", "--bare", "--quiet", url, &db.display().to_string()])?;
        fetched = true;
    }
    let has = |c: &str| git(Some(&db), &["cat-file", "-e", &format!("{}^{{commit}}", c)]).is_ok();
    let commit = match locked {
        Some(c) if has(&c) => c,
        Some(c) if mode == Mode::Offline => {
            return Err(ProjectError::NotDownloaded { commit: Some(short(&c).to_string()) });
        }
        locked => {
            if mode == Mode::Offline {
                return Err(not_downloaded());
            }
            if !fetched {
                eprintln!("updating {} from {}", name, url);
                git(Some(&db), &["fetch", "--quiet", "--force", "--tags", "origin", "+refs/heads/*:refs/heads/*"])?;
            }
            match locked {
                Some(c) if has(&c) => c,
                _ => pick_commit(&db, pin)?,
            }
        }
    };
    let version = git(Some(&db), &["tag", "--points-at", &commit])
        .unwrap_or_default()
        .lines()
        .filter_map(|t| Version::parse(t.trim()).map(|(v, _)| v))
        .max();

    let checkouts = cache.join("src");
    let checkout = checkouts.join(format!("{}-{}", name, short(&commit)));
    if !checkout.join(MANIFEST).exists() {
        let tmp = checkouts.join(format!("{}-{}.tmp", name, short(&commit)));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&checkouts).map_err(ProjectError::io("create", &tmp))?;
        git(None, &["clone", "--quiet", "--no-checkout", &db.display().to_string(), &tmp.display().to_string()])?;
        git(Some(&tmp), &["-c", "advice.detachedHead=false", "checkout", "--quiet", "--detach", &commit])?;
        let _ = std::fs::remove_dir_all(&checkout);
        std::fs::rename(&tmp, &checkout).map_err(ProjectError::io("create", &checkout))?;
        if !checkout.join(MANIFEST).exists() {
            return Err(ProjectError::NoManifest { url: url.to_string(), commit: short(&commit).to_string() });
        }
    }
    Ok((checkout, commit, version))
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

fn pick_commit(db: &Path, pin: &GitPin) -> Result<String> {
    let rev = |r: &str| git(Some(db), &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", r)]);
    let missing = |kind: &'static str, name: &str| ProjectError::NoSuchRef { kind, name: name.to_string() };
    match pin {
        GitPin::Head => rev("HEAD").map_err(|_| ProjectError::EmptyRepository),
        GitPin::Tag(t) => rev(&format!("refs/tags/{}", t)).map_err(|_| missing("tag", t)),
        GitPin::Branch(b) => rev(&format!("refs/heads/{}", b)).map_err(|_| missing("branch", b)),
        GitPin::Rev(r) => rev(r).map_err(|_| missing("commit", r)),
        GitPin::Version(req) => {
            let r = Req::parse(req).ok_or_else(|| ProjectError::BadRequirement(req.clone()))?;
            let tags = git(Some(db), &["tag", "--list"])?;
            let best = tags
                .lines()
                .filter_map(|t| Version::parse(t.trim()).map(|(v, _)| (v, t.trim().to_string())))
                .filter(|(v, _)| r.matches(*v))
                .max();
            match best {
                Some((_, tag)) => rev(&format!("refs/tags/{}", tag)),
                None => Err(ProjectError::NoMatchingTag(req.clone())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_and_tilde() {
        let m = |r: &str, v: &str| Req::parse(r).unwrap().matches(Version::parse(v).unwrap().0);
        assert!(m("0.4", "0.4.9") && !m("0.4", "0.5.0") && !m("0.4", "0.3.9"));
        assert!(m("1.2", "1.9.0") && !m("1.2", "2.0.0"));
        assert!(m("^0.0.3", "0.0.3") && !m("^0.0.3", "0.0.4"));
        assert!(m("~1.2.3", "1.2.9") && !m("~1.2.3", "1.3.0"));
        assert!(m("=1.0.0", "1.0.0") && !m("=1.0.0", "1.0.1"));
        assert!(m(">=0.3", "7.0.0") && m("*", "0.0.1"));
    }

    #[test]
    fn crate_merging() {
        let dep = |v: &str, f: &[&str]| CrateDep {
            name: "macroquad".into(),
            version: v.into(),
            features: f.iter().map(|s| s.to_string()).collect(),
            pos: Span::default(),
        };
        let mut list = vec![];
        merge_crate(&mut list, &dep("0.4", &["audio"])).unwrap();
        merge_crate(&mut list, &dep("0.4.16", &[])).unwrap();
        assert_eq!(list[0].version, "0.4.16");
        assert_eq!(list[0].features, vec!["audio".to_string()]);
        assert!(merge_crate(&mut list, &dep("0.3", &[])).is_err());
    }
}
