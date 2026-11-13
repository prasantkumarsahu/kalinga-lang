# 29. Web

A Kalinga program can run in a browser. `kalinga build --web` writes it out as JavaScript,
with an HTML page to load it, and `std/web` gives it the page: elements, events, timers,
HTTP, storage and the address bar.

`std/web` is small on purpose. It is the layer a user interface is built on, not a UI
framework: components, routing and styling systems belong in packages built on top of it.
It does not depend on `std/reactive` either; a program that wants values that keep the page
up to date imports both, as below.

## A counter

```kalinga
import "std/reactive"
import "std/web"

fn main() {
    app = Document.app()
    count = signal(0)

    shown = app.append(Document.create("p"))
    effect(fn() {
        shown.set_text("Clicked {count.get()} times")
        shown.toggle_class("big", count.get() >= 10)
    })

    plus = app.append(Document.create("button"))
    plus.set_text("+1")
    plus.on("click", fn(e: Event) { count.update(fn(n) => n + 1) })
}
```

```bash
kalinga build --web counter.kl
```

This writes a `dist` folder next to `counter.kl`:

| File | What it is |
|---|---|
| `dist/index.html` | The page: a `<div id="app">`, the styles and the script |
| `dist/app.js` | The program as JavaScript |
| `dist/*.css` | Every `.css` file next to `counter.kl`, linked from the page |

`-o folder` writes somewhere else. Open `index.html` through any static web server (a
browser runs scripts of a page opened from a file only in some cases), for example
`python -m http.server --directory dist`.

Clicking the button changes `count`; the effect that reads it runs again and changes the
one text it shows. Nothing else on the page is touched: there is no virtual DOM to compare.

The whole example, with its CSS, is in `examples/web/`.

## Styling

Put CSS in `.css` files next to the main file; `kalinga build --web` copies them into
`dist` and links them from the page. From code, give elements classes and styles:

```kalinga
import "std/web"

fn main() {
    note = Document.app().append(Document.create("p"))
    note.set_text("Saved")
    note.add_class("note")
    note.toggle_class("error", false)
    note.set_style("color", "green")
    Document.add_styles(".note {{ font-weight: bold; }}")
}
```

CSS written in a string doubles its braces (`{{` and `}}`), because `{` starts an
inserted value.

## Elements

`Document.create(tag)` makes an element; `append` puts it inside another one and gives it
back, so a new element can be made and placed in one line.

| `Document` | Gives |
|---|---|
| `Document.app()` | The `<div id="app">` of the page (the body when there is none) |
| `Document.body()` | The page's `<body>` |
| `Document.create(tag)` | A new element |
| `Document.text(text)` | A new text node |
| `Document.find(selector)` / `find_all(selector)` | Elements of the page matching a CSS selector |
| `Document.add_styles(css)` | Adds CSS rules to the page |
| `Document.title()` / `set_title(title)` | The page's title |

| `Element` method | Does |
|---|---|
| `text()` / `set_text(text)` | The text inside (`set_text` never reads HTML) |
| `html()` | The element as HTML |
| `tag()` | The tag, in lowercase |
| `attr(name)` / `set_attr(name, value)` / `remove_attr(name)` | Attributes (`attr` gives `str?`) |
| `add_class(name)` / `remove_class(name)` / `toggle_class(name, on)` / `has_class(name)` | Classes |
| `set_style(property, value)` / `remove_style(property)` | Inline styles |
| `value()` / `set_value(value)` | What is typed in an input, text area or select |
| `checked()` / `set_checked(on)` | Whether a checkbox is ticked |
| `append(child)` / `insert_before(child, before)` | Puts elements inside this one |
| `remove()` / `clear()` | Takes this element off the page / empties it |
| `parent()` / `children()` | The element around this one / the ones directly inside |
| `find(selector)` / `find_all(selector)` | Elements inside matching a CSS selector |
| `on(kind, handler)` | Calls `handler` on every `kind` event; gives a `Listener` |
| `focus()` / `click()` / `trigger(kind)` | Focuses it / sends it a click or another event |

## Events

`el.on(kind, fn(e: Event) { ... })` runs the handler on each event of that kind (`"click"`,
`"input"`, `"keydown"`, `"submit"`, ...). It gives back a `Listener`; `listener.remove()`
stops it.

| `Event` method | Gives |
|---|---|
| `kind()` | `"click"`, `"input"`, ... |
| `key()` | The key of a keyboard event: `"a"`, `"Enter"`, `"ArrowLeft"` |
| `value()` | The value of the input the event happened on |
| `x()` / `y()` | The mouse position, in pixels |
| `button()` | The mouse button: 0 left, 1 middle, 2 right |
| `target()` | The element the event happened on (`Element?`) |
| `prevent_default()` | Stops the browser's own action (following a link, sending a form) |
| `stop()` | Stops the event reaching the elements around this one |

```kalinga
import "std/web"

fn main() {
    app = Document.app()
    name = app.append(Document.create("input"))
    greeting = app.append(Document.create("p"))
    name.on("input", fn(e: Event) { greeting.set_text("Hello, {e.value()}!") })
}
```

## Timers and animation

| Call | Does |
|---|---|
| `Timer.after(seconds, f)` | Runs `f` once, later |
| `Timer.every(seconds, f)` | Runs `f` again and again |
| `timer.cancel()` | Stops a timer |
| `Frame.next(fn(time: float) { ... })` | Runs before the next screen update, with the time in seconds |
| `frame.cancel()` | Stops it |

An animation asks for the next frame at the end of each one.

## HTTP

`Http.get(url)`, `Http.post(url, body)` and `Http.send(method, url, body, headers)` are
`async` and give `Result<Response, str>`. The error is a network failure; a response of any
status, 404 included, is `Ok`. A `Response` has the fields `status` and `text`, and the
methods `ok()` (status 200 to 299) and `header(name)` (`str?`).

```kalinga
import "std/web"

async fn load(shown: Element) {
    match await Http.get("/api/items") {
        Ok(r) => shown.set_text(if r.ok() { r.text } else { "error {r.status}" })
        Err(e) => shown.set_text("offline: {e}")
    }
}

fn main() {
    shown = Document.app().append(Document.create("p"))
    Async.run(load(shown))
}
```

## Storage, the address and the window

| Call | Does |
|---|---|
| `Storage.get(key)` / `set(key, value)` / `remove(key)` | Text kept in the browser after the page closes (`get` gives `str?`) |
| `Location.path()` / `query()` / `hash()` / `href()` | Parts of the page's address |
| `History.push(path)` / `replace(path)` | Shows a new address without loading a page |
| `History.on_change(fn(path: str) { ... })` | Runs when the user goes back or forward |
| `Window.width()` / `height()` | The size of the page area, in pixels |
| `Window.on_resize(f)` / `on_key(fn(e: Event) { ... })` | Window events |

## Running without a browser

`kalinga run --web file.kl` runs a web program with Node.js, on a small stand-in page that
has `document`, `window`, `localStorage` and `history`, so a program's logic can be tried
and tested without opening a browser. Clicks can be sent with `el.click()` and other
events with `el.trigger(kind)`; `el.html()` shows what the page holds. `kalinga test --web`
runs `test fn`s the same way.

## Your own JavaScript

Anything `std/web` does not cover can be written as an `extern fn` with a JavaScript body
(chapter 19). The types `JsNode` and `JsEvent` are the browser's own objects; an
`Element`'s `node` field and an `Event`'s `event` field hold them.

```kalinga
import "std/web"

extern fn scroll_to_top(node: JsNode) = js {
    node.scrollTop = 0;
}

fn main() {
    scroll_to_top(Document.body().node)
}
```

## What a web build leaves out

- Only the code the program reaches from `main` is written to `app.js`; unused parts of
  `std/web` and other modules are left out.
- Threads, channels, `Shared`, `par_map`, reading the console, `std/fs`, `std/game`,
  `use rust` and `rust { }` bodies are not available, and are compile errors in a web
  build (chapter 23).
- `std/web` needs a web build: a native `kalinga run` of a program that imports it is an
  error.

---

Previous: [28. Reactive Values](28-reactive.md)
