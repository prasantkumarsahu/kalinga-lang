//! Every built-in function, library function, library constant and method of a built-in
//! type, with its signature and documentation.
//!
//! This table is what editors show (completion, hover, signature help) and what the
//! reference documentation lists; tests check that the type checker knows every entry and
//! that `docs/` lists exactly these.

/// A function: `name` is `print` for built-ins and `Math.sqrt` for library functions.
#[derive(Debug, Clone, Copy)]
pub struct Function {
    pub name: &'static str,
    /// Parameters as written in Kalinga, e.g. `x: float`.
    pub params: &'static [&'static str],
    pub ret: &'static str,
    pub doc: &'static str,
}

/// A method of a built-in type.
#[derive(Debug, Clone, Copy)]
pub struct Method {
    /// The receiver's type as written in the docs: `[T]`, `{K: V}`, `str`, `Result<T, E>`,
    /// `Task<T>`, `Channel<T>` or `Shared<T>`.
    pub receiver: &'static str,
    pub name: &'static str,
    pub params: &'static [&'static str],
    pub ret: &'static str,
    pub doc: &'static str,
    /// Needs a variable it can change (`var xs`).
    pub mutates: bool,
}

/// A library constant such as `Math.PI`.
#[derive(Debug, Clone, Copy)]
pub struct Constant {
    pub name: &'static str,
    pub ty: &'static str,
    pub doc: &'static str,
}

/// The library names, used as receivers: `Math.sqrt(x)`.
pub const LIBRARIES: &[&str] = &["Math", "Random", "Time", "Thread", "Async"];

const fn f(name: &'static str, params: &'static [&'static str], ret: &'static str, doc: &'static str) -> Function {
    Function { name, params, ret, doc }
}

const fn m(
    receiver: &'static str,
    name: &'static str,
    params: &'static [&'static str],
    ret: &'static str,
    doc: &'static str,
) -> Method {
    Method { receiver, name, params, ret, doc, mutates: false }
}

const fn mutating(
    receiver: &'static str,
    name: &'static str,
    params: &'static [&'static str],
    ret: &'static str,
    doc: &'static str,
) -> Method {
    Method { receiver, name, params, ret, doc, mutates: true }
}

pub const FUNCTIONS: &[Function] = &[
    f(
        "print",
        &["values..."],
        "void",
        "Prints the values separated by spaces, without a newline; the text appears at once.",
    ),
    f(
        "println",
        &["values..."],
        "void",
        "Prints the values separated by spaces, then a newline. `println()` prints an empty line.",
    ),
    f("len", &["x: [T], {K: V} or str"], "int", "Number of elements in an array or map, or characters in a string."),
    f(
        "assert",
        &["condition: bool"],
        "void",
        "Stops the program (or fails the test) with the file and line when `condition` is false. `assert(condition, message)` adds `message`.",
    ),
    f(
        "assert_eq",
        &["value: T", "expected: T"],
        "void",
        "Stops the program (or fails the test) when `value` is not `expected`, showing both.",
    ),
    f("str", &["x"], "str", "Text form of a printable value."),
    f("int", &["x"], "int", "Converts a float (drops the fraction), bool or str to int."),
    f("float", &["x"], "float", "Converts an int or str to float."),
    f("input", &["prompt: str"], "str", "Prints the prompt and reads one line from the keyboard."),
    f("chr", &["code: int"], "str", "The character with this code point: `chr(65)` is `\"A\"`."),
    f("args", &[], "[str]", "The command-line arguments the program was started with (without its own name)."),
    f("Ok", &["value: T"], "Result<T, E>", "A successful `Result`."),
    f("Err", &["error: E"], "Result<T, E>", "A failed `Result`."),
    f("Channel", &[], "Channel<T>", "A queue for sending values between threads: `Channel<int>()`."),
    f("Shared", &["value: T"], "Shared<T>", "A value several threads can read and change: `Shared(0)`."),
    f("Math.sqrt", &["x: float"], "float", "Square root."),
    f("Math.pow", &["x: float", "y: float"], "float", "`x` to the power `y`."),
    f("Math.exp", &["x: float"], "float", "e to the power `x`."),
    f("Math.ln", &["x: float"], "float", "Natural logarithm."),
    f("Math.log10", &["x: float"], "float", "Base-10 logarithm."),
    f("Math.sin", &["x: float"], "float", "Sine (radians)."),
    f("Math.cos", &["x: float"], "float", "Cosine (radians)."),
    f("Math.tan", &["x: float"], "float", "Tangent (radians)."),
    f("Math.asin", &["x: float"], "float", "Arc sine."),
    f("Math.acos", &["x: float"], "float", "Arc cosine."),
    f("Math.atan", &["x: float"], "float", "Arc tangent."),
    f("Math.atan2", &["y: float", "x: float"], "float", "Angle of the point (x, y)."),
    f("Math.hypot", &["x: float", "y: float"], "float", "Length of the vector (x, y)."),
    f("Math.floor", &["x: float"], "float", "Round down."),
    f("Math.ceil", &["x: float"], "float", "Round up."),
    f("Math.round", &["x: float"], "float", "Round to nearest (halves away from zero)."),
    f("Math.lerp", &["a: float", "b: float", "t: float"], "float", "Linear interpolation: `a + (b - a) * t`."),
    f("Math.abs", &["x: int or float"], "int or float", "Absolute value (same type as `x`)."),
    f("Math.sign", &["x: int or float"], "int or float", "-1, 0 or 1 (same type as `x`)."),
    f("Math.min", &["a: T", "b: T"], "T", "Smaller of two ints or two floats."),
    f("Math.max", &["a: T", "b: T"], "T", "Larger of two ints or two floats."),
    f("Math.clamp", &["x: T", "lo: T", "hi: T"], "T", "`x` limited to `lo..hi`."),
    f("Random.int", &["lo: int", "hi: int"], "int", "A random int in `lo..hi` (`hi` excluded)."),
    f("Random.float", &[], "float", "A random float in `0.0..1.0`."),
    f("Random.chance", &["p: float"], "bool", "`true` with probability `p`."),
    f("Random.pick", &["items: [T]"], "T?", "A random element, or `none` if empty."),
    f("Random.shuffle", &["items: [T]"], "void", "Shuffles an array in place (needs `var`)."),
    f("Random.seed", &["n: int"], "void", "Makes the following random numbers repeatable."),
    f("Time.now", &[], "float", "Seconds since the program started (for frame timing)."),
    f("Time.millis", &[], "int", "Milliseconds since 1970 (wall clock)."),
    f("Time.sleep", &["seconds: float"], "void", "Pauses the program."),
    f(
        "Time.wait",
        &["seconds: float"],
        "Async<void>",
        "Pauses an `async fn` without blocking other jobs: `await Time.wait(0.5)`.",
    ),
    f(
        "Thread.spawn",
        &["job: fn() -> T"],
        "Task<T>",
        "Runs `job` on a new thread; `task.join()` gives its result. Captured values are copied; objects cannot cross threads.",
    ),
    f("Thread.cores", &[], "int", "Number of CPU cores."),
    f("Async.spawn", &["job: Async<T>"], "Async<T>", "Starts an async job in the background; `await` it later."),
    f("Async.all", &["jobs: [Async<T>]"], "Async<[T]>", "Waits for all jobs; the results are in the same order."),
    f("Async.run", &["job: Async<T>"], "T", "Runs an async job to the end from code that is not `async`."),
];

pub const CONSTANTS: &[Constant] = &[
    Constant { name: "Math.PI", ty: "float", doc: "π, half a turn in radians." },
    Constant { name: "Math.TAU", ty: "float", doc: "τ = 2π, a full turn in radians." },
    Constant { name: "Math.E", ty: "float", doc: "Euler's number e." },
    Constant { name: "Math.INF", ty: "float", doc: "Infinity." },
];

pub const METHODS: &[Method] = &[
    mutating("[T]", "push", &["x: T"], "void", "Adds `x` at the end."),
    mutating("[T]", "pop", &[], "T?", "Removes and returns the last element, or `none` if empty."),
    m("[T]", "len", &[], "int", "Number of elements."),
    m("[T]", "contains", &["x: T"], "bool", "Whether `x` is in the array (simple element types only)."),
    m("[T]", "map", &["f: fn(T) -> U"], "[U]", "A new array with `f` applied to each element."),
    m("[T]", "filter", &["f: fn(T) -> bool"], "[T]", "The elements for which `f` returns true."),
    m("[T]", "find", &["f: fn(T) -> bool"], "T?", "The first element for which `f` returns true."),
    mutating("[T]", "sort", &[], "void", "Sorts in place (int, float, str or bool elements)."),
    mutating(
        "[T]",
        "sort_by",
        &["f: fn(T, T) -> bool"],
        "void",
        "Sorts in place; `f(a, b)` is true when `a` comes first.",
    ),
    m("[T]", "par_map", &["f: fn(T) -> U"], "[U]", "Like `map`, split over all CPU cores (values only: no objects)."),
    m("{K: V}", "get", &["key: K"], "V?", "The value for `key`, or `none`."),
    m("{K: V}", "has", &["key: K"], "bool", "Whether `key` is present."),
    mutating("{K: V}", "remove", &["key: K"], "V?", "Removes `key` and returns its value."),
    m("{K: V}", "keys", &[], "[K]", "The keys, sorted."),
    m("{K: V}", "values", &[], "[V]", "The values, in key order."),
    m("{K: V}", "len", &[], "int", "Number of entries."),
    m("str", "len", &[], "int", "Number of characters."),
    m("str", "upper", &[], "str", "Uppercase copy."),
    m("str", "lower", &[], "str", "Lowercase copy."),
    m("str", "trim", &[], "str", "Copy without leading and trailing whitespace."),
    m("str", "contains", &["s: str"], "bool", "Whether `s` occurs in this string."),
    m("str", "split", &["sep: str"], "[str]", "The parts between each `sep` (characters, when `sep` is empty)."),
    m("str", "replace", &["from: str", "to: str"], "str", "A copy with every `from` replaced by `to`."),
    m("str", "starts_with", &["s: str"], "bool", "Whether this string begins with `s`."),
    m("str", "ends_with", &["s: str"], "bool", "Whether this string ends with `s`."),
    m("str", "find", &["s: str"], "int?", "The index of the first `s`, or `none`."),
    m("str", "code_at", &["i: int"], "int", "The code point of the character at index `i`."),
    m("str", "chars", &[], "[str]", "The characters, each as a one-character string."),
    m("str", "parse_int", &[], "int?", "The whole number this string holds, or `none`."),
    m("str", "parse_float", &[], "float?", "The number this string holds, or `none`."),
    m("[T]", "join", &["sep: str"], "str", "The elements joined with `sep` between them (arrays of `str`)."),
    m("Result<T, E>", "is_ok", &[], "bool", "Whether this is `Ok`."),
    m("Result<T, E>", "is_err", &[], "bool", "Whether this is `Err`."),
    m("Result<T, E>", "ok", &[], "T?", "The `Ok` value, or `none`."),
    m("Result<T, E>", "err", &[], "E?", "The `Err` value, or `none`."),
    m("Result<T, E>", "unwrap_or", &["fallback: T"], "T", "The `Ok` value, or `fallback`."),
    m("Result<T, E>", "unwrap", &[], "T", "The `Ok` value; stops the program with the error if it is `Err`."),
    m("Task<T>", "join", &[], "T", "Waits for the thread and returns its result."),
    m("Task<T>", "is_done", &[], "bool", "Whether the thread has finished."),
    m("Channel<T>", "send", &["value: T"], "void", "Puts a value in the channel."),
    m("Channel<T>", "receive", &[], "T?", "Waits for a value; `none` once the channel is closed and empty."),
    m("Channel<T>", "try_receive", &[], "T?", "A value if one is waiting, else `none` (never blocks)."),
    m("Channel<T>", "close", &[], "void", "No more values will be sent."),
    m("Shared<T>", "get", &[], "T", "A copy of the value."),
    m("Shared<T>", "set", &["value: T"], "void", "Replaces the value."),
    m(
        "Shared<T>",
        "update",
        &["f: fn(T) -> T"],
        "void",
        "Replaces the value with `f(value)`, with no other thread in between.",
    ),
];

/// The function called `name` (`print`, `Math.sqrt`, ...).
pub fn function(name: &str) -> Option<&'static Function> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// Methods of a built-in type, by its receiver spelling (`[T]`, `str`, ...).
pub fn methods_of(receiver: &str) -> impl Iterator<Item = &'static Method> + '_ {
    METHODS.iter().filter(move |m| m.receiver == receiver)
}

impl Function {
    /// `name(params) -> ret`, as editors show it.
    pub fn signature(&self) -> String {
        let ret = if self.ret == "void" { String::new() } else { format!(" -> {}", self.ret) };
        format!("{}({}){}", self.name, self.params.join(", "), ret)
    }
}

impl Method {
    /// `name(params) -> ret`, as editors show it.
    pub fn signature(&self) -> String {
        let ret = if self.ret == "void" { String::new() } else { format!(" -> {}", self.ret) };
        format!("{}({}){}", self.name, self.params.join(", "), ret)
    }
}
