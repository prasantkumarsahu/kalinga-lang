# 5. Arrays and Strings

## Arrays

An array holds any number of values of one type. Its type is written `[T]`.

```kalinga
fn main() {
    let primes = [2, 3, 5, 7]
    let names: [str] = ["Ravi", "Mira"]
    var empty: [float] = []
    print(primes, names, empty)
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
    print(scores[0], scores[2])
    scores[1] = 25
    scores[2] += 5
    print(scores, len(scores))
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

### Array methods

| Method | Returns | Description |
|---|---|---|
| `a.push(x)` | — | Adds `x` at the end (needs `var`) |
| `a.pop()` | `T?` | Removes and returns the last element, or `none` if empty (needs `var`) |
| `a.len()` | `int` | Number of elements (same as `len(a)`) |
| `a.contains(x)` | `bool` | Whether `x` is in the array (arrays of `int`, `float`, `bool`, `str`) |

```kalinga
fn main() {
    var stack: [str] = []
    stack.push("a")
    stack.push("b")
    stack.push("c")
    print(stack.contains("b"), stack.len())

    if let top = stack.pop() {
        print("popped", top)
    }
    print(stack)
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
        print(row)
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
    print(a, b)
}
```

```text
[1, 2, 3] [1, 2, 3, 4]
```

An array of objects copies the *references*, so both arrays point at the same objects
(see [Classes](07-classes.md#objects-are-references)).

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
    print(sum(data), max_of(data), evens(data))
    let nothing: [int] = []
    print(max_of(nothing))
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
    print(line, len(line))
}
```

```text
Kalinga v1 10
```

### String methods

| Method | Returns | Description |
|---|---|---|
| `s.len()` | `int` | Number of characters (same as `len(s)`) |
| `s.upper()` | `str` | Uppercase copy |
| `s.lower()` | `str` | Lowercase copy |
| `s.trim()` | `str` | Copy without leading/trailing whitespace |
| `s.contains(t)` | `bool` | Whether `t` occurs in `s` |

```kalinga
fn main() {
    let raw = "  Hello World  "
    let clean = raw.trim()
    print(clean.upper(), clean.lower(), clean.contains("World"))
}
```

```text
HELLO WORLD hello world true
```

### Characters

Strings cannot be indexed with `s[i]`. Loop over them instead; each character is a `str`:

```kalinga
fn reverse(s: str) -> str {
    var out = ""
    for ch in s {
        out = ch + out
    }
    return out
}

fn main() {
    print(reverse("stressed"))
}
```

```text
desserts
```

Strings are values, like arrays: `+=` on one variable never changes another.

---

Previous: [4. Functions](04-functions.md) · Next: [6. Optionals](06-optionals.md)
