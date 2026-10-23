# 2. Basics

## Comments

```kalinga
// A line comment runs to the end of the line.

/* A block comment
   can span several lines. */

fn main() {
    println("comments are ignored") // trailing comments are fine too
}
```

## Doc comments

A comment that starts with `///`, written directly above a function, class, struct, field,
method, interface or enum, is that declaration's **documentation**. VS Code shows it when
you hover over the name, in the completion list and in parameter hints. It is Markdown, so
`` `code` ``, **bold**, lists and indented code examples work:

```kalinga
/// A 2D point.
struct Point {
    /// Distance from the left edge, in pixels.
    x: int
    y: int
}

/// Moves `p` by `dx` pixels to the right.
///
///     shift(Point(1, 2), 3) == Point(4, 2)
fn shift(p: Point, dx: int) -> Point {
    // An ordinary comment: a note for whoever reads this code.
    return Point(p.x + dx, p.y)
}

fn main() {
    println(shift(Point(1, 2), 3))
}
```

```text
Point(x: 4, y: 2)
```

Ordinary `//` comments are never documentation, even above a declaration. Use them for
notes; use `///` for what someone *calling* the code needs to know.

## Statements and lines

A statement ends at the end of the line. Semicolons are allowed but never needed:

```kalinga
fn main() {
    let a = 1
    let b = 2; let c = 3
    println(a + b + c)
}
```

A long expression can continue on the next line when the line ends with an operator, a
comma, or inside `(` `)` and `[` `]`, or when the next line starts with `.` (a method chain):

```kalinga
fn main() {
    let total = 1 +
        2 +
        3
    let words = [
        "one",
        "two",
    ]
    println(total, words)
}
```

```text
6 [one, two]
```

## Variables

| Keyword | Meaning |
|---|---|
| `let` | Immutable: the value can never be reassigned |
| `var` | Mutable: the value can be reassigned |

```kalinga
fn main() {
    let pi = 3.14159
    var count = 0
    count = count + 1
    count += 1
    println(pi, count)
}
```

```text
3.14159 2
```

Trying to change a `let` variable is a compile error:

```kalinga error
fn main() {
    let x = 1
    x = 2
}
```

```text
main.kl:3:5: error: cannot change `x`: it was declared with `let` (use `var` for a mutable variable)
3 |     x = 2
  |     ^
```

Prefer `let`. Use `var` only when a value really needs to change.

### Type annotations

Kalinga infers a variable's type from its value. You can also write it explicitly after a
colon:

```kalinga
fn main() {
    let a = 10            // int, inferred
    let b: float = 2.5    // float, written out
    var names: [str] = [] // an empty array needs a type
    println(a, b, names)
}
```

Annotations are required when the value alone does not say the type: an empty array `[]`
or `none` (see [Optionals](07-optionals.md)).

### Scope

A variable lives until the end of the `{ }` block it was declared in. An inner block may
declare a new variable with the same name (shadowing); the same block may not:

```kalinga
fn main() {
    let x = 1
    if true {
        let x = "inner" // a different variable that hides the outer one
        println(x)
    }
    println(x)
}
```

```text
inner
1
```

### Names

- Names contain letters, digits and `_`, and do not start with a digit.
- Names starting with `__` are reserved for the compiler.
- Class and interface names must start with an uppercase letter (`Animal`, `Show`).
- By convention, variables, functions and methods use `snake_case`.

## Constants

`const` at the top level of a file names a value the whole program can use. Its type is
written out, like a field's:

```kalinga
const MAX_LIVES: int = 3
const GRAVITY: float = 9.81
const TITLE: str = "Space Rocks"

fn main() {
    println(TITLE, MAX_LIVES, GRAVITY * 2)
}
```

```text
Space Rocks 3 19.62
```

A constant cannot be changed, and constants are written in `UPPER_CASE` by convention.
Classes and structs can have their own constants, and variables shared by the whole type
(see [Classes](09-classes.md#constants-and-static-variables)).

## Types

| Type | Meaning | Examples |
|---|---|---|
| `int` | 64-bit signed integer | `0`, `42`, `-7`, `1_000_000` |
| `float` | 64-bit floating point | `3.14`, `0.5`, `2.0` |
| `bool` | true or false | `true`, `false` |
| `str` | text (UTF-8) | `"hello"`, `"line\n"` |
| `[T]` | array of `T` | `[1, 2, 3]`, `["a", "b"]` |
| `T?` | optional `T`: a `T` or `none` | `none`, `5` (as `int?`) |
| `ClassName` | an object | `Dog("Rex")` |
| `InterfaceName` | any object implementing it | — |
| `void` | "no value", only as a return type | — |

Notes:

- A float literal needs digits on both sides of the dot: `2.0`, not `2.`.
- Underscores in numbers are ignored: `1_000` is `1000`.
- String escapes: `\n` newline, `\t` tab, `\r`, `\0`, `\\` backslash, `\"` quote.
- `{expression}` inside a string inserts a value: `"total: {n + 1}"`. Write `{{` and `}}`
  for literal braces. See [Arrays and Strings](05-arrays-and-strings.md#string-interpolation).

### No implicit conversions

Kalinga never converts between types on its own. Use `int()`, `float()` and `str()`:

```kalinga
fn main() {
    let n = 7
    let f = 2.5
    println(float(n) * f)     // int → float
    println(int(f))           // float → int (drops the fraction)
    println("n is " + str(n)) // int → str
    println(int("42") + 1)    // str → int
}
```

```text
17.5
2
n is 7
43
```

## Operators

### Arithmetic

| Operator | Meaning | Works on |
|---|---|---|
| `+` | add / join text | `int`, `float`, `str` |
| `-` | subtract | `int`, `float` |
| `*` | multiply | `int`, `float` |
| `/` | divide (integers round toward zero) | `int`, `float` |
| `%` | remainder | `int`, `float` |
| `-x` | negate | `int`, `float` |

Both sides must have the same type: `1 + 2.0` is an error, write `float(1) + 2.0`.

```kalinga
fn main() {
    println(7 / 2, 7 % 2, 7.0 / 2.0)
    println("Kal" + "inga")
}
```

```text
3 1 3.5
Kalinga
```

### Comparison and logic

| Operator | Meaning |
|---|---|
| `==` `!=` | equal, not equal (`int`, `float`, `bool`, `str`, and arrays/optionals of those) |
| `<` `<=` `>` `>=` | ordering (`int`, `float`, `str`) |
| `&&` | and (stops early if the left side is `false`) |
| `\|\|` | or (stops early if the left side is `true`) |
| `!x` | not |

```kalinga
fn main() {
    let age = 20
    let member = true
    println(age >= 18 && member)
    println("apple" < "banana", [1, 2] == [1, 2])
}
```

```text
true
true true
```

### Assignment

`=`, `+=`, `-=`, `*=`, `/=`, `%=`. `+=` also appends to a `str`:

```kalinga
fn main() {
    var s = "ab"
    s += "cd"
    var n = 10
    n *= 3
    n -= 5
    println(s, n)
}
```

```text
abcd 25
```

For operator precedence, see the [Language Reference](26-reference.md#operator-precedence).

---

Previous: [1. Getting Started](01-getting-started.md) · Next: [3. Control Flow](03-control-flow.md)
