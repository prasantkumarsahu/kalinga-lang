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
    }
    Ok((program, map))
}

/// Compiles Kalinga source (plus its imports) to Rust source.
/// `file` is the name used for the root file and to resolve its imports.
/// Errors are already rendered for display.
pub fn to_rust(file: &str, source: &str) -> Result<String, String> {
    compile(file, source, true)
}

/// Reports errors without requiring `fn main()`, so files meant to be imported can be
/// checked on their own (this is what editors run).
pub fn check(file: &str, source: &str) -> Result<(), String> {
    compile(file, source, false).map(|_| ())
}

fn compile(file: &str, source: &str, require_main: bool) -> Result<String, String> {
    let (program, map) = load(file, source)?;
    let model = checker::Model::build(&program, require_main).map_err(|e| map.render(&e))?;
    codegen::generate(&program, &model).map_err(|e| map.render(&e))
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
    let rust = to_rust(&input.display().to_string(), &source)?;

    let name = stem(input);
    let work = std::env::temp_dir().join("kalinga-build");
    std::fs::create_dir_all(&work).map_err(|e| format!("cannot create {}: {}", work.display(), e))?;
    let rs_path = work.join(format!("{}.rs", name));
    std::fs::write(&rs_path, rust).map_err(|e| format!("cannot write {}: {}", rs_path.display(), e))?;

    let exe = output.unwrap_or_else(|| work.join(format!("{}{}", name, std::env::consts::EXE_SUFFIX)));
    let result = Command::new("rustc")
        .args(["--edition", "2024", "-O", "-C", "overflow-checks=on", "--crate-name", &name])
        .arg(&rs_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|e| format!("cannot run `rustc` (is Rust installed and on PATH?): {}", e))?;
    if !result.status.success() {
        return Err(format!(
            "internal compiler error: generated Rust failed to compile.\nGenerated file: {}\n{}",
            rs_path.display(),
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(exe)
}

/// Builds and runs the program, returning its exit code.
pub fn run(input: &Path) -> Result<i32, String> {
    let exe = build(input, None)?;
    let status = Command::new(&exe).status().map_err(|e| format!("cannot run {}: {}", exe.display(), e))?;
    Ok(status.code().unwrap_or(1))
}
