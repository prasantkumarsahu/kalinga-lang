# 7. Optionals

Kalinga has **no null**. A normal `int`, `str` or object variable always holds a real
value. When "there might be nothing" is a real possibility, you say so in the type with
`?` — and the compiler makes sure you handle the empty case.

## Declaring optionals

`T?` means "a `T`, or `none`":

```kalinga
fn main() {
    var nickname: str? = none
    println(nickname)
    nickname = "Kali" // a plain str is wrapped automatically
    println(nickname)
}
```

```text
none
Kali
```

`none` on its own has no type, so a variable holding it needs an annotation:
`let x = none` is an error, `let x: int? = none` is fine.

## Using the value: if let

You cannot use an `int?` where an `int` is needed. Unwrap it with `if let`:

```kalinga
fn find_index(items: [str], target: str) -> int? {
    for i in 0..len(items) {
        if items[i] == target {
            return i
        }
    }
    return none
}

fn main() {
    let colors = ["red", "green", "blue"]

    if let i = find_index(colors, "green") {
        println("green is at", i)
    }

    if let i = find_index(colors, "pink") {
        println("pink is at", i)
    } else {
        println("no pink")
    }
}
```

```text
green is at 1
no pink
```

Inside the first block, `i` is a plain `int`. It only exists inside that block.

`if let` also works as a value, with an `else` (like [`if`](03-control-flow.md)):

```kalinga
fn main() {
    let ages: {str: int} = {"Asha": 30}
    let a = if let n = ages.get("Asha") { n + 1 } else { 0 }
    let r = if let n = ages.get("Ravi") { n + 1 } else { 0 }
    println(a, r)
}
```

```text
31 0
```

`while let` works the same way and repeats until the value is `none` (see
[Control flow](03-control-flow.md#while-let)).

## Comparing with none

`== none` and `!= none` test whether a value is present:

```kalinga
fn main() {
    var cache: int? = none
    if cache == none {
        cache = 42
    }
    println(cache != none, cache)
}
```

```text
true 42
```

## Why this matters

Forgetting the empty case is a compile error, not a crash at 2 AM:

```kalinga error
fn first(xs: [int]) -> int? {
    if len(xs) == 0 {
        return none
    }
    return xs[0]
}

fn main() {
    let n = first([1, 2])
    println(n + 1)
}
```

```text
main.kl:10:15: error: cannot apply `+` to `int?` and `int`
```

## Where optionals appear

- Function results that may be missing: `fn find(...) -> int?`
- `array.pop()` returns `T?` because the array may be empty.
- Object fields that may be unset: `next: Node?` (see [Classes](09-classes.md#fields-that-hold-objects)).

Optionals of objects work the same way:

```kalinga
class User {
    name: str
    init(name: str) {
        self.name = name
    }
}

fn lookup(id: int) -> User? {
    if id == 1 {
        return User("Asha")
    }
    return none
}

fn main() {
    for id in 1..3 {
        if let u = lookup(id) {
            println(id, "->", u.name)
        } else {
            println(id, "-> not found")
        }
    }
}
```

```text
1 -> Asha
2 -> not found
```

---

Previous: [6. Maps](06-maps.md) · Next: [8. Enums and match](08-enums-and-match.md)
