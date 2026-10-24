# 12. Lambdas and Function Values

Functions are values in Kalinga: you can store them in variables, pass them to other
functions, return them, and keep them in fields and arrays. A **lambda** is a function
written inline, without a name.

## Function types

The type of a function is written `fn(parameter types) -> result type`:

| Type | Meaning |
|---|---|
| `fn(int) -> int` | takes an `int`, returns an `int` |
| `fn(str, str) -> bool` | takes two strings, returns a `bool` |
| `fn()` | takes nothing, returns nothing |

## Writing lambdas

There are two forms:

```kalinga
fn main() {
    // Short form: `=>` followed by one expression, which is the result.
    let double = fn(x: int) => x * 2

    // Block form: like a normal function body.
    let classify = fn(n: int) -> str {
        if n < 0 {
            return "negative"
        }
        return "non-negative"
    }

    println(double(21), classify(-5))
}
```

```text
42 negative
```

When Kalinga already knows what kind of function is expected, you can leave out the
parameter types:

```kalinga
fn apply_twice(f: fn(int) -> int, x: int) -> int {
    return f(f(x))
}

fn main() {
    println(apply_twice(fn(n) => n + 10, 1)) // `n` is an int: apply_twice says so
}
```

```text
21
```

## Named functions as values

Any top-level function can be used as a value by writing its name without calling it:

```kalinga
fn square(n: int) -> int {
    return n * n
}

fn main() {
    let ops: [fn(int) -> int] = [square, fn(n) => n + 1, fn(n) => -n]
    for op in ops {
        println(op(5))
    }
}
```

```text
25
6
-5
```

## Capturing variables

A lambda can use variables from where it was written. Usually it gets its own **copy** of
each one, made when the lambda is created:

```kalinga
fn make_adder(k: int) -> fn(int) -> int {
    return fn(x) => x + k
}

fn main() {
    let add5 = make_adder(5)
    let add10 = make_adder(10)
    println(add5(1), add10(1))

    var greeting = "hello"
    let greet = fn(name: str) => "{greeting}, {name}"
    greeting = "bye" // the lambda kept its own copy
    println(greet("Asha"))
}
```

```text
6 11
hello, Asha
```

### Lambdas that change a variable

When a lambda **changes** a `var` from outside it (assigns to it, or changes it in place
with `push`, `remove`, a struct's `mut fn`, ...), there is no copy: the lambda and the
code around it share that one variable, and every other lambda that uses it shares it too.

```kalinga
fn make_counter() -> fn() -> int {
    var count = 0
    return fn() -> int {
        count += 1
        return count
    }
}

fn main() {
    var total = 0
    let add = fn(x: int) {
        total += x
    }
    add(5)
    add(7)
    println(total)

    var log: [str] = []
    let note = fn(s: str) {
        log.push(s)
    }
    note("start")
    note("stop")
    println(log)

    let a = make_counter()
    let b = make_counter()
    println(a(), a(), b())
}
```

```text
12
[start, stop]
1 2 1
```

- Only variables declared with `var` can be changed this way; changing a `let` is an
  error, inside a lambda or not.
- Each call of `make_counter` makes a new `count`, so every counter has its own.
- A variable shared like this cannot be used by a lambda that runs on another thread
  (`Thread.spawn`, `par_map`); use a `Shared` there ([chapter 20](20-threads.md)).

Objects are always shared, since they are references: a lambda that changes `c.n` changes
the one object the outside code sees too.

```kalinga
class Counter {
    n: int
}

fn main() {
    let c = Counter()
    let bump = fn() {
        c.n += 1
    }
    bump()
    bump()
    println(c.n)
}
```

```text
2
```

Inside a method, `self` cannot be used in a lambda directly. Copy it to a variable first:
`let me = self`, then use `me` in the lambda.

## Functions in fields

A field can hold a function. Calling it looks like calling a method:

```kalinga
class Button {
    label: str
    on_click: fn(str) -> str = fn(who) => "nothing happens"

    init(label: str) {
        self.label = label
    }

    fn press(who: str) -> str {
        return self.on_click(who)
    }
}

fn main() {
    let ok = Button("OK")
    println(ok.press("Asha"))
    let label = ok.label
    ok.on_click = fn(who) => "{who} pressed {label}"
    println(ok.press("Asha"))
}
```

```text
nothing happens
Asha pressed OK
```

Fields of function type need a default value, like fields that hold objects.

## Array helpers that take functions

| Method | Returns | Description |
|---|---|---|
| `a.map(f)` | `[U]` | A new array with `f` applied to every element |
| `a.filter(f)` | `[T]` | The elements for which `f` returns `true` |
| `a.find(f)` | `T?` | The first element for which `f` returns `true`, or `none` |
| `a.sort()` | — | Sorts in place (`int`, `float`, `str`, `bool`; needs `var`) |
| `a.sort_by(f)` | — | Sorts in place; `f(a, b)` returns `true` when `a` comes first |

```kalinga
fn main() {
    let nums = [5, 3, 8, 1, 9, 2]
    let evens = nums.filter(fn(n) => n % 2 == 0)
    let labels = nums.map(fn(n) => "#{n}")
    println(evens, labels, nums.find(fn(n) => n > 6))

    var words = ["banana", "fig", "cherry", "kiwi"]
    words.sort()
    println(words)
    words.sort_by(fn(a, b) => len(a) < len(b))
    println(words)
}
```

```text
[8, 2] [#5, #3, #8, #1, #9, #2] 8
[banana, cherry, fig, kiwi]
[fig, kiwi, banana, cherry]
```

`sort_by` keeps elements that compare equal in their original order.

---

Previous: [11. Interfaces](11-interfaces.md) · Next: [13. Generics](13-generics.md)
