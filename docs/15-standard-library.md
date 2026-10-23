# 15. Standard Library: Math, Random, Time, Files

Three built-in libraries are always available: `Math`, `Random` and `Time`. They are used
like static methods: `Math.sqrt(2.0)`. Files and folders come with `import "std/fs"`
([below](#files-and-folders-stdfs)).

## Math

```kalinga
fn main() {
    println(Math.sqrt(16), Math.pow(2, 10), Math.hypot(3, 4))
    println(Math.abs(-7), Math.abs(-2.5), Math.min(3, 9), Math.max(0.5, 1.5))
    println(Math.clamp(150, 0, 100), Math.lerp(0, 10, 0.25))
    println(Math.floor(2.7), Math.ceil(2.1), Math.round(2.5))
    println(Math.round(Math.sin(Math.PI / 2.0) * 100.0))
}
```

```text
4.0 1024.0 5.0
7 2.5 3 1.5
100 2.5
2.0 3.0 3.0
100.0
```

| Function | Returns | Notes |
|---|---|---|
| `sqrt(x)`, `pow(x, y)`, `exp(x)`, `ln(x)`, `log10(x)` | `float` | |
| `sin(x)`, `cos(x)`, `tan(x)`, `asin(x)`, `acos(x)`, `atan(x)` | `float` | radians |
| `atan2(y, x)`, `hypot(x, y)` | `float` | angle and length of a vector |
| `floor(x)`, `ceil(x)`, `round(x)` | `float` | `round` rounds halves away from zero; use `int(...)` for an `int` |
| `lerp(a, b, t)` | `float` | `a + (b - a) * t` |
| `abs(x)`, `sign(x)` | same as `x` | `int` or `float` |
| `min(a, b)`, `max(a, b)`, `clamp(x, lo, hi)` | same as arguments | all `int` or all `float` |
| `Math.PI`, `Math.TAU`, `Math.E`, `Math.INF` | `float` | constants |

Functions that return `float` also accept `int` arguments (`Math.sqrt(16)`), as a
convenience. `min`, `max` and `clamp` need all `int` or all `float` values.

## Random

```kalinga
fn main() {
    Random.seed(42)            // optional: makes the sequence repeatable
    let die = Random.int(1, 7) // 1 to 6 (the upper bound is excluded)
    let unit = Random.float()  // 0.0 up to (not including) 1.0
    println(die >= 1 && die <= 6, unit >= 0.0 && unit < 1.0)

    var deck = [1, 2, 3, 4, 5]
    Random.shuffle(deck)
    if let card = Random.pick(deck) {
        println("picked a card:", card >= 1 && card <= 5)
    }
    println(Random.chance(1.0), Random.chance(0.0))
}
```

```text
true true
picked a card: true
true false
```

| Function | Returns | Notes |
|---|---|---|
| `int(lo, hi)` | `int` | `lo` included, `hi` excluded; `hi` must be greater than `lo` |
| `float()` | `float` | from 0.0 up to (not including) 1.0 |
| `chance(p)` | `bool` | `true` with probability `p` |
| `pick(items)` | `T?` | a random element, or `none` for an empty array |
| `shuffle(items)` | — | shuffles a `var` array in place |
| `seed(n)` | — | the same seed always gives the same numbers |

Without `seed`, the numbers differ on every run. The generator is fast but **not** suitable
for passwords or other security uses.

## Time

```kalinga
fn main() {
    let start = Time.now()
    Time.sleep(0.05)
    let elapsed = Time.now() - start
    println(elapsed >= 0.05, elapsed < 1.0)
    println(Time.millis() > 0)
}
```

```text
true true
true
```

| Function | Returns | Notes |
|---|---|---|
| `now()` | `float` | seconds since the program started; use differences for frame timing |
| `millis()` | `int` | milliseconds since 1970 (the wall clock) |
| `sleep(seconds)` | — | pauses the program |
| `wait(seconds)` | `Async<void>` | pauses an `async fn` without blocking other async jobs (chapter 21) |

A typical frame loop measures the time since the last frame and moves things by that much:

```kalinga
fn main() {
    var x = 0.0
    let speed = 100.0 // units per second
    var last = Time.now()
    for frame in 0..3 {
        Time.sleep(0.01)
        let now = Time.now()
        let dt = now - last
        last = now
        x += speed * dt
    }
    println(x > 2.0)
}
```

```text
true
```

## Files and folders: std/fs

`import "std/fs"` gives `File`, `Dir` and `Path`. Paths are relative to the folder the
program runs in, and `/` works on every system. Everything that can fail returns a
`Result` (see [Error Handling](16-error-handling.md)) whose error says what went wrong:

```kalinga
import "std/fs"

fn save(score: int) -> Result<void, str> {
    Dir.create("saves")?
    File.write(Path.join("saves", "best.txt"), "{score}\n")?
    return Ok()
}

fn load() -> Result<int, str> {
    let text = File.read("saves/best.txt")?
    if let n = text.trim().parse_int() {
        return Ok(n)
    }
    return Err("the save file is damaged")
}

fn main() {
    save(420).unwrap()
    println(load(), Dir.list("saves").unwrap())
    match File.read("missing.txt") {
        Ok(text) => println(text)
        Err(e) => println(e)
    }
}
```

```text
Ok(420) [best.txt]
cannot read `missing.txt`: not found
```

| Function | Returns | Notes |
|---|---|---|
| `File.read(path)` | `Result<str, str>` | the whole file as text |
| `File.read_lines(path)` | `Result<[str], str>` | the lines, without line endings |
| `File.write(path, text)` | `Result<void, str>` | creates the file or replaces its contents |
| `File.append(path, text)` | `Result<void, str>` | adds to the end, creating the file if needed |
| `File.exists(path)` | `bool` | an existing file (not a folder) |
| `File.delete(path)`, `File.copy(from, to)`, `File.rename(from, to)` | `Result<void, str>` | `rename` also moves folders |
| `Dir.list(path)` | `Result<[str], str>` | names inside, sorted |
| `Dir.create(path)` | `Result<void, str>` | also creates missing parent folders |
| `Dir.exists(path)` | `bool` | an existing folder |
| `Dir.delete(path)` | `Result<void, str>` | the folder must be empty |
| `Path.join(a, b)` | `str` | `"saves" + "/" + "best.txt"` |
| `Path.name(p)`, `Path.parent(p)`, `Path.extension(p)` | `str` | `"best.txt"`, `"saves"`, `"txt"` |

---

Previous: [14. Structs and Operators](14-structs-and-operators.md) · Next: [16. Error Handling](16-error-handling.md)
