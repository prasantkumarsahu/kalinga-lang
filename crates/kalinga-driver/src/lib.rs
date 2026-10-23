//! Turns a `.kl` file into Rust source, then into a native executable via `rustc`.

mod error;
mod stdlib;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use kalinga_sema::namespaces;
use kalinga_syntax::ast::{CrateDep, Import, Program};
use kalinga_syntax::error::Diagnostic;
use kalinga_syntax::{lexer, parser};

pub use error::{CompileErrors, DriverError, Result, SourceMap};

/// The utkal project a file belongs to (if any) and its packages.
#[derive(Default)]
struct Packages {
    /// Package name → folder.
    dirs: BTreeMap<String, PathBuf>,
    rust_deps: Vec<CrateDep>,
    in_project: bool,
}

/// Finds the `utkal.toml` above `file` and the packages it uses, from the lock file and
/// the local cache (never the network: `utkal build` downloads).
fn packages_for(file: &str) -> Result<Packages> {
    let Some(project) = kalinga_project::find_project(Path::new(file)) else { return Ok(Packages::default()) };
    let r = kalinga_project::resolve(&project, kalinga_project::Mode::Offline)?;
    Ok(Packages { dirs: r.packages, rust_deps: r.rust_deps, in_project: true })
}

/// A failure while loading: an error in the program (shown with its source line) or
/// anything else.
enum Failure {
    Program(Diagnostic),
    Other(DriverError),
}

impl From<Diagnostic> for Failure {
    fn from(d: Diagnostic) -> Self {
        Failure::Program(d)
    }
}

impl From<DriverError> for Failure {
    fn from(e: DriverError) -> Self {
        Failure::Other(e)
    }
}

/// The files of one compilation, as they are loaded.
struct Loader {
    pkgs: Packages,
    map: SourceMap,
    units: namespaces::Units,
    program: Program,
    /// Canonical paths of files already queued.
    seen: HashSet<PathBuf>,
    /// Canonical path → unit, for files imported more than once.
    file_units: HashMap<PathBuf, String>,
    /// Files still to read: (path, display name, text, unit).
    queue: Vec<(PathBuf, String, String, String)>,
}

/// Parses `root` and, transitively, every file it imports, into one program.
/// Each file is loaded once, so import cycles are harmless.
fn load(root_name: &str, root_source: &str) -> Result<(Program, SourceMap)> {
    let root_path = PathBuf::from(root_name);
    let mut l = Loader {
        pkgs: packages_for(root_name)?,
        map: SourceMap::default(),
        units: namespaces::Units::default(),
        program: Program::default(),
        seen: HashSet::from([std::fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone())]),
        file_units: HashMap::new(),
        queue: vec![(root_path, root_name.to_string(), root_source.to_string(), String::new())],
    };
    match l.load_all() {
        Ok(()) => Ok((l.program, l.map)),
        Err(Failure::Program(d)) => Err(DriverError::compile(l.map, d)),
        Err(Failure::Other(e)) => Err(e),
    }
}

impl Loader {
    fn load_all(&mut self) -> std::result::Result<(), Failure> {
        for c in &self.pkgs.rust_deps {
            kalinga_project::merge_crate(&mut self.program.crates, c).map_err(DriverError::from)?;
        }
        while !self.queue.is_empty() {
            let (path, name, source, unit) = self.queue.remove(0);
            self.load_file(&path, name, source, unit)?;
        }
        self.map.display = namespaces::apply(&mut self.program, &self.units)?;
        Ok(())
    }

    fn load_file(
        &mut self,
        path: &Path,
        name: String,
        source: String,
        unit: String,
    ) -> std::result::Result<(), Failure> {
        let id = self.map.files.len() as u32;
        self.map.files.push((name, source));
        self.units.file_unit.push(unit.clone());
        self.units.file_aliases.push(Default::default());
        let parsed = lexer::lex(&self.map.files[id as usize].1, id).and_then(parser::parse)?;
        if !parsed.rust_files.is_empty() || parsed.extern_fns.iter().any(|x| x.body.is_some()) {
            self.units.rust_units.insert(unit.clone());
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for (file, pos) in &parsed.rust_files {
            self.add_rust_file(&dir, file, *pos)?;
        }
        for dep in &parsed.crates {
            kalinga_project::merge_crate(&mut self.program.crates, dep)
                .map_err(|e| Diagnostic::error(e.to_string(), dep.pos))?;
        }
        for import in &parsed.imports {
            self.add_import(&dir, id, &unit, import)?;
        }
        let p = &mut self.program;
        p.extern_types.extend(parsed.extern_types);
        p.extern_fns.extend(parsed.extern_fns);
        p.imports.extend(parsed.imports);
        p.functions.extend(parsed.functions);
        p.classes.extend(parsed.classes);
        p.interfaces.extend(parsed.interfaces);
        p.enums.extend(parsed.enums);
        p.structs.extend(parsed.structs);
        Ok(())
    }

    /// `use rust "file.rs"`, relative to the folder of the file that says it.
    fn add_rust_file(
        &mut self,
        dir: &Path,
        file: &str,
        pos: kalinga_syntax::error::Span,
    ) -> std::result::Result<(), Failure> {
        let target = dir.join(file);
        let canonical = std::fs::canonicalize(&target).map_err(|_| {
            Diagnostic::error(format!("cannot find Rust file `{}` (looked for {})", file, target.display()), pos)
        })?;
        let shown = canonical.display().to_string();
        // Windows canonical paths start with `\\?\`, which `#[path]` does not need.
        let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown).to_string();
        let rust_files = &mut self.program.rust_files;
        if rust_files.iter().any(|(p, _)| p == &shown) {
            return Ok(());
        }
        let module = shown.rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if rust_files.iter().any(|(p, _)| p.rsplit(['/', '\\']).next() == Some(module.as_str())) {
            return Err(Diagnostic::error(format!("two Rust files are named `{}`; rename one", module), pos).into());
        }
        rust_files.push((shown, pos));
        Ok(())
    }

    /// Where an import points and which unit (namespace) the target belongs to.
    fn import_target(
        &mut self,
        dir: &Path,
        unit: &str,
        import: &Import,
    ) -> std::result::Result<(PathBuf, String), Failure> {
        let fail = |m: String| Failure::Program(Diagnostic::error(m, import.pos));
        if let Some(module) = import.path.strip_prefix("std/") {
            let module = module.strip_suffix(".kl").unwrap_or(module);
            if module == "game" {
                self.program.main_thread = true;
            }
            if !stdlib::has(module) {
                return Err(fail(format!("there is no standard module `std/{}`", module)));
            }
            return Ok((stdlib::dir()?.join(format!("{}.kl", module)), format!("std_{}", module)));
        }
        if import.path.ends_with(".kl") {
            return Ok((dir.join(&import.path), unit.to_string()));
        }
        let (pkg, rest) = import.path.split_once('/').unwrap_or((&import.path, "lib"));
        match self.pkgs.dirs.get(pkg) {
            Some(pdir) => Ok((pdir.join("src").join(format!("{}.kl", rest)), kalinga_project::unit_name(pkg))),
            None if self.pkgs.in_project => Err(fail(format!(
                "unknown package `{}`: add it to utkal.toml (`utkal add {} --git <url>` or `--path <folder>`); files are imported with a `.kl` ending",
                pkg, pkg
            ))),
            None => Err(fail(format!(
                "unknown package `{}`: packages need a utkal.toml project (`utkal new`); files are imported with a `.kl` ending",
                pkg
            ))),
        }
    }

    fn add_import(&mut self, dir: &Path, id: u32, unit: &str, import: &Import) -> std::result::Result<(), Failure> {
        let fail = |m: String| Failure::Program(Diagnostic::error(m, import.pos));
        let (target, target_unit) = self.import_target(dir, unit, import)?;
        let Ok(canonical) = std::fs::canonicalize(&target) else {
            return Err(fail(format!("cannot find `{}` (looked for {})", import.path, target.display())));
        };
        let target_unit = self.file_units.get(&canonical).cloned().unwrap_or(target_unit);
        if let Some(alias) = &import.alias {
            self.units.file_aliases[id as usize].insert(alias.clone(), target_unit.clone());
        } else if target_unit != unit {
            self.units.imports.entry(unit.to_string()).or_default().insert(target_unit.clone());
        }
        if self.seen.insert(canonical.clone()) {
            let text = std::fs::read_to_string(&canonical)
                .map_err(|err| fail(format!("cannot read `{}`: {}", import.path, err)))?;
            self.file_units.insert(canonical, target_unit.clone());
            self.queue.push((target.clone(), target.display().to_string(), text, target_unit));
        }
        Ok(())
    }
}

/// Compiles Kalinga source (plus its imports) to Rust source.
/// `file` is the name used for the root file and to resolve its imports.
pub fn to_rust(file: &str, source: &str) -> Result<String> {
    compile(file, source, true).map(|(rust, _)| rust)
}

/// What the Rust build needs besides the generated source.
struct Native {
    /// crates.io dependencies.
    crates: Vec<CrateDep>,
    /// Whether the program calls hand-written Rust (`use rust` / `extern fn`).
    uses_rust: bool,
    /// `main` runs on the process's main thread (games), so it needs a big main stack.
    main_thread: bool,
}

/// Reports errors without requiring `fn main()`, so files meant to be imported can be
/// checked on their own (this is what editors run).
pub fn check(file: &str, source: &str) -> Result<()> {
    compile(file, source, false).map(|_| ())
}

fn compile(file: &str, source: &str, require_main: bool) -> Result<(String, Native)> {
    let (program, map) = load(file, source)?;
    let rust = match kalinga_sema::Model::build(&program, require_main)
        .and_then(|model| kalinga_codegen::generate(&program, &model))
    {
        Ok(rust) => rust,
        Err(d) => return Err(DriverError::compile(map, d)),
    };
    let native = Native {
        crates: program.crates.clone(),
        uses_rust: !program.rust_files.is_empty() || !program.extern_fns.is_empty() || !program.crates.is_empty(),
        main_thread: program.main_thread,
    };
    Ok((rust, native))
}

fn stem(path: &Path) -> String {
    let s: String = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) { format!("kl_{}", s) } else { s }
}

/// Name for the build files of `input`: its stem plus a short hash of the full path, so
/// two programs called `main.kl` in different folders can build at the same time.
fn build_name(input: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let full = std::fs::canonicalize(input).unwrap_or_else(|_| input.to_path_buf());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    full.hash(&mut h);
    format!("{}_{:08x}", stem(input), h.finish() as u32)
}

/// Compiles `input` to a native executable at `output` (or a temp location).
pub fn build(input: &Path, output: Option<PathBuf>) -> Result<PathBuf> {
    let source = std::fs::read_to_string(input).map_err(DriverError::io("read", input))?;
    let (rust, native) = compile(&input.display().to_string(), &source, true)?;

    let name = build_name(input);
    let work = std::env::temp_dir().join("kalinga-build");
    std::fs::create_dir_all(&work).map_err(DriverError::io("create", &work))?;
    let exe = output.unwrap_or_else(|| work.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX)));
    if !native.crates.is_empty() {
        return build_with_cargo(&work, &name, &rust, &native, &exe);
    }
    let rs_path = work.join(format!("{}.rs", name));
    std::fs::write(&rs_path, rust).map_err(DriverError::io("write", &rs_path))?;

    let result = Command::new("rustc")
        .args(["--edition", "2024", "-O", "-C", "overflow-checks=on", "--crate-name", &name])
        .args(native_flags(native.main_thread))
        .arg(&rs_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(DriverError::tool_missing("rustc"))?;
    if !result.status.success() {
        return Err(rust_rejected(&native, rs_path, &result.stderr));
    }
    Ok(exe)
}

/// Extra rustc flags. On Windows (MSVC) the C runtime is linked into the executable, so
/// a finished game runs on any Windows PC without installing the Visual C++ runtime; and
/// programs whose `main` stays on the main thread (games) get a 256 MB main stack, like
/// the worker thread other programs run on.
fn native_flags(main_thread: bool) -> Vec<&'static str> {
    let mut flags = vec![];
    if cfg!(target_env = "msvc") {
        flags.extend(["-C", "target-feature=+crt-static"]);
        if main_thread {
            flags.extend(["-C", "link-arg=/STACK:268435456"]);
        }
    }
    flags
}

fn host_triple() -> Result<String> {
    let out = Command::new("rustc").arg("-vV").output().map_err(DriverError::tool_missing("rustc"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(|h| h.trim().to_string()))
        .ok_or(DriverError::UnknownHost)
}

fn rust_rejected(native: &Native, generated: PathBuf, stderr: &[u8]) -> DriverError {
    DriverError::RustRejected {
        generated,
        stderr: String::from_utf8_lossy(stderr).into_owned(),
        uses_rust: native.uses_rust,
    }
}

/// `Cargo.toml` for a program that uses `crates`.
fn cargo_manifest(name: &str, crates: &[CrateDep]) -> String {
    let mut manifest = format!(
        "[package]\nname = \"{}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n\n[profile.release]\noverflow-checks = true\n\n[dependencies]\n",
        name.replace('_', "-")
    );
    for dep in crates {
        if dep.features.is_empty() {
            manifest.push_str(&format!("{} = {:?}\n", dep.name, dep.version));
        } else {
            let fs: Vec<String> = dep.features.iter().map(|f| format!("{:?}", f)).collect();
            manifest.push_str(&format!(
                "{} = {{ version = {:?}, features = [{}] }}\n",
                dep.name,
                dep.version,
                fs.join(", ")
            ));
        }
    }
    manifest
}

/// Builds a program that uses crates.io libraries with cargo. Compiled libraries are kept
/// in a shared target folder, so only the first build that uses a crate is slow.
fn build_with_cargo(work: &Path, name: &str, rust: &str, native: &Native, exe: &Path) -> Result<PathBuf> {
    let project = work.join("cargo").join(name);
    let src = project.join("src");
    std::fs::create_dir_all(&src).map_err(DriverError::io("create", &src))?;
    let manifest = cargo_manifest(name, &native.crates);
    let manifest_path = project.join("Cargo.toml");
    let deps_changed = std::fs::read_to_string(&manifest_path).ok().as_deref() != Some(manifest.as_str());
    std::fs::write(&manifest_path, &manifest).map_err(DriverError::io("write", &manifest_path))?;
    let main_rs = src.join("main.rs");
    std::fs::write(&main_rs, rust).map_err(DriverError::io("write", &main_rs))?;

    let target = work.join("cargo-target");
    if deps_changed {
        eprintln!("building with cargo (a crate used for the first time is downloaded and compiled once)...");
    }
    // With an explicit `--target`, RUSTFLAGS only reach the program, not build scripts or
    // proc-macros (which must not link the C runtime statically on Windows).
    let host = host_triple()?;
    let result = Command::new("cargo")
        .args(["build", "--release", "--quiet", "--target", &host, "--manifest-path"])
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", &target)
        .env("RUSTFLAGS", native_flags(native.main_thread).join(" "))
        .output()
        .map_err(DriverError::tool_missing("cargo"))?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        if ["failed to select a version", "no matching package", "failed to get"].iter().any(|m| stderr.contains(m)) {
            return Err(DriverError::CratesUnavailable { stderr: stderr.into_owned() });
        }
        return Err(rust_rejected(native, main_rs, &result.stderr));
    }
    let built =
        target.join(&host).join("release").join(format!("{}{}", name.replace('_', "-"), std::env::consts::EXE_SUFFIX));
    std::fs::copy(&built, exe).map_err(|source| DriverError::Copy {
        from: built.clone(),
        to: exe.to_path_buf(),
        source,
    })?;
    Ok(exe.to_path_buf())
}

/// Builds and runs the program, returning its exit code.
pub fn run(input: &Path) -> Result<i32> {
    let exe = build(input, None)?;
    let status = Command::new(&exe).status().map_err(|source| DriverError::Run { exe: exe.clone(), source })?;
    Ok(status.code().unwrap_or(1))
}
