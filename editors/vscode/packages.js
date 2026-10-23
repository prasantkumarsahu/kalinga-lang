// Where `import "..."` paths point, the same way the compiler resolves them:
//   "file.kl"        next to the importing file
//   "std/game"       a standard module (the compiler writes these to a temp folder)
//   "mathx", "mathx/vec"   a utkal package: its src/lib.kl or src/vec.kl
//
// Packages come from the nearest utkal.toml (path dependencies) and utkal.lock (git
// dependencies, checked out by `utkal build` into ~/.utkal/cache/src/<name>-<commit>).

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");

/** The folder holding utkal.toml at or above `dir`, or null. */
function findProject(dir, exists = fs.existsSync) {
  let cur = path.resolve(dir);
  for (;;) {
    if (exists(path.join(cur, "utkal.toml"))) return cur;
    const up = path.dirname(cur);
    if (up === cur) return null;
    cur = up;
  }
}

/** The lines of `[section]` in a TOML text. */
function section(text, name) {
  const out = [];
  let inside = false;
  for (const line of text.split(/\r?\n/)) {
    const header = /^\s*\[([^\]]+)\]\s*$/.exec(line);
    if (header) inside = header[1].trim() === name;
    else if (inside) out.push(line);
  }
  return out;
}

const tomlString = (src, key) => {
  const m = new RegExp(`\\b${key}\\s*=\\s*(?:"([^"]*)"|'([^']*)')`).exec(src);
  return m ? (m[1] !== undefined ? m[1] : m[2]) : null;
};

/**
 * Package name → folder for the project in `projectDir`, including packages that path
 * packages depend on. `readFile(p)` returns a file's text or null.
 */
function packageDirs(projectDir, readFile, home) {
  const dirs = new Map();
  const lock = readFile(path.join(projectDir, "utkal.lock")) || "";
  const commits = new Map();
  for (const block of lock.split(/\[\[package\]\]/).slice(1)) {
    const name = tomlString(block, "name");
    const commit = tomlString(block, "commit");
    if (name && commit) commits.set(name, commit);
  }
  for (const [name, commit] of commits) dirs.set(name, path.join(home, "cache", "src", `${name}-${commit.slice(0, 12)}`));

  const queue = [projectDir];
  const seen = new Set();
  while (queue.length) {
    const dir = queue.shift();
    if (seen.has(dir)) continue;
    seen.add(dir);
    const manifest = readFile(path.join(dir, "utkal.toml"));
    if (!manifest) continue;
    for (const line of section(manifest, "dependencies")) {
      const m = /^\s*"?([A-Za-z][\w-]*)"?\s*=\s*\{(.*)\}\s*$/.exec(line);
      if (!m) continue;
      const p = tomlString(m[2], "path");
      if (p !== null && !dirs.has(m[1])) dirs.set(m[1], path.resolve(dir, p));
      const pkgDir = dirs.get(m[1]);
      if (pkgDir) queue.push(pkgDir);
    }
  }
  return dirs;
}

/** The newest `std-<version>` folder the compiler has written, or null. */
function stdDir(tmp = os.tmpdir(), list = (d) => fs.readdirSync(d)) {
  const base = path.join(tmp, "kalinga-build");
  let names;
  try {
    names = list(base).filter((n) => /^std-\d+\.\d+\.\d+$/.test(n));
  } catch {
    return null;
  }
  const key = (n) => n.slice(4).split(".").map(Number);
  names.sort((a, b) => {
    const [x, y] = [key(a), key(b)];
    return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
  });
  return names.length ? path.join(base, names[names.length - 1]) : null;
}

/** `{ packages, std }` for a file in `dir`. */
function context(dir, readFile = readOrNull, home = utkalHome()) {
  const project = findProject(dir);
  return { packages: project ? packageDirs(project, readFile, home) : new Map(), std: stdDir() };
}

/** The file an import names, or null when it cannot be found. */
function resolveImport(importPath, dir, ctx) {
  if (importPath.startsWith("std/")) {
    const mod = importPath.slice(4).replace(/\.kl$/, "");
    return ctx.std ? path.join(ctx.std, mod + ".kl") : null;
  }
  if (importPath.endsWith(".kl")) return path.resolve(dir, importPath);
  const slash = importPath.indexOf("/");
  const [pkg, rest] = slash < 0 ? [importPath, "lib"] : [importPath.slice(0, slash), importPath.slice(slash + 1)];
  const pkgDir = ctx.packages.get(pkg);
  return pkgDir ? path.join(pkgDir, "src", rest + ".kl") : null;
}

function utkalHome() {
  return process.env.UTKAL_HOME || path.join(os.homedir(), ".utkal");
}

function readOrNull(p) {
  try {
    return fs.readFileSync(p, "utf8");
  } catch {
    return null;
  }
}

module.exports = { findProject, packageDirs, stdDir, context, resolveImport };
