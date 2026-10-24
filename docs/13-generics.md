# 13. Generics

**Generics** let you write a function or class once and use it with many types. The
placeholder types are called **type parameters** and are written in angle brackets:
`<T>`.

## Generic functions

```kalinga
fn first<T>(items: [T]) -> T? {
    if len(items) == 0 {
        return none
    }
    return items[0]
}

fn repeat<T>(value: T, times: int) -> [T] {
    var out: [T] = []
    for i in 0..times {
        out.push(value)
    }
    return out
}

fn main() {
    println(first([10, 20]), first(["a", "b"]))
    println(repeat("ho", 3), repeat(true, 2))
}
```

```text
10 a
[ho, ho, ho] [true, true]
```

Kalinga works out what `T` is from the arguments: in `first([10, 20])`, `T` is `int`.

Several type parameters are separated by commas:

```kalinga
fn transform<T, U>(items: [T], f: fn(T) -> U) -> [U] {
    var out: [U] = []
    for x in items {
        out.push(f(x))
    }
    return out
}

fn main() {
    println(transform([1, 2, 3], fn(n) => n * n))
    println(transform(["a", "bcd"], fn(s) => len(s)))
}
```

```text
[1, 4, 9]
[1, 3]
```

## Generic classes

```kalinga
class Stack<T> {
    items: [T]

    fn push(x: T) {
        self.items.push(x)
    }

    fn pop() -> T? {
        return self.items.pop()
    }

    fn size() -> int {
        return len(self.items)
    }
}

fn main() {
    let numbers: Stack<int> = Stack()
    numbers.push(1)
    numbers.push(2)
    println(numbers.size(), numbers.pop(), numbers.pop(), numbers.pop())

    let words = Stack<str>()
    words.push("hi")
    println(words.pop())
}
```

```text
2 2 1 none
hi
```

The type arguments come from (in order of preference):

1. Explicit arguments: `Stack<str>()`.
2. The constructor's arguments: `Pair(1, "a")` is a `Pair<int, str>`.
3. The expected type: `let s: Stack<int> = Stack()`.

If none of these says what `T` is, the compiler asks you to write it:

```text
error: cannot tell what `T` is for `Stack`; write it, e.g. `Stack<int>(...)`
```

A class with two type parameters:

```kalinga
class Pair<A, B> {
    first: A
    second: B

    init(first: A, second: B) {
        self.first = first
        self.second = second
    }

    fn swap() -> Pair<B, A> {
        return Pair(self.second, self.first)
    }
}

fn main() {
    let p = Pair("x", 5)
    let q = p.swap()
    println(p.first, p.second, "->", q.first, q.second)
}
```

```text
x 5 -> 5 x
```

## What you can do with a `T`

Inside generic code, Kalinga does not know what `T` is, so a plain `T` value can only be
**stored, copied, passed and returned** — including in arrays, maps (as values) and
optionals. Printing, comparing or calling methods on it needs a **bound** (next section):

```kalinga error
fn biggest<T>(a: T, b: T) -> T {
    if a > b {
        return a
    }
    return b
}

fn main() {}
```

```text
main.kl:2:10: error: cannot compare `T` with `T` using `>`; add a bound: `<T: Ord>`
```

Arithmetic on a `T` is not possible; pass the behaviour in as a function instead (like
`transform` above takes `f`).

A field whose type is a bare `T` starts out empty. Reading it before it is set stops the
program with `field `value` was read before it was set` — so set such fields in `init`.

## Bounds

A bound after a type parameter says what every type used for it must allow, and so what
the generic code may do with it:

| Bound | Allows | Types that have it |
|---|---|---|
| `Show` | printing: `println(x)`, `str(x)`, `"{x}"` | numbers, `bool`, `str`, and arrays, maps, optionals, plain structs and enums of them |
| `Eq` | `==`, `!=`, `contains`, `assert_eq` (with `Show`) | the same |
| `Ord` | `<`, `<=`, `>`, `>=`, `sort` (and `==`) | `int`, `float`, `str`, `bool` |
| an interface | calling its methods, passing the value where the interface is expected | classes that implement it, and the interface itself |

```kalinga
interface Shape {
    fn area() -> float
}

class Square : Shape {
    side: float

    init(side: float) {
        self.side = side
    }

    override fn area() -> float {
        return self.side * self.side
    }
}

fn largest<T: Ord>(xs: [T]) -> T? {
    var best: T? = none
    for x in xs {
        if let b = best {
            if b < x {
                best = x
            }
        } else {
            best = x
        }
    }
    return best
}

fn total_area<T: Shape>(shapes: [T]) -> float {
    var sum = 0.0
    for s in shapes {
        sum += s.area()
    }
    return sum
}

fn report<T: Show + Eq>(a: T, b: T) {
    println("{a} == {b}: {a == b}")
}

fn main() {
    println(largest([3, 9, 4]))
    println(total_area([Square(2.0), Square(3.0)]))
    report("x", "y")
}
```

```text
9
13.0
x == y: false
```

- Join several bounds with `+`: `<T: Show + Eq>`. Each type parameter has its own:
  `struct Pair<A: Show, B: Ord>`.
- Bounds work on generic functions, classes and structs.
- A type that does not meet a bound is an error where it is used:
  `` `Player` does not satisfy `T: Show` for `show_all` ``.
- Inside the generic code, a missing bound is reported with the bound to add.

## Limits in this version

- Generic classes cannot extend other classes, be extended, or implement interfaces.
- Interfaces and enums cannot be generic.

---

Previous: [12. Lambdas and Function Values](12-lambdas.md) · Next: [14. Structs and Operators](14-structs-and-operators.md)
