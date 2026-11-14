use kalinga_driver as driver;

use std::path::{Path, PathBuf};
use std::process::exit;

const USAGE: &str = "Kalinga — a simple, safe, fast object-oriented language that compiles to Rust

usage:
  kalinga run   <file.kl>              compile and run
  kalinga build <file.kl> [-o <exe>]   compile to a native executable
  kalinga test  <file.kl>              run the file's `test fn`s and report which fail
  kalinga emit  <file.kl>              print the generated Rust code
                --web                  with run, build, test or emit: compile to JavaScript
                                       (run and test use Node.js; build writes a folder,
                                       `dist` by default, with index.html, app.js and the
                                       .css files next to the program); not needed in a
                                       utkal project with `target = \"web\"`
  kalinga serve <file.kl> [-o <dir>] [--port <n>]
                                       build for the web, serve it at http://localhost:8080
                                       and build again (reloading the page) on every change
  kalinga check <file.kl> [--stdin]    report errors only; `fn main()` is optional, so files
                                       meant to be imported can be checked (--stdin reads the
                                       source from stdin)
  kalinga fix   [--from <version>] <file.kl or folder>...
                                       update code written for an earlier Kalinga version
                                       (the nearest utkal.toml says which, or --from 0.4);
                                       0.4 → 0.5: `print(...)` becomes `println(...)`,
                                       `static NAME: T = v` becomes `const NAME: T = v`
  kalinga fmt   [--check] <file.kl or folder>...  lay out code in the standard style
                                       (--check lists the files it would change, and fails)
  kalinga lsp                          run the language server (editors start it)
  kalinga -v | --version               print the installed version";

/// Errors from the compiler and the command's own messages, shown as text.
type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

fn read(file: &Path) -> CliResult<String> {
    std::fs::read_to_string(file).map_err(|e| format!("cannot read {}: {}", file.display(), e).into())
}

/// `kalinga fmt [--check]`: formats each file (or every `.kl` file in each folder) in
/// place, or with `--check` only lists the files that are not formatted.
fn fmt(args: &[String]) -> CliResult<i32> {
    let check = args.iter().any(|a| a == "--check");
    let mut code = 0;
    for p in args.iter().filter(|a| *a != "--check") {
        let report = driver::fmt::fmt_path(Path::new(p), check)?;
        for file in &report.changed {
            println!("{} {}", if check { "not formatted:" } else { "formatted" }, file.display());
        }
        for e in &report.failed {
            eprintln!("cannot format a file that is not valid Kalinga:\n{}", e);
        }
        if !report.failed.is_empty() || (check && !report.changed.is_empty()) {
            code = 1;
        }
    }
    Ok(code)
}

/// `kalinga fix [--from <version>]`: updates each file (or every `.kl` file in each
/// folder) in place. The code's version comes from `--from`, else the nearest
/// `utkal.toml`; rules that only apply to older code are skipped when it is unknown.
fn fix(args: &[String]) -> CliResult<i32> {
    let mut from = None;
    let mut paths = vec![];
    let mut rest = args.iter();
    while let Some(a) = rest.next() {
        if a == "--from" {
            let v = rest.next().ok_or("`--from` needs a version, e.g. `--from 0.4`")?;
            from = Some(driver::fix::parse_version(v).ok_or_else(|| format!("`{}` is not a version like `0.4`", v))?);
        } else {
            paths.push(a);
        }
    }
    for p in paths {
        let path = Path::new(p);
        let report = driver::fix::fix_path(path, from.or_else(|| driver::fix::project_version(path)))?;
        for (file, n) in &report.changed {
            println!("fixed {} place(s) in {}", n, file.display());
        }
        for (file, n) in &report.held_back {
            println!(
                "left {} `print(...)` call(s) in {} as they are; if this code was written for 0.4, run `kalinga fix --from 0.4 {}`",
                n,
                file.display(),
                p
            );
        }
        for e in &report.skipped {
            eprintln!("skipped a file that is not valid Kalinga:\n{}", e);
        }
    }
    Ok(0)
}

/// `kalinga build <file.kl> [-o <out>]`: an executable, or with `web` a `dist` folder.
fn build(args: &[String], file: &Path, web: bool) -> CliResult<i32> {
    let out = match args.get(2).map(String::as_str) {
        Some("-o") => match args.get(3) {
            Some(o) => Some(PathBuf::from(o)),
            None => {
                eprintln!("`-o` needs a file name");
                exit(2);
            }
        },
        Some(other) => {
            eprintln!("unexpected argument `{}`\n\n{}", other, USAGE);
            exit(2);
        }
        None => None,
    };
    if web {
        let dir = driver::build_web(file, out)?;
        println!("built {}", dir.join("index.html").display());
    } else {
        let out = out.or_else(|| Some(file.with_extension(std::env::consts::EXE_EXTENSION)));
        let exe = driver::build(file, out)?;
        println!("built {}", exe.display());
    }
    Ok(0)
}
fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut web = args.iter().any(|a| a == "--web");
    args.retain(|a| a != "--web");
    let port = match args.iter().position(|a| a == "--port") {
        Some(i) => {
            let Some(n) = args.get(i + 1).and_then(|p| p.parse::<u16>().ok()) else {
                eprintln!("`--port` needs a port number, e.g. `--port 3000`");
                exit(2);
            };
            args.drain(i..i + 2);
            n
        }
        None => 8080,
    };
    if let Some("-v" | "-version" | "--version" | "version") = args.first().map(String::as_str) {
        println!("kalinga {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.first().map(String::as_str) == Some("lsp") {
        if let Err(e) = kalinga_lsp::run() {
            eprintln!("kalinga lsp: {}", e);
            exit(1);
        }
        return;
    }
    let (cmd, file) = match (args.first(), args.get(1)) {
        (Some(c), Some(f)) => (c.as_str(), PathBuf::from(f)),
        _ => {
            eprintln!("{}", USAGE);
            exit(2);
        }
    };

    // A file in a web project builds for the web without `--web`.
    if matches!(cmd, "run" | "test" | "build" | "emit") && driver::project_backend(&file) == Some(driver::Backend::Web)
    {
        web = true;
    }
    let result: CliResult<i32> = match cmd {
        "serve" => {
            let out = match args.get(2).map(String::as_str) {
                Some("-o") => args.get(3).map(PathBuf::from),
                _ => None,
            };
            driver::web::serve(&file, out, port).map_err(Into::into).map(|_| 0)
        }
        "run" if web => driver::run_web(&file, false).map_err(Into::into),
        "test" if web => driver::run_web(&file, true).map_err(Into::into),
        "run" => driver::run(&file).map_err(Into::into),
        "test" => driver::test(&file).map_err(Into::into),
        "build" => build(&args, &file, web),
        "emit" if web => {
            read(&file).and_then(|src| Ok(driver::to_js(&file.display().to_string(), &src, false)?)).map(|js| {
                print!("{}", js);
                0
            })
        }
        "emit" => read(&file).and_then(|src| Ok(driver::to_rust(&file.display().to_string(), &src)?)).map(|rust| {
            print!("{}", rust);
            0
        }),
        "fix" => fix(&args[1..]),
        "fmt" => fmt(&args[1..]),
        "check" => {
            let source = if args.get(2).map(String::as_str) == Some("--stdin") {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                    .map(|_| s)
                    .map_err(|e| format!("cannot read stdin: {}", e).into())
            } else {
                read(&file)
            };
            let name = file.display().to_string();
            source
                .and_then(|src| match driver::project_backend(&file) {
                    Some(backend) => Ok(driver::check_for(&name, &src, backend)?),
                    None => Ok(driver::check(&name, &src)?),
                })
                .map(|_| 0)
        }
        _ => {
            eprintln!("unknown command `{}`\n\n{}", cmd, USAGE);
            exit(2);
        }
    };

    match result {
        Ok(code) => exit(code),
        Err(msg) => {
            eprintln!("{}", msg);
            exit(1);
        }
    }
}
