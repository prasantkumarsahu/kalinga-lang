# 5. Arrays and Strings

## Arrays

An array holds any number of values of one type. Its type is written `[T]`.

```kalinga
fn main() {
    let primes = [2, 3, 5, 7]
    let names: [str] = ["Ravi", "Mira"]
    var empty: [float] = []
    println(primes, names, empty)
}
```

```text
[2, 3, 5, 7] [Ravi, Mira] []
```

All elements must have the same type. An empty `[]` needs a type annotation.

### Reading and writing elements

Indexes start at `0`:

```kalinga
fn main() {
    var scores = [10, 20, 30]
    println(scores[0], scores[2])
    scores[1] = 25
    scores[2] += 5
    println(scores, len(scores))
}
```

```text
10 30
[10, 25, 35] 3
```

Changing elements requires a `var` array. Reading outside the array stops the program
with a clear error instead of reading random memory:

```text
runtime error: index out of bounds: the len is 3 but the index is 7
```

### Slices

`xs[a..b]` is a new array with the elements from index `a` up to, but not including, `b`.
Either end may be left out, and `a..=b` includes `b`:

```kalinga
fn main() {
    let xs = [10, 20, 30, 40, 50]
    println(xs[1..3], xs[..2], xs[3..], xs[1..=3])
}
```

```text
[20, 30] [10, 20] [40, 50] [20, 30, 40]
```

A slice past the end stops the program, like an index does.

### Array methods

| Method | Returns | Description |
|---|---|---|
| `a.push(x)` | — | Adds `x` at the end (needs `var`) |
| `a.pop()` | `T?` | Removes and returns the last element, or `none` if empty (needs `var`) |
| `a.len()` | `int` | Number of elements (same as `len(a)`) |
| `a.contains(x)` | `bool` | Whether `x` is in the array (arrays of `int`, `float`, `bool`, `str`) |
| `a.sort()` | — | Sorts in place (`int`, `float`, `str`, `bool`; needs `var`) |
| `a.map(f)`, `a.filter(f)`, `a.find(f)`, `a.sort_by(f)` | | Take a function; see [Lambdas](12-lambdas.md#array-helpers-that-take-functions) |

```kalinga
fn main() {
    var stack: [str] = []
    stack.push("a")
    stack.push("b")
    stack.push("c")
    println(stack.contains("b"), stack.len())

    if let top = stack.pop() {
        println("popped", top)
    }
    println(stack)
}
```

```text
true 3
popped c
[a, b]
```

### Nested arrays

```kalinga
fn main() {
    var grid = [[0, 0, 0], [0, 0, 0]]
    grid[1][2] = 5
    for row in grid {
        println(row)
    }
}
```

```text
[0, 0, 0]
[0, 0, 5]
```

### Arrays are values

Assigning an array, passing it to a function, or looping over it makes a **copy**.
The original never changes behind your back:

```kalinga
fn main() {
    var a = [1, 2, 3]
    var b = a
    b.push(4)
    println(a, b)
}
```

```text
[1, 2, 3] [1, 2, 3, 4]
```

An array of objects copies the *references*, so both arrays point at the same objects
(see [Classes](09-classes.md#objects-are-references)).

### Common patterns

```kalinga
fn sum(xs: [int]) -> int {
    var total = 0
    for x in xs {
        total += x
    }
    return total
}

fn max_of(xs: [int]) -> int? {
    if len(xs) == 0 {
        return none
    }
    var best = xs[0]
    for x in xs {
        if x > best {
            best = x
        }
    }
    return best
}

fn evens(xs: [int]) -> [int] {
    var out: [int] = []
    for x in xs {
        if x % 2 == 0 {
            out.push(x)
        }
    }
    return out
}

fn main() {
    let data = [4, 9, 2, 7, 6]
    println(sum(data), max_of(data), evens(data))
    let nothing: [int] = []
    println(max_of(nothing))
}
```

```text
28 9 [4, 2, 6]
none
```

## Strings

A `str` is UTF-8 text. Join strings with `+`; convert other values with `str()`:

```kalinga
fn main() {
    let lang = "Kalinga"
    let version = 1
    let line = lang + " v" + str(version)
    println(line, len(line))
}
```

```text
Kalinga v1 10
```

### String interpolation

Put any printable expression inside `{ }` in a string, and its value is inserted:

```kalinga
fn main() {
    let name = "Mira"
    let items = [3, 4]
    println("Hello {name}, you have {len(items)} items: {items}")
    println("next year you will be {30 + 1}")
    println("use {{ and }} for literal braces")
}
```

```text
Hello Mira, you have 2 items: [3, 4]
next year you will be 31
use { and } for literal braces
```

Interpolation works with numbers, booleans, strings, arrays, maps, optionals and enums.
For objects, insert a field or a method result: `"{user.name}"`.

### String methods

| Method | Returns | Description |
|---|---|---|
| `s.len()` | `int` | Number of characters (same as `len(s)`) |
| `s.upper()` | `str` | Uppercase copy |
| `s.lower()` | `str` | Lowercase copy |
| `s.trim()` | `str` | Copy without leading/trailing whitespace |
| `s.contains(t)` | `bool` | Whether `t` occurs in `s` |
| `s.starts_with(t)`, `s.ends_with(t)` | `bool` | Whether `s` begins / ends with `t` |
| `s.find(t)` | `int?` | Index of the first `t`, or `none` |
| `s.replace(a, b)` | `str` | Copy with every `a` replaced by `b` |
| `s.split(sep)` | `[str]` | The parts between each `sep` |
| `s.chars()` | `[str]` | The characters |
| `s.code_at(i)` | `int` | Code point of the character at `i` |
| `s.parse_int()`, `s.parse_float()` | `int?`, `float?` | The number in `s`, or `none` |
| `words.join(sep)` | `str` | An array of strings joined with `sep` |

```kalinga
fn main() {
    let raw = "  Hello World  "
    let clean = raw.trim()
    println(clean.upper(), clean.lower(), clean.contains("World"))
    let parts = "a,b,c".split(",")
    println(parts, parts.join(" + "), "3 cats".replace("cats", "dogs"))
    if let n = "42".parse_int() {
        println(n + 1)
    }
}
```

```text
HELLO WORLD hello world true
[a, b, c] a + b + c 3 dogs
43
```

### Characters

A character is a one-character `str`. `s[i]` is the character at index `i` (counting
characters, not bytes), and slices work as for arrays:

```kalinga
fn reverse(s: str) -> str {
    var out = ""
    for ch in s {
        out = ch + out
    }
    return out
}

fn main() {
    let word = "kalinga"
    println(word[0], word[1..4], reverse("stressed"))
    println(word.code_at(0), chr(75))
}
```

```text
k ali desserts
107 K
```

A string cannot be changed in place (`word[0] = "K"` is an error); build a new one with
`+`, a slice or `replace`. Strings are values, like arrays: `+=` on one variable never
changes another.

---

Previous: [4. Functions](04-functions.md) · Next: [6. Maps](06-maps.md)
