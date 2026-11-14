// A small page for running web programs under Node.js (`kalinga run --web`, `kalinga test
// --web`): enough of `document`, `window`, `location`, `history`, `localStorage` and
// `requestAnimationFrame` for std/web, so a program's page logic can be run and tested
// without a browser. `kalinga build --web` does not include it.
if (typeof document === "undefined") {
  (() => {
    const VOID = new Set(["br", "hr", "img", "input", "meta", "link"]);
    const esc = (s, attr) => {
      s = String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
      return attr ? s.replace(/"/g, "&quot;") : s;
    };

    /** Event handling shared by nodes and the window. */
    class Target {
      constructor() {
        this._on = new Map();
      }
      addEventListener(kind, f) {
        if (!this._on.has(kind)) this._on.set(kind, []);
        this._on.get(kind).push(f);
      }
      removeEventListener(kind, f) {
        const fs = this._on.get(kind);
        if (fs) this._on.set(kind, fs.filter((g) => g !== f));
      }
      dispatchEvent(ev) {
        const e = {
          type: ev.type,
          target: ev.target ?? this,
          bubbles: ev.bubbles !== false,
          key: ev.key ?? "",
          clientX: ev.clientX ?? 0,
          clientY: ev.clientY ?? 0,
          button: ev.button ?? 0,
          defaultPrevented: false,
          stopped: false,
          preventDefault() {
            this.defaultPrevented = true;
          },
          stopPropagation() {
            this.stopped = true;
          },
        };
        for (let t = this; t !== null && t !== undefined; t = e.bubbles ? t.parentNode : null) {
          e.currentTarget = t;
          for (const f of [...(t._on.get(e.type) ?? [])]) f(e);
          if (e.stopped) break;
        }
        return !e.defaultPrevented;
      }
    }

    class Node extends Target {
      constructor(type) {
        super();
        this.nodeType = type;
        this.parentNode = null;
        this.childNodes = [];
      }
      get parentElement() {
        return this.parentNode instanceof Element ? this.parentNode : null;
      }
      get children() {
        return this.childNodes.filter((c) => c instanceof Element);
      }
      get textContent() {
        return this.childNodes.map((c) => c.textContent).join("");
      }
      set textContent(text) {
        for (const c of this.childNodes) c.parentNode = null;
        this.childNodes = [];
        if (text !== "") this.appendChild(new Text(text));
      }
      appendChild(c) {
        if (c.parentNode) c.remove();
        c.parentNode = this;
        this.childNodes.push(c);
        return c;
      }
      insertBefore(c, before) {
        if (c.parentNode) c.remove();
        const i = this.childNodes.indexOf(before);
        c.parentNode = this;
        if (i < 0) this.childNodes.push(c);
        else this.childNodes.splice(i, 0, c);
        return c;
      }
      remove() {
        if (this.parentNode) {
          const p = this.parentNode;
          p.childNodes = p.childNodes.filter((c) => c !== this);
          this.parentNode = null;
        }
      }
      querySelector(sel) {
        return this.querySelectorAll(sel)[0] ?? null;
      }
      querySelectorAll(sel) {
        const parts = sel.trim().split(/\s+/).map(parse);
        const out = [];
        const walk = (n) => {
          for (const c of n.children) {
            if (matchesPath(c, parts, this)) out.push(c);
            walk(c);
          }
        };
        walk(this);
        return out;
      }
    }

    class Text extends Node {
      constructor(text) {
        super(3);
        this.data = String(text);
      }
      get textContent() {
        return this.data;
      }
      set textContent(t) {
        this.data = String(t);
      }
      get outerHTML() {
        return esc(this.data, false);
      }
    }

    /** `tag#id.a.b[name=value]` */
    function parse(simple) {
      const m = { tag: null, id: null, classes: [], attrs: [] };
      const re = /([#.]?)([\w-]+)|\[([\w-]+)(?:=["']?([^\]"']*)["']?)?\]/g;
      let x;
      while ((x = re.exec(simple)) !== null) {
        if (x[3] !== undefined) m.attrs.push([x[3], x[4]]);
        else if (x[1] === "#") m.id = x[2];
        else if (x[1] === ".") m.classes.push(x[2]);
        else m.tag = x[2].toLowerCase();
      }
      return m;
    }
    function matches(el, m) {
      if (m.tag !== null && m.tag !== "*" && el.tagName.toLowerCase() !== m.tag) return false;
      if (m.id !== null && el.getAttribute("id") !== m.id) return false;
      if (!m.classes.every((c) => el.classList.contains(c))) return false;
      return m.attrs.every(([k, v]) => (v === undefined ? el.hasAttribute(k) : el.getAttribute(k) === v));
    }
    /** `el` matches the last part, and each earlier part matches an ancestor (inside `root`). */
    function matchesPath(el, parts, root) {
      if (!matches(el, parts[parts.length - 1])) return false;
      let i = parts.length - 2;
      for (let a = el.parentElement; i >= 0 && a !== null && a !== root; a = a.parentElement) {
        if (matches(a, parts[i])) i--;
      }
      return i < 0;
    }

    class Element extends Node {
      constructor(tag) {
        super(1);
        this.tagName = tag.toUpperCase();
        this.attributes = new Map();
        this.value = "";
        this.checked = false;
        const el = this;
        this.classList = {
          list: () => (el.attributes.get("class") ?? "").split(/\s+/).filter((c) => c !== ""),
          set(cs) {
            if (cs.length === 0) el.attributes.delete("class");
            else el.attributes.set("class", cs.join(" "));
          },
          contains(c) {
            return this.list().includes(c);
          },
          add(c) {
            if (!this.contains(c)) this.set([...this.list(), c]);
          },
          remove(c) {
            this.set(this.list().filter((x) => x !== c));
          },
          toggle(c, on) {
            if (on === undefined) on = !this.contains(c);
            if (on) this.add(c);
            else this.remove(c);
            return on;
          },
        };
        this.style = {
          entries: () =>
            (el.attributes.get("style") ?? "")
              .split(";")
              .map((p) => p.split(":").map((s) => s.trim()))
              .filter((p) => p[0] !== ""),
          write(es) {
            if (es.length === 0) el.attributes.delete("style");
            else el.attributes.set("style", es.map(([k, v]) => `${k}: ${v}`).join("; "));
          },
          getPropertyValue(k) {
            return this.entries().find((e) => e[0] === k)?.[1] ?? "";
          },
          setProperty(k, v) {
            const es = this.entries();
            const e = es.find((x) => x[0] === k);
            if (e) e[1] = v;
            else es.push([k, v]);
            this.write(es);
          },
          removeProperty(k) {
            this.write(this.entries().filter((e) => e[0] !== k));
          },
        };
      }
      getAttribute(k) {
        return this.attributes.has(k) ? this.attributes.get(k) : null;
      }
      setAttribute(k, v) {
        this.attributes.set(k, String(v));
      }
      removeAttribute(k) {
        this.attributes.delete(k);
      }
      hasAttribute(k) {
        return this.attributes.has(k);
      }
      get id() {
        return this.getAttribute("id") ?? "";
      }
      focus() {
        document.activeElement = this;
      }
      click() {
        this.dispatchEvent({ type: "click", bubbles: true });
      }
      get outerHTML() {
        const tag = this.tagName.toLowerCase();
        const attrs = [...this.attributes].map(([k, v]) => ` ${k}="${esc(v, true)}"`).join("");
        if (VOID.has(tag)) return `<${tag}${attrs}>`;
        return `<${tag}${attrs}>${this.childNodes.map((c) => c.outerHTML).join("")}</${tag}>`;
      }
    }

    // `new Event(kind, { bubbles })`, as browsers have it.
    class Ev {
      constructor(type, init) {
        this.type = type;
        this.bubbles = init?.bubbles ?? false;
      }
    }

    const doc = new Node(9);
    const html = doc.appendChild(new Element("html"));
    const head = html.appendChild(new Element("head"));
    const body = html.appendChild(new Element("body"));
    const app = body.appendChild(new Element("div"));
    app.setAttribute("id", "app");
    Object.assign(doc, {
      head,
      body,
      documentElement: html,
      title: "",
      activeElement: null,
      createElement: (tag) => new Element(tag),
      createTextNode: (text) => new Text(text),
    });

    const win = new Target();
    win.parentNode = null;
    const loc = { pathname: "/", search: "", hash: "", get href() { return "http://localhost" + this.pathname + this.search + this.hash; } };
    const store = new Map();
    const define = (name, value) => Object.defineProperty(globalThis, name, { value, configurable: true, writable: true });
    define("document", doc);
    define("window", Object.assign(win, { innerWidth: 1024, innerHeight: 768 }));
    define("Event", Ev);
    define("location", loc);
    define("history", {
      pushState: (_s, _t, path) => Object.assign(loc, { pathname: String(path) }),
      replaceState: (_s, _t, path) => Object.assign(loc, { pathname: String(path) }),
    });
    define("localStorage", {
      getItem: (k) => (store.has(k) ? store.get(k) : null),
      setItem: (k, v) => store.set(k, String(v)),
      removeItem: (k) => store.delete(k),
    });
    define("requestAnimationFrame", (f) => setTimeout(() => f(performance.now()), 16));
    define("cancelAnimationFrame", (id) => clearTimeout(id));
  })();
}
