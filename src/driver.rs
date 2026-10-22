//! Turns a `.kl` file into Rust source, then into a native executable via `rustc`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{checker, codegen, lexer, parser};

/// Compiles Kalinga source to Rust source. Errors are already rendered for display.
pub fn to_rust(file: &str, source: &str) -> Result<String, String> {
    let render = |e: crate::error::CompileError| e.render(file, source);
    let tokens = lexer::lex(source).map_err(render)?;
    let program = parser::parse(tokens).map_err(render)?;
    let model = checker::Model::build(&program).map_err(render)?;
    codegen::generate(&program, &model).map_err(render)
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
