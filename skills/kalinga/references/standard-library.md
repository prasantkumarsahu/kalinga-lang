# Kalinga standard library and tools

## Contents

- [Math, Random, Time](#math-random-time)
- [Threads: Thread, Channel, Shared](#threads-thread-channel-shared)
- [Async](#async)
- [Files: std/fs](#files-stdfs)
- [2D math: std/math2d](#2d-math-stdmath2d)
- [Reactive values: std/reactive](#reactive-values-stdreactive)
- [Tests](#tests)
- [Modules and packages](#modules-and-packages)
- [Rust interop](#rust-interop)

## Math, Random, Time

Always available, used like static methods: `Math.sqrt(2.0)`.

| `Math.` | Returns | Notes |
|---|---|---|
| `sqrt(x)`, `pow(x, y)`, `exp(x)`, `ln(x)`, `log10(x)` | `float` | |
| `sin`, `cos`, `tan`, `asin`, `acos`, `atan` | `float` | radians |
| `atan2(y, x)`, `hypot(x, y)` | `float` | |
| `floor(x)`, `ceil(x)`, `round(x)` | `float` | use `int(...)` for an `int` |
| `lerp(a, b, t)` | `float` | `a + (b - a) * t` |
| `abs(x)`, `sign(x)` | same as `x` | `int` or `float` |
| `min(a, b)`, `max(a, b)`, `clamp(x, lo, hi)` | same as the arguments | all `int` or all `float` |
| `Math.PI`, `Math.TAU`, `Math.E`, `Math.INF` | `float` | constants |

| `Random.` | Returns | Notes |
|---|---|---|
| `int(lo, hi)` | `int` | `lo` included, `hi` excluded |
| `float()` | `float` | from 0.0 up to 1.0 |
| `chance(p)` | `bool` | `true` with probability `p` |
| `pick(items)` | `T?` | `none` for an empty array |
| `shuffle(items)` | — | a `var` array, in place |
| `seed(n)` | — | the same seed gives the same numbers |

| `Time.` | Returns | Notes |
|---|---|---|
| `now()` | `float` | seconds since the program started |
| `millis()` | `int` | milliseconds since 1970 |
| `sleep(seconds)` | — | blocks the thread |
| `wait(seconds)` | `Async<void>` | inside `async fn`: `await Time.wait(0.5)` |

## Threads: Thread, Channel, Shared

`Thread.spawn(fn() => ...)` runs a lambda on another thread and gives a `Task<T>`;
`task.join()` waits for its result. Only **values** cross threads (numbers, strings,
arrays, maps, structs, enums, optionals, `Channel`, `Shared`): an object, a function value
or a `var` that a lambda changes cannot be used inside a thread lambda.

```kalinga
fn work(n: int) -> int {
    var acc = 0
    for i in 0..n {
        acc = (acc + i * i) % 1000003
    }
    return acc
}

fn main() {
    let a = Thread.spawn(fn() => work(1000))
    let b = Thread.spawn(fn() => work(2000))
    println(a.join() == work(1000), b.join() == work(2000))

    let counter = Shared(0)
    var tasks: [Task<void>] = []
    for i in 0..4 {
        tasks.push(Thread.spawn(fn() {
            for k in 0..100 {
                counter.update(fn(c) => c + 1)
            }
        }))
    }
    for t in tasks {
        t.join()
    }
    println("counter", counter.get())

    let jobs = Channel<int>()
    let worker = Thread.spawn(fn() -> int {
        var sum = 0
        while let j = jobs.receive() {
            sum += j
        }
        return sum
    })
    for i in 1..4 {
        jobs.send(i)
    }
    jobs.close()
    println("sum", worker.join())
}
```

```text
true true
counter 400
sum 6
```

- `Channel<T>()`: `send(x)`, `receive()` (waits; `none` once closed and empty),
  `try_receive()`, `close()`. Copies of a channel are the same channel.
- `Shared(v)`: `get()`, `set(v)`, `update(fn(old) => new)` (atomic).
- `xs.par_map(fn(x) => ...)` maps an array on all cores. `Thread.cores()` counts them.

## Async

Single-threaded jobs that wait without blocking each other.

```kalinga
async fn delayed(name: str, seconds: float) -> str {
    await Time.wait(seconds)
    return name
}

async fn main() {
    let first = await delayed("one", 0.01)
    let all = await Async.all([delayed("a", 0.02), delayed("b", 0.01)])
    println(first, all)
}
```

```text
one [a, b]
```

- Calling an `async fn` gives an `Async<T>`; `await` runs it (only inside an `async fn`,
  not inside a lambda). `async fn main()` is allowed.
- `Async.all(jobs)` waits for all (results in order), `Async.spawn(job)` starts one in the
  background (`await` it later), `Async.run(job)` runs one from non-async code.
- `async` methods must be `static`; instance `async fn` methods are not supported.

## Files: std/fs

`import "std/fs"`. Everything that can fail returns a `Result` with a readable error.

| Function | Returns |
|---|---|
| `File.read(path)` / `File.read_lines(path)` | `Result<str, str>` / `Result<[str], str>` |
| `File.write(path, text)` / `File.append(path, text)` | `Result<void, str>` |
| `File.exists(path)` | `bool` |
| `File.delete(path)`, `File.copy(from, to)`, `File.rename(from, to)` | `Result<void, str>` |
| `Dir.list(path)` | `Result<[str], str>` (sorted names) |
| `Dir.create(path)` / `Dir.delete(path)` | `Result<void, str>` |
| `Dir.exists(path)` | `bool` |
| `Path.join(a, b)`, `Path.name(p)`, `Path.parent(p)`, `Path.extension(p)` | `str` |

```kalinga
import "std/fs"

fn save(path: str, lines: [str]) -> Result<int, str> {
    File.write(path, lines.join("\n"))?
    let back = File.read_lines(path)?
    return Ok(len(back))
}

fn main() {
    match save("notes.txt", ["milk", "eggs"]) {
        Ok(n) => println("saved", n, "lines"),
        Err(e) => println("error:", e),
    }
}
```

## 2D math: std/math2d

`import "std/math2d"` gives two structs (values, copied on assignment):

- `Vec2(x, y)`: `+`, `-`, `* k`, `/ k`, unary `-`; `length()`, `dot(o)`, `distance(o)`,
  `normalized()`, `angle()`, `lerp(to, t)`; `Vec2.zero()`, `Vec2.from_angle(radians, length)`.
- `Rect(x, y, w, h)`: `center()`, `contains(p)`, `overlaps(o)`, `moved(by)`.

`std/game` (windows, drawing, input, images, sound) builds on it; see the Kalinga docs
chapter "Making a Game" before using it.

## Reactive values: std/reactive

`import "std/reactive"`: values that keep each other up to date, with no garbage
collector.

```kalinga
import "std/reactive"

fn main() {
    let count = signal(1)
    let doubled = computed(fn() => count.get() * 2)
    effect(fn() {
        println("count is {count.get()}, doubled is {doubled.get()}")
    })
    count.set(5)
    batch(fn() {
        count.set(6)
        count.set(7)
    })
    let page = scope()
    page.run(fn() {
        effect(fn() {
            println("page sees", count.get())
        })
    })
    page.dispose()
}
```

```text
count is 1, doubled is 2
count is 5, doubled is 10
count is 7, doubled is 14
page sees 7
```

- `signal(v)`: `get()`, `set(v)`, `update(fn(old) => new)`, `peek()` (read without
  depending).
- `computed(fn() => ...)`: `get()`; lazy and cached, worked out again only after what it
  read changed.
- `effect(fn() { ... })`: runs now and after each change of what it read; returns an
  `Effect` with `dispose()`.
- `batch(fn() { ... })`: effects run once, after it ends.
- `scope()`: `run(fn() { ... })` makes the values inside belong to it; `dispose()` frees
  them (their effects stop; using a disposed signal stops the program).
- These are small handles (structs): copies are the same signal. Use the functions
  `signal(...)` and `computed(...)`, not `Signal(...)`.

## Tests

`test fn` blocks take no parameters; `assert(cond[, message])` and
`assert_eq(value, expected)` check results. Run them with `kalinga test file.kl` (or
`utkal test` in a project).

```kalinga
fn clamp(value: int, low: int, high: int) -> int {
    if value < low {
        return low
    }
    return if value > high { high } else { value }
}

test fn keeps_values_in_range() {
    assert_eq(clamp(5, 0, 10), 5)
    assert_eq(clamp(-3, 0, 10), 0)
    assert(clamp(99, 0, 10) == 10, "values above the range become the upper bound")
}
```

## Modules and packages

- `import "shapes.kl"` brings in another file (relative to this one); everything it
  declares is visible. `import "std/fs"` for a standard module.
- A project made with `utkal new app` has `utkal.toml`, `src/main.kl` and `tests/`.
  `utkal add mathx --git <url> --tag v1.0` or `--path ../mathx` adds a package, used as
  `import "mathx"`; `import "mathx" as m` then `m.Vec2`. `utkal add fastrand --rust 2`
  adds a Rust crate.
- `utkal.toml` has `kalinga = "0.6"`: the language version the code is written for;
  `utkal fix` / `kalinga fix` update older code.

## Rust interop

```kalinga
use rust "native/stats.rs"

extern fn mean(xs: [float]) -> float
extern fn cube_root(x: float) -> float = "f64::cbrt"
extern fn shout(s: str) -> str = rust { s.to_uppercase() + "!" }
extern fn parse_hex(s: str) -> Result<int, str> = rust {
    i64::from_str_radix(&s, 16).map_err(|e| e.to_string())
}
```

- `use rust "file.rs"` includes a Rust file; `extern fn` declares a Rust function's
  Kalinga signature (`int` is `i64`, `float` `f64`, `str` `String`, `[T]` `Vec<T>`,
  `T?` `Option<T>`, `Result` `Result`).
- `use crate "name" = "1.2"` uses a crates.io crate; `extern type Name` is an opaque Rust
  type.
