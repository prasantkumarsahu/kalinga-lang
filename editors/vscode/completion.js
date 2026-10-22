// Kalinga code completion and signature help.
//
// Works on raw source text (no VS Code dependency) so it can be unit-tested with node and
// keeps working while the file is half-typed and would not compile.

"use strict";

const KEYWORDS = [
  "let", "var", "if", "else", "while", "for", "in", "return", "break", "continue",
  "true", "false", "none",
];
const PRIMITIVES = ["int", "float", "bool", "str", "void"];

const BUILTINS = {
  print: { params: ["values..."], ret: "void", doc: "Prints the values separated by spaces, then a newline." },
  len: { params: ["x: [T] or str"], ret: "int", doc: "Number of elements in an array, or characters in a string." },
  str: { params: ["x"], ret: "str", doc: "Text form of an int, float, bool, str, array or optional." },
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
  return [
    { name: "push", params: [{ name: "x", type: elem }], ret: "void", doc: "Adds `x` at the end (needs a `var` array)." },
    { name: "pop", params: [], ret: elem + "?", doc: "Removes and returns the last element, or `none` if empty." },
    { name: "len", params: [], ret: "int", doc: "Number of elements." },
    { name: "contains", params: [{ name: "x", type: elem }], ret: "bool", doc: "Whether `x` is in the array." },
  ];
}

// ---------------------------------------------------------------- text helpers

/** Blanks out comments and string contents (keeping quotes) so offsets stay valid. */
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
        if (text[i] === "\\" && i + 1 < text.length && text[i + 1] !== "\n") blank(i++);
        blank(i++);
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

/** Index just past the `}` matching the `{` at `open`, or the end of text if unclosed. */
function matchBrace(code, open) {
  let d = 0;
  for (let i = open; i < code.length; i++) {
    if (code[i] === "{") d++;
    else if (code[i] === "}" && --d === 0) return i + 1;
  }
  return code.length;
}

/** `// comment` lines directly above `index`, joined. */
function docAbove(text, index) {
  const lines = text.slice(0, index).split("\n");
  lines.pop(); // the declaration's own line
  const doc = [];
  while (lines.length) {
    const l = lines[lines.length - 1].trim();
    if (!l.startsWith("//")) break;
    doc.unshift(l.replace(/^\/\/\s?/, ""));
    lines.pop();
  }
  return doc.join("\n");
}

function parseParams(src) {
  return src
    .split(",")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => {
      const m = /^([A-Za-z_]\w*)\s*:\s*(.+)$/.exec(p);
      return m ? { name: m[1], type: m[2].trim() } : { name: p, type: "?" };
    });
}

function cleanType(t) {
  return t ? t.replace(/\s+/g, "") : "void";
}

// ---------------------------------------------------------------- model

/**
 * Finds classes, interfaces, functions and every function-like body in the file.
 * Tolerates incomplete code: unclosed bodies simply run to the end of the text.
 */
function analyze(text) {
  const { code, hidden } = mask(text);
  const depth = braceDepths(code);
  const model = { text, code, hidden, depth, classes: new Map(), interfaces: new Map(), functions: new Map(), bodies: [] };

  const headerFn = /\b(override\s+)?fn\s+([A-Za-z_]\w*)\s*\(([^)]*)\)\s*(?:->\s*([^{\n]+?))?\s*\{/g;

  // Classes
  const classRe = /\bclass\s+([A-Za-z_]\w*)\s*(?::\s*([\w\s,]*?))?\s*\{/g;
  for (let m; (m = classRe.exec(code)); ) {
    if (depth[m.index] !== 0) continue;
    const open = m.index + m[0].length - 1;
    const end = matchBrace(code, open);
    const cls = {
      name: m[1],
      bases: (m[2] || "").split(",").map((s) => s.trim()).filter(Boolean),
      fields: [],
      methods: [],
      init: null,
      doc: docAbove(text, m.index),
      start: open,
      end,
    };
    const inner = depth[open] + 1;
    const body = code.slice(open + 1, end);
    const fieldRe = /^[ \t]*([a-z_]\w*)[ \t]*:[ \t]*([^=\n]+?)[ \t]*(?:=[^\n]*)?$/gm;
    for (let f; (f = fieldRe.exec(body)); ) {
      const at = open + 1 + f.index + f[0].indexOf(f[1]);
      if (depth[at] !== inner) continue;
      cls.fields.push({ name: f[1], type: cleanType(f[2]), doc: docAbove(text, at) });
    }
    headerFn.lastIndex = 0;
    for (let f; (f = headerFn.exec(body)); ) {
      const at = open + 1 + f.index;
      if (depth[at] !== inner) continue;
      const method = {
        name: f[2],
        params: parseParams(f[3]),
        ret: cleanType(f[4]),
        override: !!f[1],
        doc: docAbove(text, at),
        owner: cls.name,
      };
      cls.methods.push(method);
      const bodyOpen = at + f[0].length - 1;
      model.bodies.push({ start: bodyOpen, end: matchBrace(code, bodyOpen), params: method.params, cls: cls.name, kind: "method" });
    }
    const initRe = /\binit\s*\(([^)]*)\)\s*\{/g;
    for (let f; (f = initRe.exec(body)); ) {
      const at = open + 1 + f.index;
      if (depth[at] !== inner) continue;
      cls.init = { params: parseParams(f[1]), doc: docAbove(text, at) };
      const bodyOpen = at + f[0].length - 1;
      model.bodies.push({ start: bodyOpen, end: matchBrace(code, bodyOpen), params: cls.init.params, cls: cls.name, kind: "init" });
    }
    model.classes.set(cls.name, cls);
  }

  // Interfaces
  const ifaceRe = /\binterface\s+([A-Za-z_]\w*)\s*\{/g;
  for (let m; (m = ifaceRe.exec(code)); ) {
    if (depth[m.index] !== 0) continue;
    const open = m.index + m[0].length - 1;
    const end = matchBrace(code, open);
    const iface = { name: m[1], methods: [], doc: docAbove(text, m.index), start: open, end };
    const sigRe = /\bfn\s+([A-Za-z_]\w*)\s*\(([^)]*)\)[ \t]*(?:->[ \t]*([^\n}]+))?/g;
    const body = code.slice(open + 1, end);
    for (let f; (f = sigRe.exec(body)); ) {
      iface.methods.push({ name: f[1], params: parseParams(f[2]), ret: cleanType(f[3]), doc: docAbove(text, open + 1 + f.index), owner: iface.name });
    }
    model.interfaces.set(iface.name, iface);
  }

  // Top-level functions
  headerFn.lastIndex = 0;
  for (let m; (m = headerFn.exec(code)); ) {
    if (depth[m.index] !== 0 || m[1]) continue;
    const fn = { name: m[2], params: parseParams(m[3]), ret: cleanType(m[4]), doc: docAbove(text, m.index) };
    model.functions.set(fn.name, fn);
    const bodyOpen = m.index + m[0].length - 1;
    model.bodies.push({ start: bodyOpen, end: matchBrace(code, bodyOpen), params: fn.params, cls: null, kind: "fn" });
  }
  return model;
}

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

function classFields(model, name) {
  return lineage(model, name).flatMap((c) => c.fields.map((f) => ({ ...f, owner: c.name })));
}

/** Most-derived version of each method visible on a class. */
function classMethods(model, name) {
  const seen = new Map();
  for (const c of lineage(model, name)) {
    for (const m of c.methods) if (!seen.has(m.name)) seen.set(m.name, m);
  }
  return [...seen.values()];
}

function findInit(model, name) {
  for (const c of lineage(model, name)) if (c.init) return { ...c.init, owner: c.name };
  return null;
}

// ---------------------------------------------------------------- types

const isArray = (t) => !!t && t.startsWith("[") && t.endsWith("]");
const elemOf = (t) => (isArray(t) ? t.slice(1, -1) : null);
const unwrapOptional = (t) => (t && t.endsWith("?") ? t.slice(0, -1) : t);

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

function memberType(model, type, seg) {
  const call = /^([A-Za-z_]\w*)\s*\(/.exec(seg);
  const name = call ? call[1] : seg;
  const base = type;
  if (base === "str") {
    const m = STR_METHODS.find((x) => x.name === name);
    return m && call ? m.ret : null;
  }
  if (isArray(base)) {
    const m = arrayMethods(elemOf(base)).find((x) => x.name === name);
    return m && call ? m.ret : null;
  }
  if (model.interfaces.has(base)) {
    const m = model.interfaces.get(base).methods.find((x) => x.name === name);
    return m && call ? m.ret : null;
  }
  if (model.classes.has(base)) {
    if (call) {
      const m = classMethods(model, base).find((x) => x.name === name);
      return m ? m.ret : null;
    }
    const f = classFields(model, base).find((x) => x.name === name);
    return f ? f.type : null;
  }
  return null;
}

/** Best-effort static type of a receiver expression like `self.items[0].name`. */
function chainType(model, scope, chain) {
  const segs = splitChain(chain);
  let type = null;
  for (let i = 0; i < segs.length; i++) {
    const [seg, indexes] = stripIndexes(segs[i]);
    if (i === 0) type = atomType(model, scope, seg);
    else type = type ? memberType(model, type, seg) : null;
    for (let k = 0; k < indexes && type; k++) type = elemOf(type);
    if (!type) return null;
  }
  return type;
}

function atomType(model, scope, seg) {
  if (seg === "self") return scope.cls;
  if (seg === "super") return scope.cls ? parentOf(model, scope.cls) || null : null;
  if (/^"/.test(seg)) return "str";
  if (/^-?\d+\.\d+$/.test(seg)) return "float";
  if (/^-?\d+$/.test(seg)) return "int";
  if (seg === "true" || seg === "false") return "bool";
  if (/^\[/.test(seg)) {
    const first = seg.slice(1, -1).split(",")[0].trim();
    const t = first ? exprType(model, scope, first) : null;
    return t ? `[${t}]` : null;
  }
  const call = /^([A-Za-z_]\w*)\s*\(/.exec(seg);
  if (call) {
    const n = call[1];
    if (model.classes.has(n)) return n;
    if (model.functions.has(n)) return model.functions.get(n).ret;
    if (BUILTINS[n]) return BUILTINS[n].ret;
    return null;
  }
  if (/^[A-Za-z_]\w*$/.test(seg)) {
    const v = scope.locals.get(seg);
    return v ? v.type : null;
  }
  return null;
}

/** Type of the leading operand of an initializer expression (enough for completion). */
function exprType(model, scope, expr) {
  expr = expr.trim();
  if (!expr) return null;
  // `a + b`: the left operand decides the type for arithmetic and joins.
  const m = /^((?:[^\s+\-*\/%<>=!&|]|\([^)]*\)|\[[^\]]*\])+)\s*([-+*\/%<>=!&|]{1,2})/.exec(expr);
  if (m && !expr.startsWith("[") && !expr.startsWith("-")) {
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

function enclosingClass(model, offset) {
  for (const c of model.classes.values()) if (c.start < offset && offset <= c.end) return c;
  return null;
}

/** Is a declaration at `at` (made at brace depth `d`) still in scope at `offset`? */
function visible(depth, at, d, offset) {
  for (let i = at; i < offset; i++) if (depth[i] < d) return false;
  return true;
}

/** Variables and parameters visible at `offset`, with best-effort types. */
function scopeAt(model, offset) {
  const body = innermostBody(model, offset);
  const cls = body ? body.cls : null;
  const scope = { cls, body, locals: new Map() };
  if (!body) return scope;
  for (const p of body.params) scope.locals.set(p.name, { name: p.name, type: p.type, kind: "param" });

  const { code, depth } = model;
  const region = code.slice(body.start, offset);
  const decls = [];
  const letRe = /\b(let|var)\s+([A-Za-z_]\w*)\s*(?::\s*([^=\n]+?))?\s*=([^\n]*)/g;
  for (let m; (m = letRe.exec(region)); ) {
    const at = body.start + m.index;
    decls.push({ at, d: depth[at], name: m[2], annot: m[3], expr: m[4], kind: m[1] });
  }
  const forRe = /\bfor\s+([A-Za-z_]\w*)\s+in\s+([^{\n]*)\{/g;
  for (let m; (m = forRe.exec(region)); ) {
    const at = body.start + m.index + m[0].length;
    decls.push({ at, d: depth[at], name: m[1], forIn: m[2], kind: "for" });
  }
  const ifLetRe = /\bif\s+let\s+([A-Za-z_]\w*)\s*=\s*([^{\n]*)\{/g;
  for (let m; (m = ifLetRe.exec(region)); ) {
    const at = body.start + m.index + m[0].length;
    decls.push({ at, d: depth[at], name: m[1], unwrap: m[2], kind: "let" });
  }
  decls.sort((a, b) => a.at - b.at);
  for (const v of decls) {
    if (!visible(depth, v.at, v.d, offset)) continue;
    let type = null;
    if (v.annot) type = cleanType(v.annot);
    else if (v.expr !== undefined) type = exprType(model, scope, v.expr);
    else if (v.forIn !== undefined) {
      if (v.forIn.includes("..")) type = "int";
      else {
        const t = exprType(model, scope, v.forIn);
        type = t === "str" ? "str" : elemOf(t);
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

function memberItems(model, type, viaSuper) {
  if (!type) return [];
  const t = type;
  if (t === "str") return STR_METHODS.map((m) => methodItem(m));
  if (isArray(t)) return arrayMethods(elemOf(t)).map((m) => methodItem(m));
  if (model.interfaces.has(t)) return model.interfaces.get(t).methods.map((m) => methodItem(m));
  if (model.classes.has(t)) {
    const items = viaSuper
      ? []
      : classFields(model, t).map((f) => ({
          label: f.name,
          kind: "field",
          detail: `${f.owner}.${f.name}: ${f.type}`,
          documentation: f.doc || "",
          insertText: f.name,
          sortText: "0" + f.name,
        }));
    return items.concat(classMethods(model, t).map((m) => methodItem(m)));
  }
  return [];
}

function typeItems(model, exclude) {
  const items = PRIMITIVES.map((p) => ({ label: p, kind: "type", detail: "built-in type", insertText: p, sortText: "0" + p }));
  for (const c of model.classes.values()) {
    if (c.name !== exclude) items.push({ label: c.name, kind: "class", detail: "class " + c.name, documentation: c.doc, insertText: c.name, sortText: "1" + c.name });
  }
  for (const i of model.interfaces.values()) {
    if (i.name !== exclude) items.push({ label: i.name, kind: "interface", detail: "interface " + i.name, documentation: i.doc, insertText: i.name, sortText: "1" + i.name });
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
    } else if (d === 0 && !/[\w."]/.test(ch)) break;
  }
  return code.slice(i + 1, dot).trim();
}

/**
 * Completion items at `offset` (0-based index into `text`).
 * Each item: { label, kind, detail, documentation, insertText, snippet, sortText }.
 */
function complete(text, offset) {
  const model = analyze(text);
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
    return memberItems(model, chainType(model, scope, recv), recv === "super");
  }

  // Base list: `class Dog : An|`
  const header = /\bclass\s+(\w+)\s*:\s*[\w\s,]*$/.exec(prefix);
  if (header) return typeItems(model, header[1]).filter((i) => i.kind === "class" || i.kind === "interface");

  // Type position: after `name:`, `->`, or `[` in a type
  if (/(?:\b[A-Za-z_]\w*\s*:|->)\s*\[?\s*\w*$/.test(prefix) && !/\bfor\b|\bif\b/.test(prefix)) {
    return typeItems(model, null);
  }

  const depth = model.depth[offset];
  for (const i of model.interfaces.values()) {
    if (i.start < offset && offset <= i.end) {
      return [{ label: "fn", kind: "keyword", detail: "method signature", insertText: "fn ${1:name}(${2}) -> ${3:str}", snippet: true, sortText: "0fn" }];
    }
  }
  const cls = enclosingClass(model, offset);
  const body = innermostBody(model, offset);
  const items = [];

  // Class body, between members.
  if (cls && !body && depth === model.depth[cls.start] + 1) {
    items.push(
      { label: "fn", kind: "keyword", detail: "method", insertText: "fn ${1:name}(${2}) -> ${3:void} {\n\t$0\n}", snippet: true, sortText: "0fn" },
      { label: "init", kind: "keyword", detail: "constructor", insertText: "init(${1}) {\n\t$0\n}", snippet: true, sortText: "0init" }
    );
    const parent = parentOf(model, cls.name);
    if (parent) {
      const own = new Set(cls.methods.map((m) => m.name));
      for (const m of classMethods(model, parent)) {
        if (own.has(m.name)) continue;
        const ret = m.ret && m.ret !== "void" ? ` -> ${m.ret}` : "";
        items.push({
          label: `override fn ${m.name}`,
          kind: "method",
          detail: `override ${m.owner}.${m.name}${sigText(m.params, m.ret)}`,
          documentation: m.doc || "",
          insertText: `override fn ${m.name}(${m.params.map((p) => `${p.name}: ${p.type}`).join(", ")})${ret} {\n\t$0\n}`,
          snippet: true,
          sortText: "1" + m.name,
        });
      }
    }
    return items;
  }

  // Top level.
  if (depth === 0) {
    for (const k of ["fn", "class", "interface"]) items.push({ label: k, kind: "keyword", insertText: k, sortText: "0" + k });
    return items;
  }

  // Inside a function or method body.
  const scope = scopeAt(model, offset);
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
    items.push({ label: f.name, kind: "function", detail: `fn ${f.name}${sigText(f.params, f.ret)}`, documentation: f.doc, insertText: callSnippet(f.name, f.params), snippet: true, sortText: "2" + f.name });
  }
  for (const c of model.classes.values()) {
    const init = findInit(model, c.name);
    const params = init ? init.params : [];
    items.push({ label: c.name, kind: "constructor", detail: `${c.name}${sigText(params, "")}`, documentation: c.doc, insertText: callSnippet(c.name, params), snippet: true, sortText: "3" + c.name });
  }
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
function signatureHelp(text, offset) {
  const model = analyze(text);
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

  const before = code.slice(0, open).replace(/\s+$/, "");
  const nameMatch = /([A-Za-z_]\w*)$/.exec(before);
  if (!nameMatch) return null;
  const name = nameMatch[1];
  const head = before.slice(0, before.length - name.length);
  const scope = scopeAt(model, offset);

  let target = null; // { name, params, ret, doc }
  if (/\.\s*$/.test(head)) {
    const recv = receiverBefore(code, head.replace(/\s*$/, "").length - 1);
    const type = chainType(model, scope, recv);
    target = memberItemsRaw(model, type).find((m) => m.name === name) || null;
  } else if (name === "super" && scope.cls) {
    const parent = parentOf(model, scope.cls);
    const init = parent && findInit(model, parent);
    if (init) target = { name: "super", params: init.params, ret: "void", doc: init.doc };
  } else if (model.classes.has(name)) {
    const init = findInit(model, name);
    target = { name, params: init ? init.params : [], ret: "", doc: model.classes.get(name).doc };
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

function memberItemsRaw(model, type) {
  if (!type) return [];
  if (type === "str") return STR_METHODS;
  if (isArray(type)) return arrayMethods(elemOf(type));
  if (model.interfaces.has(type)) return model.interfaces.get(type).methods;
  if (model.classes.has(type)) return classMethods(model, type);
  return [];
}

module.exports = { analyze, complete, signatureHelp, scopeAt, chainType };
