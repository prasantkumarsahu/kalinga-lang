//! Turns a `.kl` file into Rust source, then into a native executable via `rustc`.

mod stdlib;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use kalinga_sema::{checker, namespaces};
use kalinga_syntax::ast::Program;
use kalinga_syntax::error::Diagnostic;
use kalinga_syntax::{lexer, parser};

/// Every source file taking part in a compilation: (display name, text).
/// `Span::file` indexes into it.
struct SourceMap {
    files: Vec<(String, String)>,
    /// Renamed declarations (`Vec2__mathx`) and how to show them (`mathx.Vec2`).
    display: namespaces::DisplayNames,
}

impl SourceMap {
    fn render(&self, e: &Diagnostic) -> String {
        let (name, text) = &self.files[e.span.file as usize];
        let mut msg = e.render(name, text);
        for (mangled, shown) in &self.display {
            msg = msg.replace(mangled.as_str(), shown);
        }
        msg
    }
}

/// The utkal project a file belongs to (if any) and its packages.
#[derive(Default)]
struct Packages {
    /// Package name → folder.
    dirs: std::collections::BTreeMap<String, PathBuf>,
    rust_deps: Vec<kalinga_syntax::ast::CrateDep>,
    in_project: bool,
}

/// Finds the `utkal.toml` above `file` and the packages it uses, from the lock file and
/// the local cache (never the network: `utkal build` downloads).
fn packages_for(file: &str) -> Result<Packages, String> {
    let Some(project) = kalinga_project::find_project(Path::new(file)) else { return Ok(Packages::default()) };
    let r = kalinga_project::resolve(&project, kalinga_project::Mode::Offline)?;
    Ok(Packages { dirs: r.packages, rust_deps: r.rust_deps, in_project: true })
}

/// Parses `root` and, transitively, every file it imports, into one program.
/// Each file is loaded once, so import cycles are harmless.
fn load(root_name: &str, root_source: &str) -> Result<(Program, SourceMap), String> {
    let pkgs = packages_for(root_name)?;
    let mut map = SourceMap { files: vec![], display: vec![] };
    let mut units = namespaces::Units::default();
    let mut program = Program::default();
    for c in &pkgs.rust_deps {
        kalinga_project::merge_crate(&mut program.crates, c)?;
    }
    let root_path = PathBuf::from(root_name);
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(std::fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone()));
    // (path, display name, text, unit)
    let mut queue = vec![(root_path, root_name.to_string(), root_source.to_string(), String::new())];
    // Canonical path → unit, for files imported more than once.
    let mut file_units: std::collections::HashMap<PathBuf, String> = std::collections::HashMap::new();

    while !queue.is_empty() {
        let (path, name, source, unit) = queue.remove(0);
        let id = map.files.len() as u32;
        map.files.push((name, source));
        units.file_unit.push(unit.clone());
        units.file_aliases.push(Default::default());
        let text = &map.files[id as usize].1;
        let parsed = lexer::lex(text, id).and_then(parser::parse).map_err(|e| map.render(&e))?;
        if !parsed.rust_files.is_empty() || parsed.extern_fns.iter().any(|x| x.body.is_some()) {
            units.rust_units.insert(unit.clone());
        }

        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for (file, pos) in &parsed.rust_files {
            let target = dir.join(file);
            let canonical = std::fs::canonicalize(&target).map_err(|_| {
                map.render(&Diagnostic::error(
                    format!("cannot find Rust file `{}` (looked for {})", file, target.display()),
                    *pos,
                ))
            })?;
            let shown = canonical.display().to_string();
            // Windows canonical paths start with `\\?\`, which `#[path]` does not need.
            let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown).to_string();
            if program.rust_files.iter().any(|(p, _)| p == &shown) {
                continue;
            }
            let module = shown.rsplit(['/', '\\']).next().unwrap_or("").to_string();
            if program.rust_files.iter().any(|(p, _)| p.rsplit(['/', '\\']).next() == Some(module.as_str())) {
                let e = Diagnostic::error(format!("two Rust files are named `{}`; rename one", module), *pos);
                return Err(map.render(&e));
            }
            program.rust_files.push((shown, *pos));
        }
        for dep in &parsed.crates {
            kalinga_project::merge_crate(&mut program.crates, dep)
                .map_err(|m| map.render(&Diagnostic::error(m, dep.pos)))?;
        }
        program.extern_types.extend(parsed.extern_types);
        program.extern_fns.extend(parsed.extern_fns);
        for import in &parsed.imports {
            let fail = |m: String| map.render(&Diagnostic::error(m, import.pos));
            let (target, target_unit) = if let Some(module) = import.path.strip_prefix("std/") {
                let module = module.strip_suffix(".kl").unwrap_or(module);
                if module == "game" {
                    program.main_thread = true;
                }
                if !stdlib::has(module) {
                    return Err(fail(format!("there is no standard module `std/{}`", module)));
                }
                (stdlib::dir()?.join(format!("{}.kl", module)), format!("std_{}", module))
            } else if import.path.ends_with(".kl") {
                (dir.join(&import.path), unit.clone())
            } else {
                let (pkg, rest) = import.path.split_once('/').unwrap_or((&import.path, "lib"));
                match pkgs.dirs.get(pkg) {
                    Some(pdir) => (pdir.join("src").join(format!("{}.kl", rest)), kalinga_project::unit_name(pkg)),
                    None if pkgs.in_project => {
                        return Err(fail(format!(
                            "unknown package `{}`: add it to utkal.toml (`utkal add {} --git <url>` or `--path <folder>`); files are imported with a `.kl` ending",
                            pkg, pkg
                        )));
                    }
                    None => {
                        return Err(fail(format!(
                            "unknown package `{}`: packages need a utkal.toml project (`utkal new`); files are imported with a `.kl` ending",
                            pkg
                        )));
                    }
                }
            };
            let canonical = match std::fs::canonicalize(&target) {
                Ok(c) => c,
                Err(_) => return Err(fail(format!("cannot find `{}` (looked for {})", import.path, target.display()))),
            };
            let target_unit = file_units.get(&canonical).cloned().unwrap_or(target_unit);
            if let Some(alias) = &import.alias {
                units.file_aliases[id as usize].insert(alias.clone(), target_unit.clone());
            } else if target_unit != unit {
                units.imports.entry(unit.clone()).or_default().insert(target_unit.clone());
            }
            if seen.insert(canonical.clone()) {
                let text = std::fs::read_to_string(&canonical)
                    .map_err(|err| fail(format!("cannot read `{}`: {}", import.path, err)))?;
                file_units.insert(canonical, target_unit.clone());
                queue.push((target.clone(), target.display().to_string(), text, target_unit));
            }
        }
        program.imports.extend(parsed.imports);
        program.functions.extend(parsed.functions);
        program.classes.extend(parsed.classes);
        program.interfaces.extend(parsed.interfaces);
        program.enums.extend(parsed.enums);
        program.structs.extend(parsed.structs);
    }
    map.display = namespaces::apply(&mut program, &units).map_err(|e| map.render(&e))?;
    Ok((program, map))
}

/// Compiles Kalinga source (plus its imports) to Rust source.
/// `file` is the name used for the root file and to resolve its imports.
/// Errors are already rendered for display.
pub fn to_rust(file: &str, source: &str) -> Result<String, String> {
    compile(file, source, true).map(|(rust, _)| rust)
}

/// What the Rust build needs besides the generated source.
struct Native {
    /// crates.io dependencies.
    crates: Vec<kalinga_syntax::ast::CrateDep>,
    /// Whether the program calls hand-written Rust (`use rust` / `extern fn`).
    uses_rust: bool,
    /// `main` runs on the process's main thread (games), so it needs a big main stack.
    main_thread: bool,
}

/// Reports errors without requiring `fn main()`, so files meant to be imported can be
/// checked on their own (this is what editors run).
pub fn check(file: &str, source: &str) -> Result<(), String> {
    compile(file, source, false).map(|_| ())
}

fn compile(file: &str, source: &str, require_main: bool) -> Result<(String, Native), String> {
    let (program, map) = load(file, source)?;
    let model = checker::Model::build(&program, require_main).map_err(|e| map.render(&e))?;
    let rust = kalinga_codegen::generate(&program, &model).map_err(|e| map.render(&e))?;
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
pub fn build(input: &Path, output: Option<PathBuf>) -> Result<PathBuf, String> {
    let source = std::fs::read_to_string(input).map_err(|e| format!("cannot read {}: {}", input.display(), e))?;
    let (rust, native) = compile(&input.display().to_string(), &source, true)?;

    let name = build_name(input);
    let work = std::env::temp_dir().join("kalinga-build");
    std::fs::create_dir_all(&work).map_err(|e| format!("cannot create {}: {}", work.display(), e))?;
    let exe = output.unwrap_or_else(|| work.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX)));
    if !native.crates.is_empty() {
        return build_with_cargo(&work, &name, &rust, &native, &exe);
    }
    let rs_path = work.join(format!("{}.rs", name));
    std::fs::write(&rs_path, rust).map_err(|e| format!("cannot write {}: {}", rs_path.display(), e))?;

    let result = Command::new("rustc")
        .args(["--edition", "2024", "-O", "-C", "overflow-checks=on", "--crate-name", &name])
        .args(native_flags(native.main_thread))
        .arg(&rs_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|e| format!("cannot run `rustc` (is Rust installed and on PATH?): {}", e))?;
    if !result.status.success() {
        return Err(rust_failure(&native, &rs_path, &String::from_utf8_lossy(&result.stderr)));
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

fn host_triple() -> Result<String, String> {
    let out = Command::new("rustc").arg("-vV").output().map_err(|e| format!("cannot run `rustc`: {}", e))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(|h| h.trim().to_string()))
        .ok_or_else(|| "cannot tell the Rust target platform (`rustc -vV`)".to_string())
}

fn rust_failure(native: &Native, generated: &Path, stderr: &str) -> String {
    if native.uses_rust {
        format!(
            "error: the Rust compiler rejected the program.\nThis program uses Rust code (`use rust`, `use crate` or `extern fn`): check that each \
             `extern` declaration matches the Rust code it names.\nGenerated file: {}\n{}",
            generated.display(),
            stderr
        )
    } else {
        format!(
            "internal compiler error: generated Rust failed to compile.\nGenerated file: {}\n{}",
            generated.display(),
            stderr
        )
    }
}

/// Builds a program that uses crates.io libraries with cargo. Compiled libraries are kept
/// in a shared target folder, so only the first build that uses a crate is slow.
fn build_with_cargo(work: &Path, name: &str, rust: &str, native: &Native, exe: &Path) -> Result<PathBuf, String> {
    let project = work.join("cargo").join(name);
    let src = project.join("src");
    std::fs::create_dir_all(&src).map_err(|e| format!("cannot create {}: {}", src.display(), e))?;
    let mut manifest = format!(
        "[package]\nname = \"{}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n\n[profile.release]\noverflow-checks = true\n\n[dependencies]\n",
        name.replace('_', "-")
    );
    for dep in &native.crates {
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
    let manifest_path = project.join("Cargo.toml");
    let deps_changed = std::fs::read_to_string(&manifest_path).ok().as_deref() != Some(manifest.as_str());
    std::fs::write(&manifest_path, &manifest).map_err(|e| format!("cannot write Cargo.toml: {}", e))?;
    let main_rs = src.join("main.rs");
    std::fs::write(&main_rs, rust).map_err(|e| format!("cannot write {}: {}", main_rs.display(), e))?;

    let target = work.join("cargo-target");
    if deps_changed {
        eprintln!("building with cargo (a crate used for the first time is downloaded and compiled once)...");
    }
    // With an explicit `--target`, RUSTFLAGS only reach the program, not build scripts or
    // proc-macros (which must not link the C runtime statically on Windows).
    let host = host_triple()?;
    let result = Command::new("cargo")
        .args(["build", "--release", "--quiet", "--target", &host, "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .env("RUSTFLAGS", native_flags(native.main_thread).join(" "))
        .output()
        .map_err(|e| format!("cannot run `cargo` (is Rust installed and on PATH?): {}", e))?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        if stderr.contains("failed to select a version")
            || stderr.contains("no matching package")
            || stderr.contains("failed to get")
        {
            return Err(format!(
                "error: cargo could not get the crates this program uses (check the names, versions and your internet connection)\n{}",
                stderr
            ));
        }
        return Err(rust_failure(native, &main_rs, &stderr));
    }
    let built =
        target.join(&host).join("release").join(format!("{}{}", name.replace('_', "-"), std::env::consts::EXE_SUFFIX));
    std::fs::copy(&built, exe).map_err(|e| format!("cannot copy {} to {}: {}", built.display(), exe.display(), e))?;
    Ok(exe.to_path_buf())
}

/// Builds and runs the program, returning its exit code.
pub fn run(input: &Path) -> Result<i32, String> {
    let exe = build(input, None)?;
    let status = Command::new(&exe).status().map_err(|e| format!("cannot run {}: {}", exe.display(), e))?;
    Ok(status.code().unwrap_or(1))
}
