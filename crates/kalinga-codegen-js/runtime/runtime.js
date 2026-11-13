// Kalinga runtime for JavaScript. Every generated program starts with this file. It gives
// JavaScript values Kalinga's rules: ints that stop the program instead of losing digits,
// arrays, maps and structs that are copied (not shared), sorted maps, Rust-style printing,
// and runtime errors with the same messages as a native build.
const $kl = (() => {
  const isNode = typeof process !== "undefined" && process.versions != null && process.versions.node != null;

  class KlError extends Error {}
  /** The early return of `value?`, caught by the function it is in. */
  class Ret {
    constructor(v) {
      this.v = v;
    }
  }
  const fail = (msg) => {
    throw new KlError(msg);
  };

  // ---------------------------------------------------------------- ints
  const MAX = Number.MAX_SAFE_INTEGER;
  const checked = (r, what) => (r > MAX || r < -MAX ? fail(`attempt to ${what} with overflow`) : r);
  const add = (a, b) => checked(a + b, "add");
  const sub = (a, b) => checked(a - b, "subtract");
  const mul = (a, b) => checked(a * b, "multiply");
  const idiv = (a, b) => (b === 0 ? fail("attempt to divide by zero") : checked(Math.trunc(a / b), "divide"));
  const irem = (a, b) => (b === 0 ? fail("attempt to calculate the remainder with a divisor of zero") : a % b);
  const toInt = (x) => (Number.isNaN(x) ? 0 : x >= MAX ? MAX : x <= -MAX ? -MAX : Math.trunc(x));

  // ---------------------------------------------------------------- maps
  const keyCmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
  /** A Kalinga map `{K: V}`: keys are kept in order, like Rust's BTreeMap. */
  class KlMap {
    constructor(entries) {
      this.m = new Map(entries);
      this.order = null;
    }
    get size() {
      return this.m.size;
    }
    keys() {
      if (this.order === null) this.order = [...this.m.keys()].sort(keyCmp);
      return this.order;
    }
    set(k, v) {
      if (!this.m.has(k)) this.order = null;
      this.m.set(k, v);
    }
    delete(k) {
      if (this.m.delete(k)) this.order = null;
    }
  }
  const map = (entries) => new KlMap(entries);
  const mget = (m, k, show) => (m.m.has(k) ? clone(m.m.get(k)) : fail(`key not found: ${show(k)}`));
  /** `m[k]` read in place (no copy), for changing what is stored there. */
  const mref = (m, k, show) => (m.m.has(k) ? m.m.get(k) : fail(`key not found: ${show(k)}`));
  const mput = (m, k, v, show) => (m.m.has(k) ? m.m.set(k, v) : fail(`key not found: ${show(k)}`));
  const mopt = (m, k) => (m.m.has(k) ? clone(m.m.get(k)) : null);
  const mremove = (m, k) => {
    if (!m.m.has(k)) return null;
    const v = m.m.get(k);
    m.delete(k);
    return v;
  };
  const mvalues = (m) => m.keys().map((k) => clone(m.m.get(k)));
  const mentries = (m) => m.keys().map((k) => [k, clone(m.m.get(k))]);

  // ---------------------------------------------------------------- enums
  /** A value of an enum (also `Result`): its variant and fields. Enum values never change. */
  class E {
    constructor(t, f) {
      this.t = t;
      this.f = f;
    }
    __clone() {
      return this.f.length === 0 ? this : new E(this.t, this.f.map(clone));
    }
  }
  const ok = (v) => new E("Ok", [v]);
  const err = (e) => new E("Err", [e]);

  // ---------------------------------------------------------------- copies
  /** A copy of a value: arrays, maps, structs and enums are values; objects are shared. */
  function clone(v) {
    if (v === null || typeof v !== "object") return v;
    if (Array.isArray(v)) return v.map(clone);
    if (v instanceof KlMap) return new KlMap([...v.m].map(([k, x]) => [k, clone(x)]));
    if (typeof v.__clone === "function") return v.__clone();
    return v;
  }

  // ---------------------------------------------------------------- equality and order
  function eq(a, b) {
    if (a === b) return true;
    if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
    if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => eq(x, b[i]));
    if (a instanceof KlMap) {
      if (!(b instanceof KlMap) || a.size !== b.size) return false;
      return a.keys().every((k) => b.m.has(k) && eq(a.m.get(k), b.m.get(k)));
    }
    if (a instanceof E) return b instanceof E && a.t === b.t && eq(a.f, b.f);
    if (typeof a.__eq === "function") return a.__eq(b);
    return false;
  }
  function cmp(a, b) {
    if (typeof a === "number" || typeof a === "string" || typeof a === "boolean") return a < b ? -1 : a > b ? 1 : 0;
    if (a === b) return 0;
    if (a !== null && typeof a.__cmp === "function") return a.__cmp(b);
    return 0;
  }
  const sorted = (cmpf) => (xs) => {
    xs.sort(cmpf);
  };
  const sort = (xs, cmpf) => {
    xs.sort(cmpf);
  };
  const sortBy = (xs, less) => {
    xs.sort((a, b) => (less(clone(a), clone(b)) ? -1 : less(clone(b), clone(a)) ? 1 : 0));
  };
  const numCmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

  // ---------------------------------------------------------------- printing
  const sI = (n) => String(n);
  /** A float the way Rust prints it: `3.0`, `0.1`, `100000000000000000000`, `inf`. */
  function sF(x) {
    if (Number.isNaN(x)) return "NaN";
    if (x === Infinity) return "inf";
    if (x === -Infinity) return "-inf";
    if (Number.isInteger(x) && Math.abs(x) < 1e16) return (Object.is(x, -0) ? "-" : "") + x.toFixed(1);
    const s = String(x);
    return s.includes("e") ? expand(s) : s;
  }
  /** `1e+21` → `1000000000000000000000`, `1.5e-7` → `0.00000015`. */
  function expand(s) {
    const neg = s.startsWith("-");
    const [mant, expStr] = (neg ? s.slice(1) : s).split("e");
    const exp = Number(expStr);
    const [int, frac = ""] = mant.split(".");
    const digits = int + frac;
    const point = int.length + exp;
    let out;
    if (point <= 0) out = "0." + "0".repeat(-point) + digits;
    else if (point >= digits.length) out = digits + "0".repeat(point - digits.length);
    else out = digits.slice(0, point) + "." + digits.slice(point);
    return (neg ? "-" : "") + out;
  }
  const sB = (b) => String(b);
  const sS = (s) => s;
  const sV = () => "";
  /** A value of a type parameter: its type is not known here, so numbers print as ints when whole. */
  function sAny(v) {
    if (v === null) return "none";
    if (v === undefined) return "";
    if (typeof v === "number") return Number.isInteger(v) ? sI(v) : sF(v);
    if (typeof v === "string") return v;
    if (typeof v === "boolean") return sB(v);
    if (Array.isArray(v)) return "[" + v.map(sAny).join(", ") + "]";
    if (v instanceof KlMap) return "{" + v.keys().map((k) => sAny(k) + ": " + sAny(v.m.get(k))).join(", ") + "}";
    if (v instanceof E) return v.f.length === 0 ? v.t : v.t + "(" + v.f.map(sAny).join(", ") + ")";
    if (typeof v.__show === "function") return v.__show();
    return String(v);
  }
  const sArr = (s) => (xs) => "[" + xs.map(s).join(", ") + "]";
  const sMap = (sk, sv) => (m) => "{" + m.keys().map((k) => sk(k) + ": " + sv(m.m.get(k))).join(", ") + "}";
  const sOpt = (s) => (v) => (v === null ? "none" : s(v));
  const sTup = (ss) => (t) => "(" + t.map((x, i) => ss[i](x)).join(", ") + ")";
  const sRes = (st, se) => (r) => (r.t === "Ok" ? `Ok(${st(r.f[0])})` : `Err(${se(r.f[0])})`);
  const sObj = (v) => v.__show();
  const sFn = () => "<fn>";

  // ---------------------------------------------------------------- arrays and strings
  const ix = (xs, i) =>
    i < 0
      ? fail(`index ${i} is negative`)
      : i >= xs.length
        ? fail(`index out of bounds: the len is ${xs.length} but the index is ${i}`)
        : i;
  const at = (xs, i) => clone(xs[ix(xs, i)]);
  /** `xs[i]` read in place (no copy), for changing what is stored there. */
  const aref = (xs, i) => xs[ix(xs, i)];
  const amap = (xs, f) => xs.map((x) => f(clone(x)));
  const afilter = (xs, f) => xs.filter((x) => f(clone(x))).map(clone);
  const afind = (xs, f) => {
    const x = xs.find((x) => f(clone(x)));
    return x === undefined ? null : clone(x);
  };
  const contains = (xs, v) => xs.some((x) => eq(x, v));
  const pop = (xs) => (xs.length === 0 ? null : xs.pop());
  const wide = /[\uD800-\uDFFF]/;
  const chars = (s) => (wide.test(s) ? [...s] : s.split(""));
  const slen = (s) => (wide.test(s) ? [...s].length : s.length);
  function charAt(s, i) {
    const cs = chars(s);
    if (i < 0 || i >= cs.length)
      fail(`index out of bounds: the string has ${cs.length} characters but the index is ${i}`);
    return cs[i];
  }
  function codeAt(s, i) {
    return charAt(s, i).codePointAt(0);
  }
  function range(len, from, to) {
    const a = from === null ? 0 : from;
    const b = to === null ? len : to;
    if (a < 0 || b < a || b > len) fail(`slice ${a}..${b} is out of bounds for length ${len}`);
    return [a, b];
  }
  const sliceS = (s, from, to) => {
    const cs = chars(s);
    const [a, b] = range(cs.length, from, to);
    return cs.slice(a, b).join("");
  };
  const sliceA = (xs, from, to) => {
    const [a, b] = range(xs.length, from, to);
    return xs.slice(a, b).map(clone);
  };
  const split = (s, sep) => (sep === "" ? chars(s) : s.split(sep));
  const find = (s, x) => {
    const at = s.indexOf(x);
    return at < 0 ? null : slen(s.slice(0, at));
  };
  const chr = (code) => {
    if (code < 0 || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) fail(`${code} is not a character code`);
    return String.fromCodePoint(code);
  };
  const intText = /^[+-]?\d+$/;
  const parseInt_ = (s) => {
    const t = s.trim();
    if (!intText.test(t)) return null;
    const n = Number(t);
    return Number.isSafeInteger(n) ? n : null;
  };
  const floatText = /^[+-]?(\d+\.?\d*([eE][+-]?\d+)?|\.\d+([eE][+-]?\d+)?|inf|infinity|nan)$/i;
  const parseFloat_ = (s) => {
    const t = s.trim();
    if (!floatText.test(t)) return null;
    const l = t.toLowerCase().replace(/^\+/, "");
    if (l === "inf" || l === "infinity") return Infinity;
    if (l === "-inf" || l === "-infinity") return -Infinity;
    if (l.endsWith("nan")) return NaN;
    return Number(t);
  };
  const toIntS = (s) => {
    const n = parseInt_(s);
    return n === null ? fail(`cannot convert "${s}" to int`) : n;
  };
  const toFloatS = (s) => {
    const n = parseFloat_(s);
    return n === null ? fail(`cannot convert "${s}" to float`) : n;
  };

  // ---------------------------------------------------------------- optionals and results
  const tryO = (v) => {
    if (v === null) throw new Ret(null);
    return v;
  };
  const tryR = (r) => {
    if (r.t === "Err") throw new Ret(r);
    return r.f[0];
  };
  const unwrap = (r, show) => (r.t === "Ok" ? r.f[0] : fail(`called unwrap() on an error: ${show(r.f[0])}`));
  const okValue = (r) => (r.t === "Ok" ? clone(r.f[0]) : null);
  const errValue = (r) => (r.t === "Err" ? clone(r.f[0]) : null);
  const unwrapOr = (r, d) => (r.t === "Ok" ? clone(r.f[0]) : d);
  const unset = (v, name) => (v === undefined ? fail(`field \`${name}\` was read before it was set`) : v);

  // ---------------------------------------------------------------- math
  const round = (x) => (x < 0 ? -Math.round(-x) : Math.round(x));
  const fsign = (x) => (Number.isNaN(x) ? NaN : x > 0 || Object.is(x, 0) ? 1 : -1);
  const clamp = (x, lo, hi) => (x < lo ? lo : x > hi ? hi : x);
  const lerp = (a, b, t) => a + (b - a) * t;

  // ---------------------------------------------------------------- assertions
  const assert = (cond, at, msg) => {
    if (!cond) fail(msg === null ? at : `${at}: ${msg}`);
  };
  const assertEq = (l, r, same, show, at) => {
    if (!same(l, r)) fail(`${at}: got ${show(l)}, expected ${show(r)}`);
  };

  // ---------------------------------------------------------------- random (xorshift64*, as native builds)
  const M64 = (1n << 64n) - 1n;
  let rng = (BigInt(Date.now()) * 6364136223846793005n ^ 0x9e3779b97f4a7c15n) | 1n;
  function u64() {
    let x = rng;
    x ^= x >> 12n;
    x ^= (x << 25n) & M64;
    x ^= x >> 27n;
    rng = x;
    return (x * 0x2545f4914f6cdd1dn) & M64;
  }
  const seed = (n) => {
    rng = ((BigInt.asUintN(64, BigInt(n)) ^ 0x9e3779b97f4a7c15n) | 1n) & M64;
  };
  const randInt = (lo, hi) => {
    if (hi <= lo) fail(`Random.int(${lo}, ${hi}): the second number must be greater than the first`);
    return lo + Number(u64() % BigInt(hi - lo));
  };
  const randFloat = () => Number(u64() >> 11n) / 2 ** 53;
  const shuffle = (xs) => {
    for (let i = xs.length - 1; i >= 1; i--) {
      const j = randInt(0, i + 1);
      [xs[i], xs[j]] = [xs[j], xs[i]];
    }
  };
  const pick = (xs) => (xs.length === 0 ? null : clone(xs[randInt(0, xs.length)]));

  // ---------------------------------------------------------------- time
  const started = typeof performance !== "undefined" ? performance.now() : Date.now();
  const now = () => ((typeof performance !== "undefined" ? performance.now() : Date.now()) - started) / 1000;
  const millis = () => Date.now();
  function sleep(seconds) {
    if (!(seconds > 0)) return;
    if (typeof SharedArrayBuffer !== "undefined" && typeof Atomics !== "undefined" && isNode) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, seconds * 1000);
    } else {
      const end = Date.now() + seconds * 1000;
      while (Date.now() < end) {}
    }
  }

  // ---------------------------------------------------------------- async
  /** An `Async<T>`: runs when first awaited (or spawned), like a native build. */
  class Job {
    constructor(start) {
      this.start = start;
      this.p = null;
    }
    run() {
      if (this.p === null) this.p = this.start();
      return this.p;
    }
  }
  const lazy = (start) => new Job(start);
  const run = (job) => job.run();
  const wait = (seconds) => lazy(() => new Promise((r) => setTimeout(r, Math.max(0, seconds) * 1000)));
  const spawn = (job) => {
    const p = Promise.resolve().then(() => job.run());
    return lazy(() => p);
  };
  const all = (jobs) => lazy(() => Promise.all(jobs.map((j) => j.run())));

  // ---------------------------------------------------------------- output
  let pending = "";
  function print(text) {
    if (isNode) process.stdout.write(text);
    else {
      const lines = (pending + text).split("\n");
      pending = lines.pop();
      for (const l of lines) console.log(l);
    }
  }
  function println(text) {
    if (isNode) process.stdout.write(text + "\n");
    else {
      console.log(pending + text);
      pending = "";
    }
  }
  const eprintln = (text) => (isNode ? process.stderr.write(text + "\n") : console.error(text));
  const args = () => (isNode ? process.argv.slice(2) : []);
  const exit = (code) => {
    if (isNode) process.exitCode = code;
  };

  /** What a runtime error says. */
  function message(e) {
    if (e instanceof KlError) return e.message;
    if (e instanceof RangeError && /call stack/.test(e.message)) return "stack overflow (a function called itself too deeply)";
    return "internal error: " + (e && e.stack ? e.stack : String(e));
  }
  /** Runs `main` (or an async `main`); a runtime error stops the program with code 101. */
  function main(f, onResult) {
    const done = (r) => {
      if (onResult) onResult(r);
    };
    const stop = (e) => {
      eprintln("runtime error: " + message(e));
      exit(101);
    };
    try {
      const r = f();
      if (r instanceof Job) r.run().then(done, stop);
      else if (r instanceof Promise) r.then(done, stop);
      else done(r);
    } catch (e) {
      stop(e);
    }
  }
  /** A `main` that returns a `Result`: an error is printed and the program exits with 1. */
  const mainResult = (show) => (r) => {
    if (r.t === "Err") {
      eprintln("error: " + show(r.f[0]));
      exit(1);
    }
  };
  /** `kalinga test`: each test starts with fresh `static var`s; reports like `cargo test`. */
  async function tests(list, reset) {
    println(`running ${list.length} test${list.length === 1 ? "" : "s"}`);
    const failures = [];
    for (const [name, f] of list) {
      reset();
      try {
        const r = f();
        if (r instanceof Job) await r.run();
        println(`test ${name} ... ok`);
      } catch (e) {
        println(`test ${name} ... FAILED`);
        failures.push([name, message(e)]);
      }
    }
    if (failures.length > 0) {
      println("\nfailures:");
      for (const [name, msg] of failures) println(`    ${name}: ${msg}`);
    }
    const result = failures.length === 0 ? "ok" : "FAILED";
    println(`\ntest result: ${result}. ${list.length - failures.length} passed; ${failures.length} failed`);
    if (failures.length > 0) exit(1);
  }
  /** A feature a web build does not have. */
  const unsupported = (what) => fail(`${what} is not available in a web build`);

  return {
    KlError, Ret, fail, aref, amap, afilter, afind, contains, okValue, errValue, unwrapOr, lerp, assert, assertEq, add, sub, mul, idiv, irem, toInt, KlMap, map, mget, mref, mput, mopt, mremove,
    mvalues, mentries, E, ok, err, clone, eq, cmp, sorted, sort, sortBy, numCmp, sI, sF, sB, sS, sV, sAny,
    sArr, sMap, sOpt, sTup, sRes, sObj, sFn, ix, at, pop, chars, slen, charAt, codeAt, sliceS, sliceA, split,
    find, chr, parseInt: parseInt_, parseFloat: parseFloat_, toIntS, toFloatS, tryO, tryR, unwrap, unset,
    round, fsign, clamp, seed, randInt, randFloat, shuffle, pick, now, millis, sleep, Job, lazy, run, wait,
    spawn, all, print, println, eprintln, args, exit, main, mainResult, tests, unsupported,
  };
})();
