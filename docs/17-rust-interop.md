# 17. Rust Interop

Kalinga compiles to Rust, so it can use Rust code directly: your own Rust files, Rust's
standard library, and any library (crate) from [crates.io](https://crates.io). This is how
Kalinga programs get graphics, audio, networking, machine learning and anything else the
Rust ecosystem provides.

There are four pieces:

| Declaration | Meaning |
|---|---|
| `use rust "native/stats.rs"` | compile this Rust file into the program |
| `use crate "rand" = "0.8"` | use a crates.io library (the build switches to `cargo`) |
| `extern fn name(params) -> T` | a Rust function Kalinga can call |
| `extern type Name` | a Rust type Kalinga can hold and pass around |

## Calling a Rust function

`native/stats.rs`:

```rust
pub fn mean(xs: Vec<f64>) -> f64 {
    if xs.is_empty() { 0.0 } else { xs.iter().sum::<f64>() / xs.len() as f64 }
}
```

`main.kl`:

```kalinga
use rust "native/stats.rs"

extern fn mean(xs: [float]) -> float

fn main() {
    print(mean([2.0, 4.0, 9.0]))
}
```

```text
5.0
```

- The path in `use rust` is relative to the `.kl` file, like `import`.
- Every `pub` item of the Rust file is available; `extern fn` tells Kalinga its signature.
- `kalinga check` trusts the `extern` declaration. If it does not match the Rust function,
  the Rust compiler reports it when you `run` or `build`:

```text
error: the Rust compiler rejected the program.
This program uses Rust code (`use rust`, `use crate` or `extern fn`): check that each
`extern` declaration matches the Rust code it names.
```

## How types map

| Kalinga | Rust |
|---|---|
| `int` | `i64` |
| `float` | `f64` |
| `bool` | `bool` |
| `str` | `String` |
| `[T]` | `Vec<T>` |
| `{K: V}` | `std::collections::BTreeMap<K, V>` |
| `T?` | `Option<T>` |
| a Kalinga `struct` / `enum` | the Rust type of the same name: `crate::Vec2` |
| `extern type T` | your Rust type `T` (it must implement `Clone`) |
| `fn(A) -> B` | `std::rc::Rc<dyn Fn(A) -> B>` |

Values are passed **by value**: Rust receives its own copy and returns a new value. This
keeps Kalinga's safety guarantees intact: Rust code cannot change a Kalinga variable behind
its back.

## Binding a Rust path directly

For a function that already has the right shape, skip the Rust file and name it:

```kalinga
extern fn cube_root(x: float) -> float = "f64::cbrt"
extern fn min_int(a: int, b: int) -> int = "std::cmp::min"

fn main() {
    print(cube_root(27.0), min_int(3, -8))
}
```

```text
3.0 -8
```

## Opaque Rust types

`extern type` declares a Rust type that Kalinga can store, copy and pass to Rust functions,
but not look inside. This is how you wrap things like textures, sounds, network connections
or tensors:

```rust
// native/matrix.rs
#[derive(Clone)]
pub struct Matrix { rows: usize, cols: usize, data: Vec<f64> }

pub fn matrix_identity(n: i64) -> Matrix {
    let n = n as usize;
    let mut data = vec![0.0; n * n];
    for i in 0..n { data[i * n + i] = 1.0; }
    Matrix { rows: n, cols: n, data }
}

pub fn matrix_trace(m: Matrix) -> f64 {
    (0..m.rows.min(m.cols)).map(|i| m.data[i * m.cols + i]).sum()
}
```

```kalinga
use rust "native/matrix.rs"

extern type Matrix
extern fn matrix_identity(n: int) -> Matrix
extern fn matrix_trace(m: Matrix) -> float

fn main() {
    let m = matrix_identity(4)
    print(matrix_trace(m))
}
```

```text
4.0
```

## Using crates.io libraries

`use crate` adds a dependency from crates.io. A small Rust "glue" file then adapts the
library's API to Kalinga's types:

```rust
// native/rng.rs
pub fn roll(sides: i64) -> i64 {
    fastrand::i64(1..=sides)
}
```

```kalinga
use crate "fastrand" = "2"
use rust "native/rng.rs"

extern fn roll(sides: int) -> int

fn main() {
    print(roll(6) <= 6)
}
```

- With `use crate`, `kalinga run` and `build` use `cargo` instead of plain `rustc`.
- The first build that uses a crate downloads and compiles it (needs internet, takes a
  while); after that, builds are fast again. Compiled crates are shared between programs.
- Version strings follow Cargo's rules: `"0.8"` means any compatible 0.8.x release.

## A pattern for libraries

For anything bigger than a few functions, keep the Rust side in one file per library and
wrap it in a small Kalinga module:

```text
my-game/
├── main.kl                 import "engine/window.kl"
└── engine/
    ├── window.kl           use crate "macroquad" = "0.4"
    │                       use rust "window_glue.rs"
    │                       extern type Texture
    │                       extern fn draw_texture(t: Texture, x: float, y: float)
    │                       ... plus friendly Kalinga classes and structs on top
    └── window_glue.rs      pub fn draw_texture(t: Texture, x: f64, y: f64) { ... }
```

The rest of the game is written in plain Kalinga against `window.kl`.

## Limits in this version

- Rust code receives copies; it cannot keep references to Kalinga objects or call back
  into Kalinga except through function values passed to it.
- Only crates.io version requirements are supported (no features, git or path
  dependencies yet).
- `extern fn`s cannot be generic.

---

Previous: [16. Modules](16-modules.md) · Next: [18. Safety and Errors](18-safety-and-errors.md)
