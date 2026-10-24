# 14. Structs and Operators

A **struct** is a small **value type**: like a class, it has fields and methods, but it is
copied when assigned or passed, and it is stored directly inside arrays and other values
instead of on its own. That makes structs the right tool for vectors, colours, rectangles,
particles and other small pieces of data that games and numeric code use by the million.

## Declaring and creating structs

```kalinga
struct Vec2 {
    x: float
    y: float
}

fn main() {
    let a = Vec2(3.0, 4.0) // the fields, in order
    var b = a              // a copy
    b.x = 10.0
    println(a, b, a == Vec2(3.0, 4.0))
}
```

```text
Vec2(x: 3.0, y: 4.0) Vec2(x: 10.0, y: 4.0) true
```

- `Name(...)` takes the fields in the order they are declared. Fields with a default may be
  left off at the end: with `struct Light { color: str, on: bool = true }`, both
  `Light("red")` and `Light("red", false)` work.
- Fields can be named, as [arguments](04-functions.md#named-arguments) are: with
  `struct Rect { x: float = 0, y: float = 0, w: float, h: float }`, `Rect(w: 10, h: 5)`
  leaves `x` and `y` at their defaults.
- Structs have no `init`. For other ways to build one, write a `static fn` (below).
- A struct whose fields are all printable can be printed and compared with `==`.
- Changing a field needs a `var`: `let a = ...; a.x = 1.0` is an error, because `a` is a
  value, not a reference.

### Struct or class?

| | `struct` | `class` |
|---|---|---|
| Assigning or passing | copies the value | shares the same object |
| Stored in an array | inline, next to each other | as references to separate objects |
| Inheritance, interfaces | no | yes |
| Best for | small data: vectors, colours, particles, settings | things with identity: players, enemies, scenes |

A million `struct` points use about a third of the memory of a million `class` points and
are built about four times faster — the same as hand-written Rust.

## Methods

Methods read the struct through `self`. A method that **changes** the struct must be
declared `mut fn`, and can only be called on something that can change (a `var`, an array
element, a field of an object):

```kalinga
struct Counter {
    n: int

    fn doubled() -> int {
        return self.n * 2
    }

    mut fn add(k: int) {
        self.n += k
    }

    static fn starting_at(n: int) -> Counter {
        return Counter(n)
    }
}

fn main() {
    var c = Counter.starting_at(5)
    c.add(3)
    println(c.n, c.doubled())

    var many = [Counter(1), Counter(2)]
    many[1].add(40)
    println(many[1].n)
}
```

```text
8 16
42
```

Forgetting `mut` is caught by the compiler:

```kalinga error
struct Counter {
    n: int

    fn add(k: int) {
        self.n += k
    }
}

fn main() {}
```

```text
main.kl:5:13: error: this method changes `self`; declare it `mut fn`
```

`static fn`s work like in classes (`Counter.starting_at(5)`), and `private` hides a field
or method outside the struct.

## Operators

A struct can define what `+`, `-`, `*`, `/` and `%` mean for it with `op`:

```kalinga
struct Vec2 {
    x: float
    y: float

    op +(o: Vec2) -> Vec2 {
        return Vec2(self.x + o.x, self.y + o.y)
    }

    op -(o: Vec2) -> Vec2 {
        return Vec2(self.x - o.x, self.y - o.y)
    }

    op *(k: float) -> Vec2 {
        return Vec2(self.x * k, self.y * k)
    }

    op *(o: Vec2) -> float { // a second `*`: dot product
        return self.x * o.x + self.y * o.y
    }

    op -() -> Vec2 { // unary minus
        return Vec2(-self.x, -self.y)
    }
}

fn main() {
    var pos = Vec2(0.0, 10.0)
    let vel = Vec2(2.0, -1.0)
    let dt = 0.5
    pos += vel * dt // compound assignment uses the operator too
    println(pos, -pos, pos * Vec2(1.0, 0.0))
}
```

```text
Vec2(x: 1.0, y: 9.5) Vec2(x: -1.0, y: -9.5) 1.0
```

- The struct is always on the **left**: `v * 2.0` works; `2.0 * v` does not.
- An operator can be defined several times with different right-hand types (`* float`
  and `* Vec2` above); the right one is chosen from the type on the right.
- `op -()` with no parameter is unary minus.
- `+=`, `-=`, `*=`, `/=` and `%=` work when the operator returns the struct's own type.

## A game-style example

Structs, operators and the [standard library](15-standard-library.md) together:

```kalinga
struct Vec2 {
    x: float
    y: float

    op +(o: Vec2) -> Vec2 {
        return Vec2(self.x + o.x, self.y + o.y)
    }

    op *(k: float) -> Vec2 {
        return Vec2(self.x * k, self.y * k)
    }
}

struct Ball {
    pos: Vec2
    vel: Vec2
}

fn main() {
    let gravity = Vec2(0.0, -9.8)
    var balls = [Ball(Vec2(0.0, 10.0), Vec2(1.0, 0.0)), Ball(Vec2(5.0, 2.0), Vec2(0.0, 6.0))]
    let dt = 0.1
    for step in 0..10 {
        for i in 0..len(balls) {
            balls[i].vel += gravity * dt
            balls[i].pos += balls[i].vel * dt
        }
    }
    for b in balls {
        println("ball at ({Math.round(b.pos.x)}, {Math.round(b.pos.y)})")
    }
}
```

```text
ball at (1.0, 5.0)
ball at (5.0, 3.0)
```

## Rules at a glance

- Structs cannot extend classes or implement interfaces, and cannot be generic (yet).
- A struct cannot contain itself directly (put it in an array, or use a class).
- Fields of struct type inside a **class** need a default value, like other non-optional
  fields.

---

Previous: [13. Generics](13-generics.md) · Next: [15. Standard Library: Math, Random, Time](15-standard-library.md)
