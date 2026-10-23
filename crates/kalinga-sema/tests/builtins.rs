//! The built-ins registry and the type checker agree: every registered function and method
//! type-checks when called with arguments of its parameter types.

use kalinga_sema::builtins::{self, FUNCTIONS, METHODS};
use kalinga_sema::{BUILTINS, Model, typeck};
use kalinga_syntax::{lexer, parser};

/// Type-checks a program; `Err` holds the messages.
fn check(source: &str) -> Result<(), Vec<String>> {
    let tokens = lexer::lex(source, 0).map_err(|e| vec![e.message])?;
    let prog = parser::parse(tokens).map_err(|es| es.into_iter().map(|e| e.message).collect::<Vec<_>>())?;
    let model = Model::build(&prog, true).map_err(|e| vec![e.message])?;
    typeck::check(&prog, &model).map(|_| ()).map_err(|es| es.into_iter().map(|e| e.message).collect())
}

/// An argument for a parameter written like `x: float` or `f: fn(T) -> bool`, where the
/// type parameters stand for `int`.
fn argument(param: &str) -> &'static str {
    let ty = param.split_once(": ").map_or(param, |(_, t)| t);
    match ty {
        "float" | "int or float" => "1.0",
        "int" | "T" | "values..." | "x" => "1",
        "str" | "K" | "[T], {K: V} or str" => "\"k\"",
        "[T]" => "items",
        "fn() -> T" => "fn() => 1",
        "fn(T) -> U" | "fn(T) -> T" => "fn(v) => v",
        "fn(T) -> bool" => "fn(v) => true",
        "fn(T, T) -> bool" => "fn(a, b) => true",
        "Async<T>" => "job()",
        "[Async<T>]" => "[job()]",
        "E" => "\"e\"",
        other => panic!("no test argument for parameter type `{}`", other),
    }
}

fn call(name: &str, params: &[&str]) -> String {
    let args: Vec<&str> = params.iter().map(|p| argument(p)).collect();
    format!("{}({})", name, args.join(", "))
}

const PRELUDE: &str =
    "async fn job() -> int {\n    return 1\n}\n\nfn res() -> Result<int, str> {\n    return Ok(1)\n}\n";

#[test]
fn every_function_type_checks() {
    for f in FUNCTIONS {
        let expr = match f.name {
            "Channel" => "Channel<int>()".to_string(),
            name => call(name, f.params),
        };
        let stmt = match f.name {
            "Ok" | "Err" => format!("let r: Result<int, str> = {}", expr),
            _ if f.ret == "void" => expr,
            _ => format!("let r = {}", expr),
        };
        let source = format!("{}\nfn main() {{\n    var items = [1]\n    {}\n}}\n", PRELUDE, stmt);
        if let Err(errors) = check(&source) {
            panic!("`{}` does not type-check:\n{}\n{}", f.name, errors.join("\n"), source);
        }
    }
}

#[test]
fn every_method_type_checks() {
    let receivers = [
        ("[T]", "items"),
        ("{K: V}", "map"),
        ("str", "text"),
        ("Result<T, E>", "result"),
        ("Task<T>", "task"),
        ("Channel<T>", "channel"),
        ("Shared<T>", "shared"),
    ];
    for m in METHODS {
        let Some((_, var)) = receivers.iter().find(|(r, _)| *r == m.receiver) else {
            panic!("no test receiver for `{}`", m.receiver);
        };
        // `join` needs an array of strings.
        let var = if (m.receiver, m.name) == ("[T]", "join") { "words" } else { var };
        let expr = format!("{}.{}", var, call(m.name, m.params));
        let stmt = if m.ret == "void" { expr } else { format!("let r = {}", expr) };
        let source = format!(
            "{}\nfn main() {{\n    var items = [1]\n    let words = [\"w\"]\n    var map = {{\"k\": 1}}\n    let text = \"t\"\n    let result = res()\n    let task = Thread.spawn(fn() => 1)\n    let channel = Channel<int>()\n    let shared = Shared(1)\n    {}\n}}\n",
            PRELUDE, stmt
        );
        if let Err(errors) = check(&source) {
            panic!("`{}.{}` does not type-check:\n{}\n{}", m.receiver, m.name, errors.join("\n"), source);
        }
    }
}

#[test]
fn unknown_library_functions_are_rejected() {
    let errors = check("fn main() {\n    let x = Math.nope(1.0)\n}\n").unwrap_err();
    assert_eq!(errors, vec!["`Math` has no function `nope`"]);
}

#[test]
fn plain_builtins_are_registered() {
    for name in BUILTINS {
        assert!(builtins::function(name).is_some(), "`{}` is not in the registry", name);
    }
}
