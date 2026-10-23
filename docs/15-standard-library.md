# 15. Standard Library: Math, Random, Time

Three built-in libraries are always available: `Math`, `Random` and `Time`. They are used
like static methods: `Math.sqrt(2.0)`.

## Math

```kalinga
fn main() {
    print(Math.sqrt(16), Math.pow(2, 10), Math.hypot(3, 4))
    print(Math.abs(-7), Math.abs(-2.5), Math.min(3, 9), Math.max(0.5, 1.5))
    print(Math.clamp(150, 0, 100), Math.lerp(0, 10, 0.25))
    print(Math.floor(2.7), Math.ceil(2.1), Math.round(2.5))
    print(Math.round(Math.sin(Math.PI / 2.0) * 100.0))
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
    Random.seed(42)                     // optional: makes the sequence repeatable
    let die = Random.int(1, 7)          // 1 to 6 (the upper bound is excluded)
    let unit = Random.float()           // 0.0 up to (not including) 1.0
    print(die >= 1 && die <= 6, unit >= 0.0 && unit < 1.0)

    var deck = [1, 2, 3, 4, 5]
    Random.shuffle(deck)
    if let card = Random.pick(deck) {
        print("picked a card:", card >= 1 && card <= 5)
    }
    print(Random.chance(1.0), Random.chance(0.0))
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
    print(elapsed >= 0.05, elapsed < 1.0)
    print(Time.millis() > 0)
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
    let speed = 100.0          // units per second
    var last = Time.now()
    for frame in 0..3 {
        Time.sleep(0.01)
        let now = Time.now()
        let dt = now - last
        last = now
        x += speed * dt
    }
    print(x > 2.0)
}
```

```text
true
```

---

Previous: [14. Structs and Operators](14-structs-and-operators.md) · Next: [16. Error Handling](16-error-handling.md)
