//! Turns a `.kl` file into Rust source, then into a native executable via `rustc`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ast::Program;
use crate::error::CompileError;
use crate::{checker, codegen, lexer, parser};

/// Every source file taking part in a compilation: (display name, text).
/// `Pos::file` indexes into it.
struct SourceMap {
    files: Vec<(String, String)>,
}

impl SourceMap {
    fn render(&self, e: &CompileError) -> String {
        let (name, text) = &self.files[e.pos.file as usize];
        e.render(name, text)
    }
}

/// Parses `root` and, transitively, every file it imports, into one program.
/// Each file is loaded once, so import cycles are harmless.
fn load(root_name: &str, root_source: &str) -> Result<(Program, SourceMap), String> {
    let mut map = SourceMap { files: vec![] };
    let mut program = Program::default();
    let root_path = PathBuf::from(root_name);
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(std::fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone()));
    let mut queue = vec![(root_path, root_name.to_string(), root_source.to_string())];

    while !queue.is_empty() {
        let (path, name, source) = queue.remove(0);
        let id = map.files.len() as u32;
        map.files.push((name, source));
        let text = &map.files[id as usize].1;
        let parsed = lexer::lex(text, id).and_then(parser::parse).map_err(|e| map.render(&e))?;

        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for (file, pos) in &parsed.rust_files {
            let target = dir.join(file);
            let canonical = std::fs::canonicalize(&target).map_err(|_| {
                map.render(&CompileError::new(format!("cannot find Rust file `{}` (looked for {})", file, target.display()), *pos))
            })?;
            let shown = canonical.display().to_string();
            // Windows canonical paths start with `\\?\`, which `#[path]` does not need.
            let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown).to_string();
            if program.rust_files.iter().any(|(p, _)| p == &shown) {
                continue;
            }
            let module = shown.rsplit(['/', '\\']).next().unwrap_or("").to_string();
            if program.rust_files.iter().any(|(p, _)| p.rsplit(['/', '\\']).next() == Some(module.as_str())) {
                let e = CompileError::new(format!("two Rust files are named `{}`; rename one", module), *pos);
                return Err(map.render(&e));
            }
            program.rust_files.push((shown, *pos));
        }
        for (name, version, pos) in &parsed.crates {
            if let Some((_, v, _)) = program.crates.iter().find(|(n, _, _)| n == name) {
                if v != version {
                    let e = CompileError::new(format!("crate `{}` is used with two versions: \"{}\" and \"{}\"", name, v, version), *pos);
                    return Err(map.render(&e));
                }
                continue;
            }
            program.crates.push((name.clone(), version.clone(), *pos));
        }
        program.extern_types.extend(parsed.extern_types);
        program.extern_fns.extend(parsed.extern_fns);
        for (import, pos) in &parsed.imports {
            let target = dir.join(import);
            let canonical = match std::fs::canonicalize(&target) {
                Ok(c) => c,
                Err(_) => {
                    let e = CompileError::new(format!("cannot find `{}` (looked for {})", import, target.display()), *pos);
                    return Err(map.render(&e));
                }
            };
            if seen.insert(canonical.clone()) {
                let text = std::fs::read_to_string(&canonical).map_err(|err| {
                    map.render(&CompileError::new(format!("cannot read `{}`: {}", import, err), *pos))
                })?;
                queue.push((target.clone(), target.display().to_string(), text));
            }
        }
        program.functions.extend(parsed.functions);
        program.classes.extend(parsed.classes);
        program.interfaces.extend(parsed.interfaces);
        program.enums.extend(parsed.enums);
        program.structs.extend(parsed.structs);
    }
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
    /// crates.io dependencies: (name, version).
    crates: Vec<(String, String)>,
    /// Whether the program calls hand-written Rust (`use rust` / `extern fn`).
    uses_rust: bool,
}

/// Reports errors without requiring `fn main()`, so files meant to be imported can be
/// checked on their own (this is what editors run).
pub fn check(file: &str, source: &str) -> Result<(), String> {
    compile(file, source, false).map(|_| ())
}

fn compile(file: &str, source: &str, require_main: bool) -> Result<(String, Native), String> {
    let (program, map) = load(file, source)?;
    let model = checker::Model::build(&program, require_main).map_err(|e| map.render(&e))?;
    let rust = codegen::generate(&program, &model).map_err(|e| map.render(&e))?;
    let native = Native {
        crates: program.crates.iter().map(|(n, v, _)| (n.clone(), v.clone())).collect(),
        uses_rust: !program.rust_files.is_empty() || !program.extern_fns.is_empty() || !program.crates.is_empty(),
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

/// Compiles `input` to a native executable at `output` (or a temp location).
pub fn build(input: &Path, output: Option<PathBuf>) -> Result<PathBuf, String> {
    let source = std::fs::read_to_string(input).map_err(|e| format!("cannot read {}: {}", input.display(), e))?;
    let (rust, native) = compile(&input.display().to_string(), &source, true)?;

    let name = stem(input);
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

fn rust_failure(native: &Native, generated: &Path, stderr: &str) -> String {
    if native.uses_rust {
        format!(
            "error: the Rust compiler rejected the program.\nThis program uses Rust code (`use rust`, `use crate` or `extern fn`): check that each \
             `extern` declaration matches the Rust code it names.\nGenerated file: {}\n{}",
            generated.display(),
            stderr
        )
    } else {
        format!("internal compiler error: generated Rust failed to compile.\nGenerated file: {}\n{}", generated.display(), stderr)
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
    for (krate, version) in &native.crates {
        manifest.push_str(&format!("{} = {:?}\n", krate, version));
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
    let result = Command::new("cargo")
        .args(["build", "--release", "--quiet", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .map_err(|e| format!("cannot run `cargo` (is Rust installed and on PATH?): {}", e))?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        if stderr.contains("failed to select a version") || stderr.contains("no matching package") || stderr.contains("failed to get") {
            return Err(format!("error: cargo could not get the crates this program uses (check the names, versions and your internet connection)\n{}", stderr));
        }
        return Err(rust_failure(native, &main_rs, &stderr));
    }
    let built = target.join("release").join(format!("{}{}", name.replace('_', "-"), std::env::consts::EXE_SUFFIX));
    std::fs::copy(&built, exe).map_err(|e| format!("cannot copy {} to {}: {}", built.display(), exe.display(), e))?;
    Ok(exe.to_path_buf())
}

/// Builds and runs the program, returning its exit code.
pub fn run(input: &Path) -> Result<i32, String> {
    let exe = build(input, None)?;
    let status = Command::new(&exe).status().map_err(|e| format!("cannot run {}: {}", exe.display(), e))?;
    Ok(status.code().unwrap_or(1))
}
