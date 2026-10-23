use kalinga_driver as driver;

use std::path::{Path, PathBuf};
use std::process::exit;

const USAGE: &str = "Kalinga — a simple, safe, fast object-oriented language that compiles to Rust

usage:
  kalinga run   <file.kl>              compile and run
  kalinga build <file.kl> [-o <exe>]   compile to a native executable
  kalinga emit  <file.kl>              print the generated Rust code
  kalinga check <file.kl> [--stdin]    report errors only; `fn main()` is optional, so files
                                       meant to be imported can be checked (--stdin reads the
                                       source from stdin)
  kalinga fix   <file.kl or folder>...  update code written for an earlier Kalinga version
                                       (0.4 → 0.5: `print(...)` becomes `println(...)`,
                                       `static NAME: T = v` becomes `const NAME: T = v`)
  kalinga lsp                          run the language server (editors start it)
  kalinga -v | --version               print the installed version";

/// Errors from the compiler and the command's own messages, shown as text.
type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

fn read(file: &Path) -> CliResult<String> {
    std::fs::read_to_string(file).map_err(|e| format!("cannot read {}: {}", file.display(), e).into())
}

/// `kalinga fix`: updates each file (or every `.kl` file in each folder) in place.
fn fix(paths: &[String]) -> CliResult<i32> {
    for p in paths {
        let report = driver::fix::fix_path(Path::new(p))?;
        for (file, n) in &report.changed {
            println!("fixed {} place(s) in {}", n, file.display());
        }
        for e in &report.skipped {
            eprintln!("skipped a file that is not valid Kalinga:\n{}", e);
        }
    }
    Ok(0)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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

    let result: CliResult<i32> = match cmd {
        "run" => driver::run(&file).map_err(Into::into),
        "build" => {
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
            let out = out.or_else(|| Some(file.with_extension(std::env::consts::EXE_EXTENSION)));
            driver::build(&file, out).map_err(Into::into).map(|exe| {
                println!("built {}", exe.display());
                0
            })
        }
        "emit" => read(&file).and_then(|src| Ok(driver::to_rust(&file.display().to_string(), &src)?)).map(|rust| {
            print!("{}", rust);
            0
        }),
        "fix" => fix(&args[1..]),
        "check" => {
            let source = if args.get(2).map(String::as_str) == Some("--stdin") {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                    .map(|_| s)
                    .map_err(|e| format!("cannot read stdin: {}", e).into())
            } else {
                read(&file)
            };
            source.and_then(|src| Ok(driver::check(&file.display().to_string(), &src)?)).map(|_| 0)
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
