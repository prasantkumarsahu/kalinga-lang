//! utkal — the Kalinga package manager and project tool.

use std::path::{Path, PathBuf};
use std::process::{Command, exit};

use kalinga_driver as driver;
use kalinga_project::{self as project, MANIFEST, Mode};

/// Errors from the libraries and the command's own messages, shown as text.
type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

const USAGE: &str = "utkal — the Kalinga package manager

usage:
  utkal new <name> [--lib]          create a project (an app, or a library with --lib)
  utkal add <name> --git <url> [--tag <t> | --branch <b> | --rev <commit> | --version <req>]
  utkal add <name> --path <folder>  add a Kalinga package
  utkal add <crate> --rust <version> [--features a,b]
                                    add a Rust crate from crates.io
  utkal remove <name>               remove a package or Rust crate
  utkal build                       compile src/main.kl to target/<name>
  utkal run [args...]               build and run
  utkal check                       report errors (src/main.kl or src/lib.kl)
  utkal fix                         update src/ and tests/ for this Kalinga version
  utkal test                        run every tests/*.kl; a test passes when it exits with 0
  utkal update                      fetch the newest allowed versions and rewrite utkal.lock
  utkal -v | --version              print the installed version";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    let result = match args.first().map(String::as_str) {
        Some("-v" | "-version" | "--version" | "version") => {
            println!("utkal {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Some("new") => new(&rest),
        Some("add") => add(&rest),
        Some("remove") => remove(&rest),
        Some("build") => project_dir().and_then(|dir| build(&dir)).map(|exe| {
            println!("built {}", exe.display());
            0
        }),
        Some("run") => run(&rest),
        Some("check") => check(),
        Some("fix") => fix(),
        Some("test") => test(),
        Some("update") => project_dir().and_then(|dir| {
            let r = project::resolve(&dir, Mode::Update)?;
            println!("{} package(s) up to date", r.packages.len());
            Ok(0)
        }),
        Some(other) => Err(format!("unknown command `{}`\n\n{}", other, USAGE).into()),
        None => Err(USAGE.into()),
    };
    match result {
        Ok(code) => exit(code),
        Err(msg) => {
            eprintln!("{}", msg);
            exit(1);
        }
    }
}

/// The project folder: the nearest one above the current folder holding `utkal.toml`.
fn project_dir() -> CliResult<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot read the current folder: {}", e))?;
    project::find_project(&cwd).ok_or_else(|| {
        format!("no {} here or in any parent folder (create a project with `utkal new <name>`)", MANIFEST).into()
    })
}

fn new(args: &[&str]) -> CliResult<i32> {
    let Some(name) = args.first().filter(|a| !a.starts_with('-')) else {
        return Err("usage: utkal new <name> [--lib]".into());
    };
    let lib = args.contains(&"--lib");
    let dir = PathBuf::from(name);
    let pkg = dir.file_name().and_then(|n| n.to_str()).unwrap_or(name).to_string();
    if !project::valid_package_name(&pkg) {
        return Err(format!(
            "`{}` is not a valid package name (use letters, digits, `-` and `_`, starting with a letter)",
            pkg
        )
        .into());
    }
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()).into());
    }
    let src = dir.join("src");
    let write =
        |p: PathBuf, text: &str| std::fs::write(&p, text).map_err(|e| format!("cannot write {}: {}", p.display(), e));
    std::fs::create_dir_all(&src).map_err(|e| format!("cannot create {}: {}", src.display(), e))?;
    write(
        dir.join(MANIFEST),
        &format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nkalinga = \"{}\"\n\n[dependencies]\n\n[rust-dependencies]\n",
            pkg,
            language_version()
        ),
    )?;
    write(dir.join(".gitignore"), "target/\n")?;
    if lib {
        write(
            src.join("lib.kl"),
            "// The public part of this package: other projects use it with\n// `import \"NAME\"` once they `utkal add` it.\n\nfn greet(who: str) -> str {\n    return \"Hello, {who}!\"\n}\n"
                .replace("NAME", &pkg)
                .as_str(),
        )?;
        std::fs::create_dir_all(dir.join("tests")).map_err(|e| e.to_string())?;
        write(
            dir.join("tests").join("greet.kl"),
            "// A test passes when main returns Ok (exit code 0).\nimport \"../src/lib.kl\"\n\nfn main() -> Result<void, str> {\n    if greet(\"utkal\") != \"Hello, utkal!\" {\n        return Err(\"greet is wrong\")\n    }\n    return Ok()\n}\n",
        )?;
    } else {
        write(src.join("main.kl"), "fn main() {\n    println(\"Hello from Kalinga!\")\n}\n")?;
    }
    println!("created {} `{}` in {}", if lib { "library" } else { "app" }, pkg, dir.display());
    Ok(0)
}

/// Value of `--flag value` in `args`.
fn flag<'a>(args: &[&'a str], name: &str) -> Option<&'a str> {
    args.iter().position(|a| *a == name).and_then(|i| args.get(i + 1).copied())
}

fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

fn add(args: &[&str]) -> CliResult<i32> {
    let Some(name) = args.first().filter(|a| !a.starts_with('-')) else {
        return Err(format!("usage:\n{}", USAGE).into());
    };
    let dir = project_dir()?;
    let (section, line) = if let Some(version) = flag(args, "--rust") {
        let line = match flag(args, "--features") {
            Some(fs) => {
                let fs: Vec<String> = fs.split(',').map(|f| toml_str(f.trim())).collect();
                format!("{} = {{ version = {}, features = [{}] }}", name, toml_str(version), fs.join(", "))
            }
            None => format!("{} = {}", name, toml_str(version)),
        };
        ("rust-dependencies", line)
    } else {
        if !project::valid_package_name(name) {
            return Err(format!("`{}` is not a valid package name", name).into());
        }
        let source = match (flag(args, "--git"), flag(args, "--path")) {
            (Some(url), None) => {
                let mut s = format!("git = {}", toml_str(url));
                for key in ["tag", "branch", "rev", "version"] {
                    if let Some(v) = flag(args, &format!("--{}", key)) {
                        s.push_str(&format!(", {} = {}", key, toml_str(v)));
                    }
                }
                s
            }
            (None, Some(path)) => format!("path = {}", toml_str(&path.replace('\\', "/"))),
            _ => return Err("give the package's source: `--git <url>` or `--path <folder>` (or `--rust <version>` for a Rust crate)".into()),
        };
        ("dependencies", format!("{} = {{ {} }}", name, source))
    };
    let path = dir.join(MANIFEST);
    let old = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let text = set_entry(&old, section, name, Some(&line));
    project::parse_manifest(&text).map_err(|e| format!("cannot add `{}`: {}", name, e))?;
    std::fs::write(&path, &text).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    // Fetch now, so mistakes show up at once; undo the change if it fails.
    if let Err(e) = project::resolve(&dir, Mode::Fetch) {
        let _ = std::fs::write(&path, &old);
        return Err(e.into());
    }
    println!("added `{}` to [{}]", name, section);
    Ok(0)
}

fn remove(args: &[&str]) -> CliResult<i32> {
    let Some(name) = args.first() else { return Err("usage: utkal remove <name>".into()) };
    let dir = project_dir()?;
    let path = dir.join(MANIFEST);
    let old = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let mut text = old.clone();
    for section in ["dependencies", "rust-dependencies"] {
        text = set_entry(&text, section, name, None);
    }
    if text == old {
        return Err(format!("`{}` is not in {}", name, MANIFEST).into());
    }
    std::fs::write(&path, &text).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    project::resolve(&dir, Mode::Fetch)?;
    println!("removed `{}`", name);
    Ok(0)
}

/// Sets (or with `None` removes) the one-line entry `name = ...` in `[section]`, keeping
/// the rest of the file (comments included) as it is.
fn set_entry(text: &str, section: &str, name: &str, line: Option<&str>) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = format!("[{}]", section);
    let is_entry = |l: &str| {
        let key = l.split('=').next().unwrap_or("").trim().trim_matches('"');
        key == name && l.contains('=')
    };
    match lines.iter().position(|l| l.trim() == header) {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map_or(lines.len(), |i| start + 1 + i);
            match (lines[start + 1..end].iter().position(|l| is_entry(l)), line) {
                (Some(i), Some(new)) => lines[start + 1 + i] = new.to_string(),
                (Some(i), None) => {
                    lines.remove(start + 1 + i);
                }
                (None, Some(new)) => {
                    // After the last non-blank line of the section.
                    let mut at = end;
                    while at > start + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, new.to_string());
                }
                (None, None) => {}
            }
        }
        None => {
            if let Some(new) = line {
                if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push(header);
                lines.push(new.to_string());
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Fetches packages, then compiles `src/main.kl` to `target/<name>`.
fn build(dir: &Path) -> CliResult<PathBuf> {
    let manifest = project::read_manifest(dir)?;
    project::resolve(dir, Mode::Fetch)?;
    let main = dir.join("src").join("main.kl");
    if !main.is_file() {
        return Err(format!(
            "{} not found (a library has only src/lib.kl: use `utkal check` or `utkal test`)",
            main.display()
        )
        .into());
    }
    let target = dir.join("target");
    std::fs::create_dir_all(&target).map_err(|e| format!("cannot create {}: {}", target.display(), e))?;
    let exe = target.join(format!("{}{}", manifest.name, std::env::consts::EXE_SUFFIX));
    Ok(driver::build(&main, Some(exe))?)
}

fn run(args: &[&str]) -> CliResult<i32> {
    let dir = project_dir()?;
    let exe = build(&dir)?;
    let status = Command::new(&exe).args(args).status().map_err(|e| format!("cannot run {}: {}", exe.display(), e))?;
    Ok(status.code().unwrap_or(1))
}

/// Updates the project's code for this Kalinga version, and its `kalinga = "..."` line.
fn fix() -> CliResult<i32> {
    let dir = project_dir()?;
    for folder in ["src", "tests"] {
        if dir.join(folder).is_dir() {
            let report = driver::fix::fix_path(&dir.join(folder))?;
            for (file, n) in &report.changed {
                println!("fixed {} place(s) in {}", n, file.display());
            }
            for e in &report.skipped {
                eprintln!("skipped a file that is not valid Kalinga:\n{}", e);
            }
        }
    }
    let path = dir.join(MANIFEST);
    let old = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let line = format!("kalinga = {}", toml_str(&language_version()));
    let text = set_entry(&old, "package", "kalinga", Some(&line));
    if text != old {
        std::fs::write(&path, &text).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
        println!("set {} in {}", line, MANIFEST);
    }
    Ok(0)
}

/// This compiler's language version, `major.minor` (`0.5`).
fn language_version() -> String {
    env!("CARGO_PKG_VERSION").rsplit_once('.').map_or(env!("CARGO_PKG_VERSION"), |(v, _)| v).to_string()
}

fn check() -> CliResult<i32> {
    let dir = project_dir()?;
    project::resolve(&dir, Mode::Fetch)?;
    let mut checked = 0;
    for file in ["main.kl", "lib.kl"] {
        let path = dir.join("src").join(file);
        if path.is_file() {
            let source =
                std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
            driver::check(&path.display().to_string(), &source)?;
            checked += 1;
        }
    }
    if checked == 0 {
        return Err("no src/main.kl or src/lib.kl to check".into());
    }
    println!("no errors");
    Ok(0)
}

fn test() -> CliResult<i32> {
    let dir = project_dir()?;
    project::resolve(&dir, Mode::Fetch)?;
    let mut tests: Vec<PathBuf> = std::fs::read_dir(dir.join("tests"))
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "kl")).collect()
        })
        .unwrap_or_default();
    tests.sort();
    if tests.is_empty() {
        println!("no tests (put programs in tests/*.kl)");
        return Ok(0);
    }
    let mut failed = vec![];
    for t in &tests {
        let name = t.file_name().unwrap().to_string_lossy().to_string();
        let ok = match driver::build(t, None) {
            Ok(exe) => {
                let out = Command::new(&exe)
                    .current_dir(&dir)
                    .output()
                    .map_err(|e| format!("cannot run {}: {}", exe.display(), e))?;
                if !out.status.success() {
                    eprint!("{}", String::from_utf8_lossy(&out.stderr));
                }
                out.status.success()
            }
            Err(e) => {
                eprintln!("{}", e);
                false
            }
        };
        println!("test {} ... {}", name, if ok { "ok" } else { "FAILED" });
        if !ok {
            failed.push(name);
        }
    }
    println!("\n{} passed, {} failed", tests.len() - failed.len(), failed.len());
    Ok(if failed.is_empty() { 0 } else { 1 })
}
