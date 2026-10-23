use kalinga_lang::driver;

use std::path::PathBuf;
use std::process::exit;

const USAGE: &str = "Kalinga — a simple, safe, fast object-oriented language that compiles to Rust

usage:
  kalinga run   <file.kl>              compile and run
  kalinga build <file.kl> [-o <exe>]   compile to a native executable
  kalinga emit  <file.kl>              print the generated Rust code
  kalinga check <file.kl> [--stdin]    report errors only; `fn main()` is optional, so files
                                       meant to be imported can be checked (--stdin reads the
                                       source from stdin)
  kalinga -v | --version               print the installed version";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some("-v" | "-version" | "--version" | "version") = args.first().map(String::as_str) {
        println!("kalinga {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let (cmd, file) = match (args.first(), args.get(1)) {
        (Some(c), Some(f)) => (c.as_str(), PathBuf::from(f)),
        _ => {
            eprintln!("{}", USAGE);
            exit(2);
        }
    };

    let result = match cmd {
        "run" => driver::run(&file),
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
            driver::build(&file, out).map(|exe| {
                println!("built {}", exe.display());
                0
            })
        }
        "emit" => std::fs::read_to_string(&file)
            .map_err(|e| format!("cannot read {}: {}", file.display(), e))
            .and_then(|src| driver::to_rust(&file.display().to_string(), &src))
            .map(|rust| {
                print!("{}", rust);
                0
            }),
        "check" => {
            let source = if args.get(2).map(String::as_str) == Some("--stdin") {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                    .map(|_| s)
                    .map_err(|e| format!("cannot read stdin: {}", e))
            } else {
                std::fs::read_to_string(&file).map_err(|e| format!("cannot read {}: {}", file.display(), e))
            };
            source.and_then(|src| driver::check(&file.display().to_string(), &src)).map(|_| 0)
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
