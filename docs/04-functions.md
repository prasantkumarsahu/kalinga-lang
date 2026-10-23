# 4. Functions

## Declaring functions

```kalinga
fn add(a: int, b: int) -> int {
    return a + b
}

fn say_hi(name: str) {
    println("Hi,", name)
}

fn main() {
    println(add(2, 3))
    say_hi("Asha")
}
```

```text
5
Hi, Asha
```

- Every parameter needs a type: `name: type`.
- `-> type` gives the return type. Leave it out when the function returns nothing.
- Functions can be declared in any order; `main` can call a function defined below it.

## Returning values

A function with a return type must return a value on **every** path. The compiler checks
this:

```kalinga error
fn sign(n: int) -> str {
    if n >= 0 {
        return "positive"
    }
}

fn main() {}
```

```text
main.kl:1:1: error: `sign` must return a `str` on every path
```

Fix it by adding an `else` branch or a final `return`.

A function without a return type can use a bare `return` to stop early:

```kalinga
fn check(n: int) {
    if n < 0 {
        println("negative, stopping")
        return
    }
    println("ok:", n)
}

fn main() {
    check(5)
    check(-1)
}
```

```text
ok: 5
negative, stopping
```

## Parameters are values

Parameters cannot be reassigned inside the function (they behave like `let`). Copy into a
`var` if you need to change one:

```kalinga
fn countdown(from: int) {
    var n = from
    while n > 0 {
        println(n)
        n -= 1
    }
}

fn main() {
    countdown(3)
}
```

How values are passed depends on the type:

| Type | Passed as |
|---|---|
| `int`, `float`, `bool` | a copy |
| `str`, arrays, optionals | a copy — changes inside the function do not affect the caller |
| objects (classes, interfaces) | a reference — the function sees and changes the same object |

```kalinga
class Holder {
    value: int
}

fn fill(items: [int], b: Holder) {
    var copy = items
    copy.push(99)       // only the local copy grows
    b.value = 99        // the caller's object changes
}

fn main() {
    let nums = [1, 2]
    let b = Holder()
    fill(nums, b)
    println(nums, b.value)
}
```

```text
[1, 2] 99
```

## Recursion

Functions can call themselves. Programs run with a large stack (256 MB), so deep
recursion is fine:

```kalinga
fn fib(n: int) -> int {
    if n < 2 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn sum_to(n: int) -> int {
    if n == 0 {
        return 0
    }
    return n + sum_to(n - 1)
}

fn main() {
    println(fib(30))
    println(sum_to(100000))
}
```

```text
832040
5000050000
```

## Built-in functions

| Function | Returns | Description |
|---|---|---|
| `print(a, b, ...)` | — | Prints the values separated by spaces, then a newline. `print()` prints an empty line. |
| `len(x)` | `int` | Number of elements in an array, or characters in a string |
| `str(x)` | `str` | Text form of an `int`, `float`, `bool`, `str`, array or optional |
| `int(x)` | `int` | From `float` (drops the fraction), `bool` (`1`/`0`) or `str` (parses; error if not a number) |
| `float(x)` | `float` | From `int` or `str` |
| `input(prompt)` | `str` | Prints the optional prompt and reads one line from the keyboard |

How values are printed:

```kalinga
fn main() {
    let maybe: int? = none
    println(42, 2.0, 0.25, true, "text", [1, 2, 3], maybe)
}
```

```text
42 2.0 0.25 true text [1, 2, 3] none
```

Objects cannot be printed directly; print their fields or a method result instead.

Reading input:

```kalinga
fn main() {
    let name = input("Your name: ")
    let age = int(input("Your age: "))
    println("Hello", name, "- next year you will be", age + 1)
}
```

## Things functions cannot do (yet)

- No default parameter values and no overloading (two functions with the same name).
- Functions themselves can be stored, passed and returned — see [Lambdas](12-lambdas.md) —
  and written once for many types — see [Generics](13-generics.md).
- Built-in names (`print`, `len`, `str`, `int`, `float`, `input`) cannot be redefined.
- There are no global variables; pass values as parameters or keep them in objects.

---

Previous: [3. Control Flow](03-control-flow.md) · Next: [5. Arrays and Strings](05-arrays-and-strings.md)
