// Kalinga code completion and signature help.
//
// Works on raw source text (no VS Code dependency) so it can be unit-tested with node and
// keeps working while the file is half-typed and would not compile.

"use strict";

const KEYWORDS = [
  "let", "var", "if", "else", "while", "for", "in", "match", "return", "break", "continue",
  "true", "false", "none", "fn", "as", "is", "await",
];
const PRIMITIVES = ["int", "float", "bool", "str", "void"];

const f = (name, params, ret, doc) => ({
  name,
  params: params.map(([n, t]) => ({ name: n, type: t })),
  ret,
  doc,
  owner: null,
});

/** Built-in library objects: `Math.sqrt(x)`, `Random.int(lo, hi)`, `Time.now()`. */
const LIBRARY = {
  Math: {
    values: { PI: "float", TAU: "float", E: "float", INF: "float" },
    fns: [
      f("sqrt", [["x", "float"]], "float", "Square root."),
      f("pow", [["x", "float"], ["y", "float"]], "float", "`x` to the power `y`."),
      f("abs", [["x", "int or float"]], "int or float", "Absolute value (same type as `x`)."),
      f("sign", [["x", "int or float"]], "int or float", "-1, 0 or 1 (same type as `x`)."),
      f("min", [["a", "T"], ["b", "T"]], "T", "Smaller of two ints or two floats."),
      f("max", [["a", "T"], ["b", "T"]], "T", "Larger of two ints or two floats."),
      f("clamp", [["x", "T"], ["lo", "T"], ["hi", "T"]], "T", "`x` limited to `lo..hi`."),
      f("lerp", [["a", "float"], ["b", "float"], ["t", "float"]], "float", "Linear interpolation: `a + (b - a) * t`."),
      f("floor", [["x", "float"]], "float", "Round down."),
      f("ceil", [["x", "float"]], "float", "Round up."),
      f("round", [["x", "float"]], "float", "Round to nearest (halves away from zero)."),
      f("sin", [["x", "float"]], "float", "Sine (radians)."),
      f("cos", [["x", "float"]], "float", "Cosine (radians)."),
      f("tan", [["x", "float"]], "float", "Tangent (radians)."),
      f("asin", [["x", "float"]], "float", "Arc sine."),
      f("acos", [["x", "float"]], "float", "Arc cosine."),
      f("atan", [["x", "float"]], "float", "Arc tangent."),
      f("atan2", [["y", "float"], ["x", "float"]], "float", "Angle of the point (x, y)."),
      f("hypot", [["x", "float"], ["y", "float"]], "float", "Length of the vector (x, y)."),
      f("exp", [["x", "float"]], "float", "e to the power `x`."),
      f("ln", [["x", "float"]], "float", "Natural logarithm."),
      f("log10", [["x", "float"]], "float", "Base-10 logarithm."),
    ],
  },
  Random: {
    values: {},
    fns: [
      f("int", [["lo", "int"], ["hi", "int"]], "int", "A random int in `lo..hi` (`hi` excluded)."),
      f("float", [], "float", "A random float in `0.0..1.0`."),
      f("chance", [["p", "float"]], "bool", "`true` with probability `p`."),
      f("pick", [["items", "[T]"]], "T?", "A random element, or `none` if empty."),
      f("shuffle", [["items", "[T]"]], "void", "Shuffles an array in place (needs `var`)."),
      f("seed", [["n", "int"]], "void", "Makes the following random numbers repeatable."),
    ],
  },
  Time: {
    values: {},
    fns: [
      f("now", [], "float", "Seconds since the program started (for frame timing)."),
      f("millis", [], "int", "Milliseconds since 1970 (wall clock)."),
      f("sleep", [["seconds", "float"]], "void", "Pauses the program."),
      f("wait", [["seconds", "float"]], "Async<void>", "Pauses an `async fn` without blocking other jobs: `await Time.wait(0.5)`."),
    ],
  },
  Thread: {
    values: {},
    fns: [
      f("spawn", [["job", "fn() -> T"]], "Task<T>", "Runs `job` on a new thread; `task.join()` gives its result. Captured values are copied; objects cannot cross threads."),
      f("cores", [], "int", "Number of CPU cores."),
    ],
  },
  Async: {
    values: {},
    fns: [
      f("spawn", [["job", "Async<T>"]], "Async<T>", "Starts an async job in the background; `await` it later."),
      f("all", [["jobs", "[Async<T>]"]], "Async<[T]>", "Waits for all jobs; the results are in the same order."),
      f("run", [["job", "Async<T>"]], "T", "Runs an async job to the end from code that is not `async`."),
    ],
  },
};

/** Built-in generic types and their methods. */
const BUILTIN_TYPES = {
  Result: { params: ["T", "E"], doc: "`Ok(value)` or `Err(error)`; unwrap with `match` or `?`." },
  Channel: { params: ["T"], doc: "A queue for sending values between threads: `Channel<int>()`." },
  Shared: { params: ["T"], doc: "A value several threads can read and change: `Shared<int>(0)`." },
  Task: { params: ["T"], doc: "A running thread, from `Thread.spawn`." },
  Async: { params: ["T"], doc: "A running async job, from calling an `async fn`." },
};

function builtinTypeMethods(name, args) {
  const p = (n, type) => ({ name: n, type });
  const [t, e] = [args[0] || "T", args[1] || "E"];
  switch (name) {
    case "Result":
      return [
        { name: "is_ok", params: [], ret: "bool", doc: "Whether this is `Ok`." },
        { name: "is_err", params: [], ret: "bool", doc: "Whether this is `Err`." },
        { name: "ok", params: [], ret: t + "?", doc: "The `Ok` value, or `none`." },
        { name: "err", params: [], ret: e + "?", doc: "The `Err` value, or `none`." },
        { name: "unwrap_or", params: [p("fallback", t)], ret: t, doc: "The `Ok` value, or `fallback`." },
        { name: "unwrap", params: [], ret: t, doc: "The `Ok` value; stops the program with the error if it is `Err`." },
      ];
    case "Channel":
      return [
        { name: "send", params: [p("value", t)], ret: "void", doc: "Puts a value in the channel." },
        { name: "receive", params: [], ret: t + "?", doc: "Waits for a value; `none` once the channel is closed and empty." },
        { name: "try_receive", params: [], ret: t + "?", doc: "A value if one is waiting, else `none` (never blocks)." },
        { name: "close", params: [], ret: "void", doc: "No more values will be sent." },
      ];
    case "Shared":
      return [
        { name: "get", params: [], ret: t, doc: "A copy of the value." },
        { name: "set", params: [p("value", t)], ret: "void", doc: "Replaces the value." },
        { name: "update", params: [p("f", `fn(${t}) -> ${t}`)], ret: "void", doc: "Replaces the value with `f(value)`, with no other thread in between." },
      ];
    case "Task":
      return [
        { name: "join", params: [], ret: t, doc: "Waits for the thread and returns its result." },
        { name: "is_done", params: [], ret: "bool", doc: "Whether the thread has finished." },
      ];
    default:
      return [];
  }
}

const BUILTINS = {
  print: { params: ["values..."], ret: "void", doc: "Prints the values separated by spaces, then a newline." },
  len: { params: ["x: [T], {K: V} or str"], ret: "int", doc: "Number of elements in an array or map, or characters in a string." },
  str: { params: ["x"], ret: "str", doc: "Text form of a printable value." },
  int: { params: ["x"], ret: "int", doc: "Converts a float (drops the fraction), bool or str to int." },
  float: { params: ["x"], ret: "float", doc: "Converts an int or str to float." },
  input: { params: ["prompt: str"], ret: "str", doc: "Prints the prompt and reads one line from the keyboard." },
};

const STR_METHODS = [
  { name: "len", params: [], ret: "int", doc: "Number of characters." },
  { name: "upper", params: [], ret: "str", doc: "Uppercase copy." },
  { name: "lower", params: [], ret: "str", doc: "Lowercase copy." },
  { name: "trim", params: [], ret: "str", doc: "Copy without leading and trailing whitespace." },
  { name: "contains", params: [{ name: "s", type: "str" }], ret: "bool", doc: "Whether `s` occurs in this string." },
];

function arrayMethods(elem) {
  const p = (name, type) => ({ name, type });
  return [
    { name: "push", params: [p("x", elem)], ret: "void", doc: "Adds `x` at the end (needs a `var` array)." },
    { name: "pop", params: [], ret: elem + "?", doc: "Removes and returns the last element, or `none` if empty." },
    { name: "len", params: [], ret: "int", doc: "Number of elements." },
    { name: "contains", params: [p("x", elem)], ret: "bool", doc: "Whether `x` is in the array." },
    { name: "map", params: [p("f", `fn(${elem}) -> U`)], ret: "[U]", doc: "A new array with `f` applied to each element." },
    { name: "filter", params: [p("f", `fn(${elem}) -> bool`)], ret: `[${elem}]`, doc: "The elements for which `f` returns true." },
    { name: "find", params: [p("f", `fn(${elem}) -> bool`)], ret: elem + "?", doc: "The first element for which `f` returns true." },
    { name: "sort", params: [], ret: "void", doc: "Sorts in place (int, float, str, bool; needs `var`)." },
    { name: "sort_by", params: [p("f", `fn(${elem}, ${elem}) -> bool`)], ret: "void", doc: "Sorts in place; `f(a, b)` is true when `a` comes first." },
    { name: "par_map", params: [p("f", `fn(${elem}) -> U`)], ret: "[U]", doc: "Like `map`, split over all CPU cores (values only: no objects)." },
  ];
}

function mapMethods(k, v) {
  const p = (name, type) => ({ name, type });
  return [
    { name: "get", params: [p("key", k)], ret: v + "?", doc: "The value for `key`, or `none`." },
    { name: "has", params: [p("key", k)], ret: "bool", doc: "Whether `key` is present." },
    { name: "remove", params: [p("key", k)], ret: v + "?", doc: "Removes `key` and returns its value (needs `var`)." },
    { name: "keys", params: [], ret: `[${k}]`, doc: "The keys, sorted." },
    { name: "values", params: [], ret: `[${v}]`, doc: "The values, in key order." },
    { name: "len", params: [], ret: "int", doc: "Number of entries." },
  ];
}

// ---------------------------------------------------------------- text helpers

/**
 * Blanks out comments and string contents (keeping quotes) so offsets stay valid.
 * `{...}` interpolations inside strings stay visible, since they are code.
 */
function mask(text) {
  const out = text.split("");
  const hidden = new Uint8Array(text.length);
  let i = 0;
  const blank = (j) => {
    if (out[j] !== "\n") out[j] = " ";
    hidden[j] = 1;
  };
  while (i < text.length) {
    const c = text[i];
    if (c === "/" && text[i + 1] === "/") {
      while (i < text.length && text[i] !== "\n") blank(i++);
    } else if (c === "/" && text[i + 1] === "*") {
      const end = text.indexOf("*/", i + 2);
      const stop = end < 0 ? text.length : end + 2;
      while (i < stop) blank(i++);
    } else if (c === '"') {
      i++;
      while (i < text.length && text[i] !== '"' && text[i] !== "\n") {
        if (text[i] === "\\" && i + 1 < text.length && text[i + 1] !== "\n") {
          blank(i++);
          blank(i++);
        } else if (text[i] === "{" && text[i + 1] === "{") {
          blank(i++);
          blank(i++);
        } else if (text[i] === "{") {
          // Interpolation: keep the braces out of brace counting, keep the code.
          blank(i++);
          let d = 0;
          while (i < text.length && text[i] !== "\n" && !(text[i] === "}" && d === 0)) {
            if (text[i] === "{") d++;
            else if (text[i] === "}") d--;
            else if (text[i] === '"') {
              // A string inside the interpolation.
              i++;
              while (i < text.length && text[i] !== '"' && text[i] !== "\n") i++;
            }
            i++;
          }
          if (text[i] === "}") blank(i++);
        } else {
          blank(i++);
        }
      }
      i++;
    } else {
      i++;
    }
  }
  return { code: out.join(""), hidden };
}

/** depth[i] = number of unclosed `{` before position i. */
function braceDepths(code) {
  const depth = new Int32Array(code.length + 1);
  let d = 0;
  for (let i = 0; i < code.length; i++) {
    depth[i] = d;
    if (code[i] === "{") d++;
    else if (code[i] === "}") d = Math.max(0, d - 1);
  }
  depth[code.length] = d;
  return depth;
}

/** Index just past the bracket matching the one at `open`, or the end of text. */
function matchClose(code, open) {
  const pairs = { "{": "}", "(": ")", "[": "]", "<": ">" };
  const o = code[open];
  const c = pairs[o];
  let d = 0;
  for (let i = open; i < code.length; i++) {
    if (code[i] === o) d++;
    else if (code[i] === c && --d === 0) return i + 1;
  }
  return code.length;
}

const matchBrace = (code, open) => matchClose(code, open);

function skipSpaces(code, i) {
  while (i < code.length && (code[i] === " " || code[i] === "\t")) i++;
  return i;
}

/** End index of a type written at `i` (`int`, `[T]`, `{K: V}`, `fn(A) -> B`, `Stack<int>?`). */
function readType(code, i) {
  i = skipSpaces(code, i);
  if (code.startsWith("fn", i) && code[skipSpaces(code, i + 2)] === "(") {
    i = matchClose(code, skipSpaces(code, i + 2));
    const j = skipSpaces(code, i);
    if (code.startsWith("->", j)) i = readType(code, j + 2);
    return i;
  }
  if (code[i] === "[" || code[i] === "{") i = matchClose(code, i);
  else {
    while (i < code.length && /\w/.test(code[i])) i++;
    if (code[i] === "<") i = matchClose(code, i);
  }
  if (code[i] === "?") i++;
  return i;
}

/** Splits at top-level occurrences of `sep` (outside (), [], {}, <>). */
function splitTop(src, sep) {
  const parts = [];
  let d = 0;
  let cur = "";
  for (let i = 0; i < src.length; i++) {
    const ch = src[i];
    if ("([{<".includes(ch)) d++;
    else if (")]}".includes(ch) || (ch === ">" && src[i - 1] !== "-")) d--;
    if (ch === sep && d === 0) {
      parts.push(cur);
      cur = "";
    } else cur += ch;
  }
  parts.push(cur);
  return parts;
}

/** `// comment` lines directly above `index`, joined. */
/**
 * The `///` doc comment written just above a declaration, as Markdown. Plain `//` comments
 * are notes for the code's readers and are not documentation.
 */
function docAbove(text, index) {
  const lines = text.slice(0, index).split("\n");
  lines.pop(); // the declaration's own line
  const doc = [];
  while (lines.length) {
    const l = lines[lines.length - 1].trimStart();
    if (!l.startsWith("///") || l.startsWith("////")) break;
    doc.unshift(l.slice(3).replace(/^ /, "").trimEnd());
    lines.pop();
  }
  return doc.join("\n");
}

function cleanType(t) {
  return t ? t.replace(/\s+/g, " ").replace(/\s*([<>\[\]{}(),:?])\s*/g, "$1").replace(/->/g, " -> ").replace(/,/g, ", ").replace(/:/g, ": ").trim() : "void";
}

function parseParams(src) {
  return splitTop(src, ",")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => {
      const m = /^([A-Za-z_]\w*)\s*(?::\s*([\s\S]+))?$/.exec(p);
      return m ? { name: m[1], type: m[2] ? cleanType(m[2]) : null } : { name: p, type: null };
    });
}

/** Parses `fn name<T>(params) -> ret` starting at the `fn` keyword index. */
function parseFnHeader(code, at) {
  let i = skipSpaces(code, at + 2);
  const nm = /^[A-Za-z_]\w*/.exec(code.slice(i));
  if (!nm) return null;
  const name = nm[0];
  i = skipSpaces(code, i + name.length);
  let tparams = [];
  if (code[i] === "<") {
    const end = matchClose(code, i);
    tparams = code.slice(i + 1, end - 1).split(",").map((s) => s.trim()).filter(Boolean);
    i = skipSpaces(code, end);
  }
  if (code[i] !== "(") return null;
  const close = matchClose(code, i);
  const params = parseParams(code.slice(i + 1, close - 1));
  i = skipSpaces(code, close);
  let ret = "void";
  if (code.startsWith("->", i)) {
    const start = skipSpaces(code, i + 2);
    const end = readType(code, start);
    ret = cleanType(code.slice(start, end));
    i = skipSpaces(code, end);
  }
  return { name, tparams, params, ret, bodyOpen: code[i] === "{" ? i : -1 };
}

/** Modifier words (`private`, `static`, ...) written just before `index` on the same line. */
function modifiersBefore(code, index) {
  const line = code.slice(code.lastIndexOf("\n", index - 1) + 1, index);
  return new Set(line.trim().split(/\s+/).filter(Boolean));
}

// ---------------------------------------------------------------- model

/**
 * Finds classes, interfaces, enums, functions, imports and function-like bodies.
 * Tolerates incomplete code: unclosed bodies simply run to the end of the text.
 */
function analyze(text, dir) {
  const { code, hidden } = mask(text);
  const depth = braceDepths(code);
  const model = {
    text, code, hidden, depth, dir: dir || "",
    imports: [], classes: new Map(), interfaces: new Map(), enums: new Map(), functions: new Map(), bodies: [],
    externTypes: new Set(),
  };
  for (const m of code.matchAll(/\bextern\s+type\s+([A-Za-z_]\w*)/g)) model.externTypes.add(m[1]);

  // The import path itself is masked (string contents); read it from the original text.
  // `import "pkg" as name` keeps the package's declarations under `name.`.
  model.aliases = new Map();
  for (const m of text.matchAll(/^[ \t]*import[ \t]+"([^"\n]+)"(?:[ \t]+as[ \t]+([A-Za-z_]\w*))?/gm)) {
    if (m[2]) model.aliases.set(m[2], m[1]);
    else model.imports.push(m[1]);
  }

  const fnsAt = (start, end, wantDepth) => {
    const out = [];
    const re = /\bfn\b/g;
    re.lastIndex = start;
    for (let m; (m = re.exec(code)) && m.index < end; ) {
      if (depth[m.index] !== wantDepth) continue;
      const h = parseFnHeader(code, m.index);
      if (h) out.push({ at: m.index, h, mods: modifiersBefore(code, m.index) });
    }
    return out;
  };

  // Classes
  for (const m of code.matchAll(/\b(class|struct)\s+([A-Za-z_]\w*)/g)) {
    if (depth[m.index] !== 0) continue;
    const isStruct = m[1] === "struct";
    m[1] = m[2];
    let i = skipSpaces(code, m.index + m[0].length);
    let tparams = [];
    if (code[i] === "<") {
      const end = matchClose(code, i);
      tparams = code.slice(i + 1, end - 1).split(",").map((s) => s.trim()).filter(Boolean);
      i = skipSpaces(code, end);
    }
    let bases = [];
    if (code[i] === ":") {
      const open = code.indexOf("{", i);
      bases = code.slice(i + 1, open < 0 ? code.length : open).split(",").map((s) => s.trim()).filter(Boolean);
      i = open < 0 ? code.length : open;
    }
    while (i < code.length && code[i] !== "{" && code[i] !== "\n") i++;
    if (code[i] !== "{") continue;
    const open = i;
    const end = matchBrace(code, open);
    const cls = {
      name: m[1], tparams, bases, struct: isStruct,
      abstract: modifiersBefore(code, m.index).has("abstract"),
      fields: [], methods: [], init: null,
      doc: docAbove(text, m.index), start: open, end,
    };
    const inner = depth[open] + 1;
    const fieldRe = /^[ \t]*((?:private[ \t]+)?)([a-z_]\w*)[ \t]*:/gm;
    fieldRe.lastIndex = open + 1;
    for (let f; (f = fieldRe.exec(code)) && f.index < end; ) {
      const at = f.index + f[0].indexOf(f[2]);
      if (depth[at] !== inner) continue;
      const tStart = f.index + f[0].length;
      const type = cleanType(code.slice(tStart, readType(code, tStart)));
      cls.fields.push({ name: f[2], type, private: !!f[1], doc: docAbove(text, at), owner: cls.name });
    }
    for (const { at, h, mods } of fnsAt(open + 1, end, inner)) {
      const method = {
        name: h.name, params: h.params, ret: h.ret, owner: cls.name, doc: docAbove(text, at),
        override: mods.has("override"), private: mods.has("private"), static: mods.has("static"), abstract: mods.has("abstract"),
        mut: mods.has("mut"),
      };
      cls.methods.push(method);
      if (h.bodyOpen >= 0) {
        model.bodies.push({ start: h.bodyOpen, end: matchBrace(code, h.bodyOpen), params: h.params, cls: method.static ? null : cls.name, owner: cls.name, kind: "method" });
      }
    }
    const initRe = /\binit\s*\(/g;
    initRe.lastIndex = open + 1;
    for (let f; (f = initRe.exec(code)) && f.index < end; ) {
      if (depth[f.index] !== inner) continue;
      const paren = f.index + f[0].length - 1;
      const close = matchClose(code, paren);
      cls.init = { params: parseParams(code.slice(paren + 1, close - 1)), doc: docAbove(text, f.index) };
      const bodyOpen = code.indexOf("{", close);
      if (bodyOpen >= 0) model.bodies.push({ start: bodyOpen, end: matchBrace(code, bodyOpen), params: cls.init.params, cls: cls.name, owner: cls.name, kind: "init" });
    }
    model.classes.set(cls.name, cls);
  }

  // Interfaces
  for (const m of code.matchAll(/\binterface\s+([A-Za-z_]\w*)\s*(?::\s*([\w\s,]*?))?\s*\{/g)) {
    if (depth[m.index] !== 0) continue;
    const open = m.index + m[0].length - 1;
    const end = matchBrace(code, open);
    const iface = {
      name: m[1], supers: (m[2] || "").split(",").map((s) => s.trim()).filter(Boolean),
      methods: [], doc: docAbove(text, m.index), start: open, end,
    };
    for (const { at, h } of fnsAt(open + 1, end, depth[open] + 1)) {
      iface.methods.push({ name: h.name, params: h.params, ret: h.ret, doc: docAbove(text, at), owner: iface.name });
    }
    model.interfaces.set(iface.name, iface);
  }

  // Enums
  for (const m of code.matchAll(/\benum\s+([A-Za-z_]\w*)\s*\{/g)) {
    if (depth[m.index] !== 0) continue;
    const open = m.index + m[0].length - 1;
    const end = matchBrace(code, open);
    const en = { name: m[1], variants: [], doc: docAbove(text, m.index), start: open, end };
    const body = code.slice(open + 1, end - 1);
    const vre = /\b([A-Z]\w*)\s*(\()?/g;
    for (let v; (v = vre.exec(body)); ) {
      let params = [];
      if (v[2]) {
        const p = open + 1 + v.index + v[0].length - 1;
        const close = matchClose(code, p);
        params = parseParams(code.slice(p + 1, close - 1));
        vre.lastIndex = close - (open + 1);
      }
      en.variants.push({ name: v[1], params });
    }
    model.enums.set(en.name, en);
  }

  // Top-level functions
  for (const { at, h, mods } of fnsAt(0, code.length, 0)) {
    if ([...mods].some((x) => x !== "extern" && x !== "async")) continue;
    const ret = mods.has("async") ? `Async<${h.ret || "void"}>` : h.ret;
    const fn = { name: h.name, tparams: h.tparams, params: h.params, ret, doc: docAbove(text, at), extern: mods.has("extern"), async: mods.has("async") };
    model.functions.set(fn.name, fn);
    if (h.bodyOpen >= 0) model.bodies.push({ start: h.bodyOpen, end: matchBrace(code, h.bodyOpen), params: h.params, cls: null, owner: null, kind: "fn" });
  }
  return model;
}

/**
 * Adds declarations from imported files (transitively). `load(path, dir)` returns
 * `{ text, dir }` for an import written in a file located in `dir`, or null.
 */
function withImports(model, load) {
  model.namespaces = new Map();
  if (!load) return model;
  const seen = new Set();
  const queue = model.imports.map((p) => [p, model.dir]);
  while (queue.length) {
    const [path, dir] = queue.shift();
    const res = load(path, dir);
    if (!res || seen.has(res.key || res.dir + "/" + path)) continue;
    seen.add(res.key || res.dir + "/" + path);
    const other = analyze(res.text, res.dir);
    for (const kind of ["classes", "interfaces", "enums", "functions"]) {
      for (const [k, v] of other[kind]) if (!model[kind].has(k)) model[kind].set(k, v);
    }
    for (const t of other.externTypes) model.externTypes.add(t);
    for (const p of other.imports) queue.push([p, res.dir]);
  }
  // `import "pkg" as name`: the package's declarations, reachable as `name.Thing`. Its
  // classes are also known as `name.Class`, so values like `name.Point(1, 2)` have a type.
  for (const [alias, path] of model.aliases) {
    const res = load(path, model.dir);
    if (!res) continue;
    const ns = withImports(analyze(res.text, res.dir), load);
    model.namespaces.set(alias, ns);
    for (const [k, v] of ns.classes) if (!model.classes.has(`${alias}.${k}`)) model.classes.set(`${alias}.${k}`, v);
    for (const [k, v] of ns.interfaces) if (!model.interfaces.has(`${alias}.${k}`)) model.interfaces.set(`${alias}.${k}`, v);
    for (const [k, v] of ns.enums) if (!model.enums.has(`${alias}.${k}`)) model.enums.set(`${alias}.${k}`, v);
  }
  return model;
}

/** Completions after `name.` for `import "pkg" as name`. */
function namespaceItems(ns) {
  const items = [];
  for (const f of ns.functions.values()) {
    items.push({ label: f.name, kind: "function", detail: `fn ${f.name}${sigText(f.params, f.ret)}`, documentation: f.doc, insertText: callSnippet(f.name, f.params), snippet: true, sortText: "1" + f.name });
  }
  for (const c of ns.classes.values()) {
    const init = findInit(ns, c.name);
    const params = c.struct ? c.fields.map((x) => ({ name: x.name, type: x.type })) : init ? init.params : [];
    items.push({ label: c.name, kind: c.abstract ? "class" : "constructor", detail: `${c.struct ? "struct" : "class"} ${c.name}${sigText(params, "")}`, documentation: c.doc, insertText: c.abstract ? c.name : callSnippet(c.name, params), snippet: !c.abstract, sortText: "2" + c.name });
  }
  for (const i of ns.interfaces.values()) items.push({ label: i.name, kind: "interface", detail: "interface " + i.name, documentation: i.doc, insertText: i.name, sortText: "2" + i.name });
  for (const e of ns.enums.values()) items.push({ label: e.name, kind: "enum", detail: "enum " + e.name, documentation: e.doc, insertText: e.name, sortText: "2" + e.name });
  return items;
}

// ---------------------------------------------------------------- class helpers

/** Class chain from `name` up to the root (classes only). */
function lineage(model, name) {
  const out = [];
  const seen = new Set();
  let cur = model.classes.get(name);
  while (cur && !seen.has(cur.name)) {
    seen.add(cur.name);
    out.push(cur);
    cur = cur.bases.map((b) => model.classes.get(b)).find(Boolean);
  }
  return out;
}

function parentOf(model, name) {
  const cls = model.classes.get(name);
  return cls ? cls.bases.find((b) => model.classes.has(b)) : undefined;
}

function visibleTo(member, viewer) {
  return !member.private || member.owner === viewer;
}

function classFields(model, name) {
  return lineage(model, name).flatMap((c) => c.fields);
}

/** Most-derived version of each instance method visible on a class. */
function classMethods(model, name) {
  const seen = new Map();
  for (const c of lineage(model, name)) {
    for (const m of c.methods) if (!m.static && !seen.has(m.name)) seen.set(m.name, m);
  }
  return [...seen.values()];
}

function staticMethods(model, name) {
  const seen = new Map();
  for (const c of lineage(model, name)) {
    for (const m of c.methods) if (m.static && !seen.has(m.name)) seen.set(m.name, m);
  }
  return [...seen.values()];
}

function interfaceMethods(model, name, seen = new Set()) {
  const iface = model.interfaces.get(name);
  if (!iface || seen.has(name)) return [];
  seen.add(name);
  return iface.methods.concat(...iface.supers.map((s) => interfaceMethods(model, s, seen)));
}

function findInit(model, name) {
  for (const c of lineage(model, name)) if (c.init) return { ...c.init, owner: c.name };
  return null;
}

// ---------------------------------------------------------------- types

const isArray = (t) => !!t && t.startsWith("[") && t.endsWith("]");
const elemOf = (t) => (isArray(t) ? t.slice(1, -1) : null);
const isMap = (t) => !!t && t.startsWith("{") && t.endsWith("}");
const unwrapOptional = (t) => (t && t.endsWith("?") ? t.slice(0, -1) : t);

function mapKV(t) {
  const [k, ...rest] = splitTop(t.slice(1, -1), ":");
  return [k.trim(), rest.join(":").trim()];
}

/** `Stack<int>` → ["Stack", ["int"]] */
function splitGeneric(t) {
  const m = /^([A-Za-z_]\w*)<(.*)>$/.exec(t || "");
  return m ? [m[1], splitTop(m[2], ",").map((s) => s.trim())] : [t, []];
}

/** Replaces a generic class's type parameters in `type` with the receiver's arguments. */
function substitute(model, receiverType, type) {
  const [name, args] = splitGeneric(receiverType);
  const cls = model.classes.get(name);
  if (!cls || !cls.tparams.length || !args.length || !type) return type;
  let out = type;
  cls.tparams.forEach((p, i) => {
    out = out.replace(new RegExp(`\\b${p}\\b`, "g"), args[i] || p);
  });
  return out;
}

/** Splits `a.b(c, d).e[0]` into top-level segments `a`, `b(c, d)`, `e[0]`. */
function splitChain(chain) {
  const parts = [];
  let d = 0;
  let cur = "";
  for (const ch of chain) {
    if ("([".includes(ch)) d++;
    if (")]".includes(ch)) d--;
    if (ch === "." && d === 0) {
      parts.push(cur.trim());
      cur = "";
    } else cur += ch;
  }
  parts.push(cur.trim());
  return parts;
}

/** Peels trailing `[...]` index operations: returns [base, count]. */
function stripIndexes(seg) {
  let n = 0;
  while (seg.endsWith("]")) {
    let d = 0;
    let i = seg.length - 1;
    for (; i >= 0; i--) {
      if (seg[i] === "]") d++;
      else if (seg[i] === "[" && --d === 0) break;
    }
    if (i <= 0) break;
    seg = seg.slice(0, i).trim();
    n++;
  }
  return [seg, n];
}

/** Methods (with signatures) available on a value of `type`. */
function methodsOf(model, type) {
  if (!type) return [];
  if (type.startsWith("lib:")) return LIBRARY[type.slice(4)].fns;
  if (type === "str") return STR_METHODS;
  if (isArray(type)) return arrayMethods(elemOf(type));
  if (isMap(type)) return mapMethods(...mapKV(type));
  if (model.interfaces.has(type)) return interfaceMethods(model, type);
  const [name, targs] = splitGeneric(type);
  if (BUILTIN_TYPES[name] && !model.classes.has(name)) return builtinTypeMethods(name, targs);
  if (model.classes.has(name)) {
    return classMethods(model, name).map((m) => ({
      ...m,
      params: m.params.map((p) => ({ ...p, type: substitute(model, type, p.type) })),
      ret: substitute(model, type, m.ret),
    }));
  }
  return [];
}

function indexType(type) {
  if (isArray(type)) return elemOf(type);
  if (isMap(type)) return mapKV(type)[1];
  return null;
}

function memberType(model, type, seg) {
  const call = /^([A-Za-z_]\w*)\s*\(/.exec(seg);
  const name = call ? call[1] : seg;
  if (type.startsWith("lib:") && !call) return LIBRARY[type.slice(4)].values[name] || null;
  if (call) {
    const m = methodsOf(model, type).find((x) => x.name === name);
    return m ? m.ret : null;
  }
  const [cname] = splitGeneric(type);
  if (model.classes.has(cname)) {
    const f = classFields(model, cname).find((x) => x.name === name);
    return f ? substitute(model, type, f.type) : null;
  }
  return null;
}

/** Best-effort static type of a receiver expression like `self.items[0].name`. */
function chainType(model, scope, chain) {
  let segs = splitChain(chain);
  let type = null;
  // `alias.Thing(...)` for `import "pkg" as alias`.
  const ns = segs.length > 1 && !scope.locals.has(segs[0]) && model.namespaces && model.namespaces.get(segs[0]);
  if (ns) {
    const [seg, indexes] = stripIndexes(segs[1]);
    type = atomType(ns, { locals: new Map() }, seg);
    if (type && ns.classes.has(splitGeneric(type)[0])) type = `${segs[0]}.${type}`;
    for (let k = 0; k < indexes && type; k++) type = indexType(type);
    if (!type) return null;
    segs = segs.slice(2);
    for (const s of segs) {
      const [seg2, idx2] = stripIndexes(s);
      type = memberType(model, unwrapOptional(type) === type ? type : null, seg2);
      for (let k = 0; k < idx2 && type; k++) type = indexType(type);
      if (!type) return null;
    }
    return type;
  }
  for (let i = 0; i < segs.length; i++) {
    const [seg, indexes] = stripIndexes(segs[i]);
    if (i === 0) type = atomType(model, scope, seg);
    else type = type ? memberType(model, unwrapOptional(type) === type ? type : null, seg) : null;
    for (let k = 0; k < indexes && type; k++) type = indexType(type);
    if (!type) return null;
  }
  return type;
}

function atomType(model, scope, seg) {
  if (seg === "self") {
    const cls = scope.cls && model.classes.get(scope.cls);
    return cls && cls.tparams.length ? `${cls.name}<${cls.tparams.join(", ")}>` : scope.cls;
  }
  if (seg === "super") return scope.cls ? parentOf(model, scope.cls) || null : null;
  if (/^"/.test(seg)) return "str";
  if (/^-?\d+\.\d+$/.test(seg)) return "float";
  if (/^-?\d+$/.test(seg)) return "int";
  if (seg === "true" || seg === "false") return "bool";
  if (/^\[/.test(seg)) {
    const first = splitTop(seg.slice(1, -1), ",")[0].trim();
    const t = first ? exprType(model, scope, first) : null;
    return t ? `[${t}]` : null;
  }
  if (/^\{/.test(seg)) {
    const entry = splitTop(seg.slice(1, -1), ",")[0];
    const [k, v] = entry ? splitTop(entry, ":") : [];
    const kt = k && exprType(model, scope, k);
    const vt = v && exprType(model, scope, v);
    return kt && vt ? `{${kt}: ${vt}}` : null;
  }
  const call = /^([A-Za-z_]\w*)\s*(<[^(]*>)?\s*\(/.exec(seg);
  if (call) {
    const n = call[1];
    const local = scope.locals.get(n);
    if (local && local.type && local.type.startsWith("fn(")) {
      const r = /\)\s*->\s*(.+)$/.exec(local.type);
      return r ? r[1] : "void";
    }
    if (model.classes.has(n)) return call[2] ? n + call[2].replace(/\s+/g, "") : n;
    if ((n === "Channel" || n === "Shared") && call[2]) return n + call[2].replace(/\s+/g, "");
    if (n === "Shared") {
      const arg = splitTop(seg.slice(seg.indexOf("(") + 1, matchClose(seg, seg.indexOf("(")) - 1), ",")[0];
      const t = arg && exprType(model, scope, arg);
      return t ? `Shared<${t}>` : null;
    }
    if (model.functions.has(n)) return model.functions.get(n).ret;
    if (BUILTINS[n]) return BUILTINS[n].ret;
    return null;
  }
  if (/^[A-Za-z_]\w*$/.test(seg)) {
    const v = scope.locals.get(seg);
    if (!v && LIBRARY[seg]) return "lib:" + seg;
    return v ? v.type : null;
  }
  return null;
}

/** `fn(a: int, b) => a` → `fn(int, _) -> int` (best effort). */
function lambdaType(model, scope, expr) {
  const close = matchClose(expr, expr.indexOf("("));
  const params = parseParams(expr.slice(expr.indexOf("(") + 1, close - 1));
  const rest = expr.slice(close).trim();
  let ret = "void";
  const arrow = /^->\s*/.exec(rest);
  if (arrow) {
    const end = readType(rest, arrow[0].length);
    ret = cleanType(rest.slice(arrow[0].length, end));
  } else if (rest.startsWith("=>")) {
    const inner = { ...scope, locals: new Map(scope.locals) };
    for (const p of params) inner.locals.set(p.name, { name: p.name, type: p.type, kind: "param" });
    ret = exprType(model, inner, rest.slice(2)) || "_";
  }
  return `fn(${params.map((p) => p.type || "_").join(", ")})${ret === "void" ? "" : " -> " + ret}`;
}

/** Type of an initializer expression (best effort, enough for completion). */
function exprType(model, scope, expr) {
  expr = expr.trim();
  if (!expr) return null;
  const cast = /\s(as|is)\s+([A-Z]\w*)\s*$/.exec(expr);
  if (cast) return cast[1] === "is" ? "bool" : cast[2] + "?";
  if (/^fn\s*\(/.test(expr)) return lambdaType(model, scope, expr);
  if (/^[A-Z]\w*\.[A-Z]\w*/.test(expr)) {
    const en = /^([A-Z]\w*)\./.exec(expr)[1];
    if (model.enums.has(en)) return en;
  }
  // `Channel<int>()`, `Stack<str>()`: the `<` is not a comparison.
  if (/^[A-Z]\w*\s*<[^()]*>\s*\(/.test(expr)) return chainType(model, scope, expr);
  // `a + b`: the left operand decides the type for arithmetic and joins.
  const m =/^((?:[^\s+\-*\/%<>=!&|]|\([^)]*\)|\[[^\]]*\])+)\s*([-+*\/%<>=!&|]{1,2})/.exec(expr);
  if (m && !/^[\[{"-]/.test(expr)) {
    const op = m[2];
    if (["==", "!=", "<", "<=", ">", ">=", "&&", "||"].includes(op)) return "bool";
    return chainType(model, scope, m[1]);
  }
  if (expr.startsWith("!")) return "bool";
  return chainType(model, scope, expr);
}

// ---------------------------------------------------------------- scope at cursor

function innermostBody(model, offset) {
  let best = null;
  for (const b of model.bodies) {
    if (b.start < offset && offset <= b.end && (!best || b.start > best.start)) best = b;
  }
  return best;
}

function enclosing(map, offset) {
  for (const c of map.values()) if (c.start !== undefined && c.start < offset && offset <= c.end) return c;
  return null;
}

/** Is a declaration at `at` (made at brace depth `d`) still in scope at `offset`? */
function visible(depth, at, d, offset) {
  for (let i = at; i < offset; i++) if (depth[i] < d) return false;
  return true;
}

function findVariant(model, name) {
  for (const en of model.enums.values()) {
    const v = en.variants.find((x) => x.name === name);
    if (v) return { en, v };
  }
  return null;
}

/** Variables and parameters visible at `offset`, with best-effort types. */
function scopeAt(model, offset) {
  const body = innermostBody(model, offset);
  const cls = body ? body.cls : null;
  const scope = { cls, owner: body ? body.owner : null, body, locals: new Map() };
  if (!body) return scope;
  for (const p of body.params) scope.locals.set(p.name, { name: p.name, type: p.type, kind: "param" });

  const { code, depth } = model;
  const start = body.start;
  const region = code.slice(start, offset);
  const decls = [];
  for (const m of region.matchAll(/\b(let|var)\s+([A-Za-z_]\w*)\s*(:)?/g)) {
    const at = start + m.index;
    let i = at + m[0].length;
    let annot = null;
    if (m[3]) {
      const end = readType(code, i);
      annot = cleanType(code.slice(i, end));
      i = end;
    }
    i = skipSpaces(code, i);
    const eol = code.indexOf("\n", i);
    const expr = code[i] === "=" ? code.slice(i + 1, eol < 0 ? code.length : eol) : "";
    decls.push({ at, d: depth[at], name: m[2], annot, expr, kind: m[1] });
  }
  for (const m of region.matchAll(/\bfor\s+([A-Za-z_]\w*)\s+in\s+([^{\n]*)\{/g)) {
    const at = start + m.index + m[0].length;
    decls.push({ at, d: depth[at], name: m[1], forIn: m[2], kind: "for" });
  }
  for (const m of region.matchAll(/\bif\s+let\s+([A-Za-z_]\w*)\s*=\s*([^{\n]*)\{/g)) {
    const at = start + m.index + m[0].length;
    decls.push({ at, d: depth[at], name: m[1], unwrap: m[2], kind: "let" });
  }
  // match arm bindings: `Circle(r) =>`
  for (const m of region.matchAll(/(?:\b[A-Z]\w*\.)?\b([A-Z]\w*)\s*\(([^()]*)\)\s*=>/g)) {
    const found = findVariant(model, m[1]);
    if (!found) continue;
    const at = start + m.index + m[0].length;
    m[2].split(",").map((s) => s.trim()).forEach((b, i) => {
      if (b && b !== "_") decls.push({ at, d: depth[at], name: b, type: found.v.params[i] ? found.v.params[i].type : null, kind: "let" });
    });
  }
  // lambda parameters: `fn(x: int) =>` / `xs.map(fn(x) => ...)`
  for (const m of region.matchAll(/\bfn\s*\(([^()]*)\)/g)) {
    const at = start + m.index + m[0].length;
    const before = code.slice(Math.max(0, start + m.index - 200), start + m.index);
    const recv = /([\w.\]\[)("]+)\.(map|filter|find|sort_by)\(\s*$/.exec(before);
    for (const p of parseParams(m[1])) {
      decls.push({ at, d: depth[at], name: p.name, type: p.type, elemOf: p.type ? null : recv && recv[1], kind: "param" });
    }
  }
  decls.sort((a, b) => a.at - b.at);
  for (const v of decls) {
    if (!visible(depth, v.at, v.d, offset)) continue;
    let type = v.type || null;
    if (v.elemOf) {
      const t = chainType(model, scope, v.elemOf);
      type = t ? elemOf(t) : null;
    }
    if (v.annot) type = v.annot;
    else if (v.expr !== undefined) type = exprType(model, scope, v.expr);
    else if (v.forIn !== undefined) {
      if (v.forIn.includes("..")) type = "int";
      else {
        const t = exprType(model, scope, v.forIn);
        type = t === "str" ? "str" : isMap(t) ? mapKV(t)[0] : elemOf(t);
      }
    } else if (v.unwrap !== undefined) type = unwrapOptional(exprType(model, scope, v.unwrap));
    scope.locals.set(v.name, { name: v.name, type, kind: v.kind });
  }
  return scope;
}

// ---------------------------------------------------------------- completion items

function sigText(params, ret) {
  const ps = params.map((p) => (p.type ? `${p.name}: ${p.type}` : p.name)).join(", ");
  return `(${ps})${ret && ret !== "void" ? " -> " + ret : ""}`;
}

/** `name(${1:a}, ${2:b})` or `name()` */
function callSnippet(name, params) {
  if (!params.length) return `${name}()`;
  return `${name}(${params.map((p, i) => `\${${i + 1}:${p.name.replace(/[}$\\]/g, "")}}`).join(", ")})`;
}

function methodItem(m, sort = "1") {
  return {
    label: m.name,
    kind: "method",
    detail: `${m.owner ? m.owner + "." : ""}${m.name}${sigText(m.params, m.ret)}`,
    documentation: m.doc || "",
    insertText: callSnippet(m.name, m.params),
    snippet: true,
    sortText: sort + m.name,
  };
}

function memberItems(model, type, scope, viaSuper) {
  if (!type) return [];
  const [cname] = splitGeneric(type);
  if (model.classes.has(cname)) {
    const viewer = scope.owner || scope.cls;
    const fields = viaSuper
      ? []
      : classFields(model, cname)
          .filter((f) => visibleTo(f, viewer))
          .map((f) => ({
            label: f.name,
            kind: "field",
            detail: `${f.owner}.${f.name}: ${substitute(model, type, f.type)}`,
            documentation: f.doc || "",
            insertText: f.name,
            sortText: "0" + f.name,
          }));
    const methods = methodsOf(model, type)
      .filter((m) => visibleTo(m, viewer) && !(viaSuper && m.abstract))
      .map((m) => methodItem(m));
    return fields.concat(methods);
  }
  return methodsOf(model, type).map((m) => methodItem(m));
}

/** `Enum.` → variants, `Class.` / `Struct.` → static methods, `Math.` → library. */
function typeMemberItems(model, name, scope) {
  if (LIBRARY[name] && !model.classes.has(name)) {
    const lib = LIBRARY[name];
    const values = Object.entries(lib.values).map(([v, t]) => ({ label: v, kind: "constant", detail: `${name}.${v}: ${t}`, insertText: v, sortText: "0" + v }));
    return values.concat(lib.fns.map((m) => ({ ...methodItem(m), detail: `${name}.${m.name}${sigText(m.params, m.ret)}` })));
  }
  if (model.enums.has(name)) {
    return model.enums.get(name).variants.map((v) => ({
      label: v.name,
      kind: "enumMember",
      detail: `${name}.${v.name}${v.params.length ? sigText(v.params, "") : ""}`,
      insertText: v.params.length ? callSnippet(v.name, v.params) : v.name,
      snippet: v.params.length > 0,
      sortText: "0" + v.name,
    }));
  }
  if (model.classes.has(name)) {
    const viewer = scope.owner || scope.cls;
    return staticMethods(model, name).filter((m) => visibleTo(m, viewer)).map((m) => methodItem(m));
  }
  return [];
}

function typeItems(model, exclude) {
  const items = PRIMITIVES.map((p) => ({ label: p, kind: "type", detail: "built-in type", insertText: p, sortText: "0" + p }));
  for (const c of model.classes.values()) {
    if (c.name === exclude) continue;
    const generic = c.tparams.length > 0;
    items.push({
      label: c.name,
      kind: "class",
      detail: `${c.struct ? "struct" : "class"} ${c.name}${generic ? "<" + c.tparams.join(", ") + ">" : ""}`,
      documentation: c.doc,
      insertText: generic ? `${c.name}<${c.tparams.map((t, i) => `\${${i + 1}:${t}}`).join(", ")}>` : c.name,
      snippet: generic,
      sortText: "1" + c.name,
    });
  }
  for (const i of model.interfaces.values()) {
    if (i.name !== exclude) items.push({ label: i.name, kind: "interface", detail: "interface " + i.name, documentation: i.doc, insertText: i.name, sortText: "1" + i.name });
  }
  for (const e of model.enums.values()) {
    items.push({ label: e.name, kind: "enum", detail: "enum " + e.name, documentation: e.doc, insertText: e.name, sortText: "1" + e.name });
  }
  for (const t of model.externTypes) {
    items.push({ label: t, kind: "class", detail: "extern type " + t, insertText: t, sortText: "1" + t });
  }
  for (const [name, b] of Object.entries(BUILTIN_TYPES)) {
    if (model.classes.has(name)) continue;
    items.push({
      label: name,
      kind: "class",
      detail: `${name}<${b.params.join(", ")}>`,
      documentation: b.doc,
      insertText: `${name}<${b.params.map((t, i) => `\${${i + 1}:${t === "E" ? "str" : "int"}}`).join(", ")}>`,
      snippet: true,
      sortText: "2" + name,
    });
  }
  return items;
}

/** Text from the start of the current line up to the cursor (masked). */
function linePrefix(code, offset) {
  const start = code.lastIndexOf("\n", offset - 1) + 1;
  return code.slice(start, offset);
}

/** Receiver expression ending just before a `.` at `dot`, scanning backwards. */
function receiverBefore(code, dot) {
  let i = dot - 1;
  let d = 0;
  for (; i >= 0; i--) {
    const ch = code[i];
    if (")]".includes(ch)) d++;
    else if ("([".includes(ch)) {
      if (d === 0) break;
      d--;
    } else if (d === 0 && !/[\w."<>]/.test(ch)) break;
  }
  return code.slice(i + 1, dot).trim();
}

/** Innermost `match <expr> {` block containing `offset`: { open, scrutinee }. */
function enclosingMatch(model, offset) {
  const { code } = model;
  let best = null;
  for (const m of code.matchAll(/\bmatch\b([^{\n]*)\{/g)) {
    const open = m.index + m[0].length - 1;
    if (open < offset && offset <= matchBrace(code, open) && (!best || open > best.open)) best = { open, scrutinee: m[1] };
  }
  return best;
}

/**
 * Completion items at `offset` (0-based index into `text`).
 * Each item: { label, kind, detail, documentation, insertText, snippet, sortText }.
 * `options.load(path, dir)` lets declarations from imported files take part.
 */
function complete(text, offset, options = {}) {
  const model = withImports(analyze(text, options.dir), options.load);
  const { code, hidden } = model;
  if (offset > 0 && hidden[offset - 1]) return []; // inside a comment or string
  const prefix = linePrefix(code, offset);

  // Member access: `expr.` or `expr.partial`
  const member = /\.\s*([A-Za-z_]\w*)?$/.exec(prefix);
  if (member && !/\.\.\s*\w*$/.test(prefix)) {
    const dot = offset - member[0].length;
    const recv = receiverBefore(code, dot);
    if (!recv) return [];
    const scope = scopeAt(model, offset);
    if (model.namespaces.has(recv) && !scope.locals.has(recv)) return namespaceItems(model.namespaces.get(recv));
    const qualified = /^([A-Za-z_]\w*)\.([A-Z]\w*)$/.exec(recv);
    if (qualified && model.namespaces.has(qualified[1]) && !scope.locals.has(qualified[1])) {
      return typeMemberItems(model.namespaces.get(qualified[1]), qualified[2], scope);
    }
    if (/^[A-Z]\w*$/.test(recv) && !scope.locals.has(recv)) return typeMemberItems(model, recv, scope);
    return memberItems(model, chainType(model, scope, recv), scope, recv === "super");
  }

  // Base list: `class Dog : An|` / `interface Pet : Na|`
  const header = /\b(class|interface)\s+(\w+)(?:<[^>]*>)?\s*:\s*[\w\s,]*$/.exec(prefix);
  if (header) {
    const kinds = header[1] === "class" ? ["class", "interface"] : ["interface"];
    return typeItems(model, header[2]).filter((i) => kinds.includes(i.kind));
  }

  // Type position: after `name:`, `->`, `<`, `[`, `{K:` in a type
  if (/(?:\b[A-Za-z_]\w*\s*:|->|\bfn\s*\([^)]*|<|,\s*)\s*[\[{]?\s*\w*$/.test(prefix) && /(:|->|<|\bfn\s*\()/.test(prefix) && !/\b(for|if|match|print)\b|=/.test(prefix)) {
    return typeItems(model, null);
  }

  const depth = model.depth[offset];
  const iface = enclosing(model.interfaces, offset);
  if (iface) {
    return [{ label: "fn", kind: "keyword", detail: "method signature", insertText: "fn ${1:name}(${2}) -> ${3:str}", snippet: true, sortText: "0fn" }];
  }
  if (enclosing(model.enums, offset)) return [];
  const cls = enclosing(model.classes, offset);
  const body = innermostBody(model, offset);
  const items = [];

  // Class or struct body, between members.
  if (cls && !body && depth === model.depth[cls.start] + 1) {
    const kw = (label, detail, insertText) => ({ label, kind: "keyword", detail, insertText, snippet: true, sortText: "0" + label });
    if (cls.struct) {
      return [
        kw("fn", "method (reads self)", "fn ${1:name}(${2}) -> ${3:float} {\n\t$0\n}"),
        kw("mut fn", "method that changes self", "mut fn ${1:name}(${2}) {\n\t$0\n}"),
        kw("static fn", "function on the struct type", `static fn \${1:name}(\${2}) -> ${cls.name} {\n\t$0\n}`),
        kw("const", "constant, used as Type.NAME", "const ${1:NAME}: ${2:int} = $0"),
        kw("static var", "variable shared by the type", "static var ${1:name}: ${2:int} = $0"),
        kw("op +", "operator", `op +(other: ${cls.name}) -> ${cls.name} {\n\treturn $0\n}`),
        kw("private", "only usable inside this struct", "private "),
      ];
    }
    items.push(
      kw("fn", "method", "fn ${1:name}(${2}) -> ${3:void} {\n\t$0\n}"),
      kw("init", "constructor", "init(${1}) {\n\t$0\n}"),
      kw("private", "only usable inside this class", "private "),
      kw("static fn", "method called on the class", "static fn ${1:name}(${2}) -> ${3:void} {\n\t$0\n}"),
      kw("const", "constant, used as Class.NAME", "const ${1:NAME}: ${2:int} = $0"),
      kw("static var", "variable shared by the class", "static var ${1:name}: ${2:int} = $0"),
      kw("static async fn", "async function on the class", "static async fn ${1:name}(${2}) -> ${3:int} {\n\t$0\n}")
    );
    if (cls.abstract) items.push(kw("abstract fn", "method subclasses must implement", "abstract fn ${1:name}(${2}) -> ${3:void}"));
    const parent = parentOf(model, cls.name);
    const own = new Set(cls.methods.map((m) => m.name));
    // Interface methods not written yet: `override fn` stubs, listed first.
    for (const b of cls.bases) {
      if (!model.interfaces.has(b)) continue;
      for (const m of interfaceMethods(model, b)) {
        if (own.has(m.name) || (parent && classMethods(model, parent).some((x) => x.name === m.name))) continue;
        own.add(m.name);
        const ret = m.ret && m.ret !== "void" ? ` -> ${m.ret}` : "";
        items.push({
          label: `override fn ${m.name}`,
          kind: "method",
          detail: `implement ${m.owner}.${m.name}${sigText(m.params, m.ret)}`,
          documentation: m.doc || "",
          insertText: `override fn ${m.name}(${m.params.map((p) => `${p.name}: ${p.type}`).join(", ")})${ret} {\n\t$0\n}`,
          snippet: true,
          sortText: "0" + m.name,
        });
      }
    }
    if (parent) {
      for (const m of classMethods(model, parent)) {
        if (own.has(m.name) || m.private) continue;
        const ret = m.ret && m.ret !== "void" ? ` -> ${m.ret}` : "";
        items.push({
          label: `override fn ${m.name}`,
          kind: "method",
          detail: `${m.abstract ? "implement abstract" : "override"} ${m.owner}.${m.name}${sigText(m.params, m.ret)}`,
          documentation: m.doc || "",
          insertText: `override fn ${m.name}(${m.params.map((p) => `${p.name}: ${p.type}`).join(", ")})${ret} {\n\t$0\n}`,
          snippet: true,
          sortText: (m.abstract ? "0" : "1") + m.name,
        });
      }
    }
    return items;
  }

  // Top level.
  if (depth === 0) {
    const top = {
      fn: "fn",
      "async fn": "async fn ${1:name}(${2}) -> ${3:int} {\n\t$0\n}",
      class: "class",
      "abstract class": "abstract class",
      struct: "struct",
      interface: "interface",
      enum: "enum",
      const: "const ${1:NAME}: ${2:int} = $0",
      import: 'import "${1:file.kl}"',
      "import as": 'import "${1:package}" as ${2:name}',
      "use rust": 'use rust "${1:native/file.rs}"',
      "use crate": 'use crate "${1:name}" = "${2:1}"',
      "extern fn": "extern fn ${1:name}(${2}) -> ${3:int}",
      "extern type": "extern type ${1:Name}",
    };
    for (const [k, ins] of Object.entries(top)) {
      items.push({ label: k, kind: "keyword", insertText: ins, snippet: ins.includes("$"), sortText: "0" + k });
    }
    return items;
  }

  // Inside a function or method body.
  const scope = scopeAt(model, offset);

  // Arm position inside `match x { ... }`: offer the patterns.
  const mt = enclosingMatch(model, offset);
  if (mt && depth === model.depth[mt.open] + 1 && /^\s*\w*$/.test(prefix)) {
    const st = exprType(model, scope, mt.scrutinee);
    const en = st && model.enums.get(st);
    if (en) {
      for (const v of en.variants) {
        items.push({
          label: v.name,
          kind: "enumMember",
          detail: `${en.name}.${v.name}${v.params.length ? sigText(v.params, "") : ""}`,
          insertText: (v.params.length ? callSnippet(v.name, v.params) : v.name) + " => ",
          snippet: v.params.length > 0,
          sortText: "0" + v.name,
        });
      }
    }
    items.push({ label: "_", kind: "keyword", detail: "matches anything else", insertText: "_ => ", sortText: "1_" });
    return items;
  }

  for (const v of scope.locals.values()) {
    items.push({ label: v.name, kind: "variable", detail: v.type ? `${v.name}: ${v.type}` : v.name, insertText: v.name, sortText: "0" + v.name });
  }
  if (scope.cls) {
    items.push({ label: "self", kind: "keyword", detail: "this " + scope.cls + " object", insertText: "self", sortText: "1self" });
    if (parentOf(model, scope.cls)) {
      items.push({ label: "super", kind: "keyword", detail: "base class " + parentOf(model, scope.cls), insertText: "super", sortText: "1super" });
    }
  }
  for (const f of model.functions.values()) {
    if (f.name === "main") continue;
    const tp = f.tparams && f.tparams.length ? `<${f.tparams.join(", ")}>` : "";
    items.push({ label: f.name, kind: "function", detail: `fn ${f.name}${tp}${sigText(f.params, f.ret)}`, documentation: f.doc, insertText: callSnippet(f.name, f.params), snippet: true, sortText: "2" + f.name });
  }
  for (const c of model.classes.values()) {
    if (c.abstract) {
      items.push({ label: c.name, kind: "class", detail: `abstract class ${c.name}`, documentation: c.doc, insertText: c.name, sortText: "3" + c.name });
      continue;
    }
    const init = findInit(model, c.name);
    const params = c.struct ? c.fields.map((x) => ({ name: x.name, type: x.type })) : init ? init.params : [];
    items.push({ label: c.name, kind: "constructor", detail: `${c.name}${sigText(params, "")}`, documentation: c.doc, insertText: callSnippet(c.name, params), snippet: true, sortText: "3" + c.name });
  }
  for (const e of model.enums.values()) {
    items.push({ label: e.name, kind: "enum", detail: "enum " + e.name, documentation: e.doc, insertText: e.name, sortText: "3" + e.name });
  }
  for (const lib of Object.keys(LIBRARY)) {
    items.push({ label: lib, kind: "module", detail: `built-in ${lib} library`, insertText: lib, sortText: "4" + lib });
  }
  for (const alias of model.namespaces.keys()) {
    items.push({ label: alias, kind: "module", detail: `package ${model.aliases.get(alias)}`, insertText: alias, sortText: "3" + alias });
  }
  items.push(
    { label: "Ok", kind: "constructor", detail: "Ok(value): a successful Result", insertText: "Ok(${1})", snippet: true, sortText: "4Ok" },
    { label: "Err", kind: "constructor", detail: "Err(error): a failed Result", insertText: "Err(${1})", snippet: true, sortText: "4Err" },
    { label: "Channel", kind: "constructor", detail: "Channel<T>(): send values between threads", insertText: "Channel<${1:int}>()", snippet: true, sortText: "4Channel" },
    { label: "Shared", kind: "constructor", detail: "Shared<T>(value): a value threads share", insertText: "Shared<${1:int}>(${2:0})", snippet: true, sortText: "4Shared" }
  );
  for (const [name, b] of Object.entries(BUILTINS)) {
    const params = b.params.map((p) => ({ name: p.split(":")[0].replace("...", "") || "x" }));
    items.push({
      label: name,
      kind: "function",
      detail: `${name}(${b.params.join(", ")})${b.ret !== "void" ? " -> " + b.ret : ""}`,
      documentation: b.doc,
      insertText: name === "print" ? "print(${1})" : callSnippet(name, params),
      snippet: true,
      sortText: "4" + name,
    });
  }
  for (const k of KEYWORDS) items.push({ label: k, kind: "keyword", insertText: k, sortText: "5" + k });
  return items;
}

// ---------------------------------------------------------------- signature help

/**
 * Signature of the call surrounding `offset`, or null.
 * Returns { label, params: [string], activeParameter, documentation }.
 */
function signatureHelp(text, offset, options = {}) {
  const model = withImports(analyze(text, options.dir), options.load);
  const { code, hidden } = model;
  if (offset > 0 && hidden[offset - 1]) return null;

  // Find the innermost unclosed `(` and count commas at its level.
  let d = 0;
  let commas = 0;
  let open = -1;
  for (let i = offset - 1; i >= 0; i--) {
    const ch = code[i];
    if (ch === "{" || ch === "}") break;
    if (")]".includes(ch)) d++;
    else if (ch === "[") d--;
    else if (ch === "(") {
      if (d === 0) {
        open = i;
        break;
      }
      d--;
    } else if (ch === "," && d === 0) commas++;
  }
  if (open < 0) return null;

  let before = code.slice(0, open).replace(/\s+$/, "");
  if (before.endsWith(">")) before = before.slice(0, before.lastIndexOf("<")).replace(/\s+$/, ""); // `first<int>(`
  const nameMatch = /([A-Za-z_]\w*)$/.exec(before);
  if (!nameMatch) return null;
  const name = nameMatch[1];
  const head = before.slice(0, before.length - name.length);
  const scope = scopeAt(model, offset);

  let target = null; // { name, params, ret, doc }
  if (/\.\s*$/.test(head)) {
    const recv = receiverBefore(code, head.replace(/\s*$/, "").length - 1);
    const ns = !scope.locals.has(recv) && model.namespaces.get(recv);
    if (ns) {
      if (ns.functions.has(name)) target = { ...ns.functions.get(name), name: `${recv}.${name}` };
      else if (ns.classes.has(name)) {
        const c = ns.classes.get(name);
        const init = findInit(ns, name);
        target = { name: `${recv}.${name}`, params: c.struct ? c.fields.map((x) => ({ name: x.name, type: x.type })) : init ? init.params : [], ret: "", doc: c.doc };
      }
    } else if (/^[A-Z]\w*$/.test(recv) && !scope.locals.has(recv)) {
      if (LIBRARY[recv] && !model.classes.has(recv)) {
        const fn = LIBRARY[recv].fns.find((x) => x.name === name);
        if (fn) target = { ...fn, name: `${recv}.${fn.name}` };
      } else if (model.enums.has(recv)) {
        const v = model.enums.get(recv).variants.find((x) => x.name === name);
        if (v) target = { name: `${recv}.${v.name}`, params: v.params, ret: "", doc: "" };
      } else if (model.classes.has(recv)) {
        target = staticMethods(model, recv).find((m) => m.name === name) || null;
      }
    } else {
      target = methodsOf(model, chainType(model, scope, recv)).find((m) => m.name === name) || null;
    }
  } else if (name === "super" && scope.cls) {
    const parent = parentOf(model, scope.cls);
    const init = parent && findInit(model, parent);
    if (init) target = { name: "super", params: init.params, ret: "void", doc: init.doc };
  } else if (scope.locals.has(name) && (scope.locals.get(name).type || "").startsWith("fn(")) {
    const t = scope.locals.get(name).type;
    const close = matchClose(t, 2);
    const ps = splitTop(t.slice(3, close - 1), ",").map((s) => s.trim()).filter(Boolean);
    const r = /^\s*->\s*(.+)$/.exec(t.slice(close));
    target = { name, params: ps.map((p) => ({ name: p })), ret: r ? r[1] : "void", doc: "" };
  } else if (model.classes.has(name)) {
    const c = model.classes.get(name);
    const init = findInit(model, name);
    const params = c.struct ? c.fields.map((x) => ({ name: x.name, type: x.type })) : init ? init.params : [];
    target = { name, params, ret: "", doc: c.doc };
  } else if (model.functions.has(name)) {
    target = model.functions.get(name);
  } else if (BUILTINS[name]) {
    const b = BUILTINS[name];
    target = { name, params: b.params.map((p) => ({ name: p })), ret: b.ret, doc: b.doc };
  }
  if (!target) return null;

  const params = target.params.map((p) => (p.type ? `${p.name}: ${p.type}` : p.name));
  const ret = target.ret && target.ret !== "void" ? ` -> ${target.ret}` : "";
  return {
    label: `${target.name}(${params.join(", ")})${ret}`,
    params,
    activeParameter: Math.min(commas, Math.max(0, params.length - 1)),
    documentation: target.doc || "",
  };
}

// ---------------------------------------------------------------- hover

const PRIMITIVE_DOCS = {
  int: "64-bit whole number.",
  float: "64-bit floating-point number.",
  bool: "`true` or `false`.",
  str: "Text (UTF-8).",
  void: "No value.",
};

/** `fn name<T>(params) -> ret` */
function fnHeader(f, prefix = "fn") {
  const tp = f.tparams && f.tparams.length ? `<${f.tparams.join(", ")}>` : "";
  const ret = f.async ? f.ret.replace(/^Async<(.*)>$/, "$1") : f.ret;
  return `${f.async ? "async " : ""}${prefix} ${f.name}${tp}${sigText(f.params, ret)}`;
}

function methodHeader(m) {
  const mods = [m.private && "private", m.static && "static", m.abstract && "abstract", m.override && "override", m.mut && "mut"].filter(Boolean);
  return [...mods, "fn"].join(" ") + ` ${m.name}${sigText(m.params, m.ret)}`;
}

function classHeader(model, c) {
  const tp = c.tparams.length ? `<${c.tparams.join(", ")}>` : "";
  const bases = c.bases.length ? ` : ${c.bases.join(", ")}` : "";
  const kind = c.struct ? "struct" : (c.abstract ? "abstract class" : "class");
  let out = `${kind} ${c.name}${tp}${bases}`;
  if (c.struct) {
    out += `(${c.fields.map((x) => `${x.name}: ${x.type}`).join(", ")})`;
  } else if (!c.abstract) {
    const init = findInit(model, c.name);
    if (init) out += sigText(init.params, "");
  }
  return out;
}

function md(code, doc, note) {
  let out = "```kalinga\n" + code + "\n```";
  if (note) out += "\n\n" + note;
  if (doc) out += "\n\n---\n\n" + doc;
  return out;
}

/** Hover text for a top-level name (`model` may be a package namespace). */
function declHover(model, name, scope) {
  if (model.functions.has(name)) {
    const f = model.functions.get(name);
    return md(fnHeader(f, f.extern ? "extern fn" : "fn"), f.doc);
  }
  if (model.classes.has(name)) {
    const c = model.classes.get(name);
    return md(classHeader(model, c), c.doc);
  }
  if (model.interfaces.has(name)) {
    const i = model.interfaces.get(name);
    const supers = i.supers.length ? ` : ${i.supers.join(", ")}` : "";
    const body = i.methods.map((m) => `    fn ${m.name}${sigText(m.params, m.ret)}`).join("\n");
    return md(`interface ${i.name}${supers} {\n${body}\n}`, i.doc);
  }
  if (model.enums.has(name)) {
    const e = model.enums.get(name);
    const body = e.variants.map((v) => `    ${v.name}${v.params.length ? sigText(v.params, "") : ""}`).join("\n");
    return md(`enum ${e.name} {\n${body}\n}`, e.doc);
  }
  if (model.externTypes.has(name)) return md(`extern type ${name}`, "", "A Rust type, used through `extern fn`s.");
  if (LIBRARY[name] && !(scope && scope.locals.has(name))) {
    const names = Object.keys(LIBRARY[name].values).concat(LIBRARY[name].fns.map((f) => f.name + "()"));
    return md(name, "", `Built-in library: ${names.map((n) => "`" + n + "`").join(", ")}.`);
  }
  if (BUILTIN_TYPES[name]) return md(`${name}<${BUILTIN_TYPES[name].params.join(", ")}>`, BUILTIN_TYPES[name].doc);
  if (BUILTINS[name]) {
    const b = BUILTINS[name];
    return md(`fn ${name}(${b.params.join(", ")})${b.ret !== "void" ? " -> " + b.ret : ""}`, b.doc);
  }
  if (PRIMITIVE_DOCS[name]) return md(name, PRIMITIVE_DOCS[name]);
  if (name === "Ok" || name === "Err") return md(`${name}(${name === "Ok" ? "value" : "error"})`, `A${name === "Ok" ? " successful" : " failed"} \`Result\`.`);
  const variant = findVariant(model, name);
  if (variant) return md(`${variant.en.name}.${variant.v.name}${variant.v.params.length ? sigText(variant.v.params, "") : ""}`, "");
  return null;
}

/** Hover text for `name` after `recv.`. */
function memberHover(model, scope, recv, name) {
  const ns = !scope.locals.has(recv) && model.namespaces && model.namespaces.get(recv);
  if (ns) return declHover(ns, name, null);
  const qualified = /^([A-Za-z_]\w*)\.([A-Z]\w*)$/.exec(recv);
  if (qualified && model.namespaces.has(qualified[1]) && !scope.locals.has(qualified[1])) {
    return memberHover(model.namespaces.get(qualified[1]), { locals: new Map() }, qualified[2], name);
  }
  if (/^[A-Z]\w*$/.test(recv) && !scope.locals.has(recv)) {
    if (LIBRARY[recv] && !model.classes.has(recv)) {
      const lib = LIBRARY[recv];
      if (lib.values[name]) return md(`${recv}.${name}: ${lib.values[name]}`, "");
      const f = lib.fns.find((x) => x.name === name);
      return f ? md(`fn ${recv}.${f.name}${sigText(f.params, f.ret)}`, f.doc) : null;
    }
    if (model.enums.has(recv)) {
      const v = model.enums.get(recv).variants.find((x) => x.name === name);
      return v ? md(`${recv}.${v.name}${v.params.length ? sigText(v.params, "") : ""}`, model.enums.get(recv).doc) : null;
    }
    if (model.classes.has(recv)) {
      const m = staticMethods(model, recv).find((x) => x.name === name);
      return m ? md(methodHeader(m), m.doc, `in \`${m.owner}\``) : null;
    }
  }
  const type = chainType(model, scope, recv);
  if (!type) return null;
  const [cname] = splitGeneric(type);
  if (model.classes.has(cname)) {
    const f = classFields(model, cname).find((x) => x.name === name);
    if (f) return md(`${f.private ? "private " : ""}${f.name}: ${substitute(model, type, f.type)}`, f.doc, `field of \`${f.owner}\``);
  }
  const m = methodsOf(model, type).find((x) => x.name === name);
  if (!m) return null;
  const note = m.owner ? `in \`${m.owner}\`` : `on \`${type}\``;
  return md(m.owner ? methodHeader(m) : `fn ${m.name}${sigText(m.params, m.ret)}`, m.doc, note);
}

/**
 * Hover for the word at `offset`: `{ contents: markdown, start, end }` or null.
 * `options.load(path, dir)` lets declarations from imported files and packages take part.
 */
function hover(text, offset, options = {}) {
  const model = withImports(analyze(text, options.dir), options.load);
  const { code, hidden } = model;
  if (hidden[offset] && hidden[Math.max(0, offset - 1)]) return null; // a comment or string
  let start = offset;
  let end = offset;
  while (start > 0 && /\w/.test(code[start - 1])) start--;
  while (end < code.length && /\w/.test(code[end])) end++;
  if (start === end || /^\d/.test(code.slice(start, end))) return null;
  const name = code.slice(start, end);
  const scope = scopeAt(model, end);
  const result = (contents) => (contents ? { contents, start, end } : null);

  let before = start - 1;
  while (before >= 0 && /[ \t]/.test(code[before])) before--;
  if (code[before] === "." && code[before - 1] !== ".") {
    const recv = receiverBefore(code, before);
    return recv ? result(memberHover(model, scope, recv, name)) : null;
  }

  if (name === "self" && scope.cls) return result(md(`self: ${scope.cls}`, ""));
  const local = scope.locals.get(name);
  if (local) {
    const what = { param: "parameter", for: "loop variable", let: "let", var: "var" }[local.kind] || "variable";
    return result(md(`${local.kind === "var" ? "var" : "let"} ${name}${local.type ? ": " + local.type : ""}`, "", what === "let" || what === "var" ? "" : what));
  }
  if (model.namespaces.has(name)) return result(md(`import "${model.aliases.get(name)}" as ${name}`, "", `Package; use \`${name}.Name\`.`));

  // A member's own declaration, inside its class or struct.
  const cls = enclosing(model.classes, start);
  if (cls) {
    const m = cls.methods.find((x) => x.name === name);
    const header = code.slice(code.lastIndexOf("\n", start) + 1, start);
    if (m && /\bfn\s*$/.test(header)) return result(md(methodHeader(m), m.doc, `in \`${cls.name}\``));
    const f = cls.fields.find((x) => x.name === name);
    if (f && /^\s*(private\s+)?$/.test(header)) return result(md(`${f.private ? "private " : ""}${f.name}: ${f.type}`, f.doc, `field of \`${cls.name}\``));
  }
  const iface = enclosing(model.interfaces, start);
  if (iface) {
    const m = iface.methods.find((x) => x.name === name);
    if (m) return result(md(`fn ${m.name}${sigText(m.params, m.ret)}`, m.doc, `in interface \`${iface.name}\``));
  }
  return result(declHover(model, name, scope));
}

module.exports = { analyze, complete, signatureHelp, hover, scopeAt, chainType };
