# 3. Control Flow

Conditions in Kalinga must be `bool`. There is no "truthy" `0`, `""` or empty array:
`if count { }` is an error, write `if count > 0 { }`. Braces are always required.

## if / else

```kalinga
fn grade(score: int) -> str {
    if score >= 90 {
        return "A"
    } else if score >= 75 {
        return "B"
    } else {
        return "C"
    }
}

fn main() {
    print(grade(95), grade(80), grade(40))
}
```

```text
A B C
```

`else` may also start on the next line:

```kalinga
fn main() {
    let ready = false
    if ready {
        print("go")
    }
    else {
        print("wait")
    }
}
```

## while

Repeats while the condition is `true`:

```kalinga
fn main() {
    var n = 1
    while n < 100 {
        n *= 2
    }
    print(n)
}
```

```text
128
```

`while true { }` is an endless loop; leave it with `break` or `return`.

## for over a range

`for i in a..b` counts from `a` up to, but not including, `b`:

```kalinga
fn main() {
    for i in 0..3 {
        print("step", i)
    }
}
```

```text
step 0
step 1
step 2
```

The bounds can be any `int` expressions, for example `for i in 0..len(items)`.
If `b <= a`, the loop body never runs.

## for over an array

```kalinga
fn main() {
    let fruits = ["apple", "mango", "kiwi"]
    for f in fruits {
        print(f.upper())
    }
}
```

```text
APPLE
MANGO
KIWI
```

The loop works on a copy of the array, so changing the original variable inside the loop
does not affect the iteration.

## for over a string

Each step gives one character as a `str`:

```kalinga
fn main() {
    var vowels = 0
    for ch in "education" {
        if "aeiou".contains(ch) {
            vowels += 1
        }
    }
    print(vowels)
}
```

```text
5
```

## break and continue

- `break` leaves the innermost loop.
- `continue` skips to the next step of the innermost loop.

```kalinga
fn main() {
    for i in 1..20 {
        if i % 2 == 0 {
            continue
        }
        if i > 9 {
            break
        }
        print(i)
    }
}
```

```text
1
3
5
7
9
```

Using `break` or `continue` outside a loop is a compile error.

---

Previous: [2. Basics](02-basics.md) · Next: [4. Functions](04-functions.md)
