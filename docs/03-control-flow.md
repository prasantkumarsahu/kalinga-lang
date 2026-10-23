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
    println(grade(95), grade(80), grade(40))
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
        println("go")
    }
    else {
        println("wait")
    }
}
```

### if as a value

An `if` can also give a value. Each branch ends with the value it gives, and `else` is
required:

```kalinga
fn main() {
    let score = 82
    let grade = if score >= 90 { "A" } else if score >= 75 { "B" } else { "C" }
    let bonus = if score > 80 {
        let extra = score - 80
        extra * 10
    } else {
        0
    }
    println(grade, bonus)
}
```

```text
B 20
```

All branches must give the same type. A branch that gives `none` makes the value optional
(`if found { x } else { none }` is an `int?` when `x` is an `int`).

## while

Repeats while the condition is `true`:

```kalinga
fn main() {
    var n = 1
    while n < 100 {
        n *= 2
    }
    println(n)
}
```

```text
128
```

`while true { }` is an endless loop; leave it with `break` or `return`.

### while let

`while let name = value` repeats while an optional value is not `none`, with `name` holding
the value inside (see [Optionals](07-optionals.md)):

```kalinga
fn main() {
    var todo = ["wash", "cook", "shop"]
    while let task = todo.pop() {
        println("doing", task)
    }
}
```

```text
doing shop
doing cook
doing wash
```

## for over a range

`for i in a..b` counts from `a` up to, but not including, `b`:

```kalinga
fn main() {
    for i in 0..3 {
        println("step", i)
    }
}
```

```text
step 0
step 1
step 2
```

The bounds can be any `int` expressions, for example `for i in 0..len(items)`.
If `b <= a`, the loop body never runs. `a..=b` includes `b`:

```kalinga
fn main() {
    var total = 0
    for i in 1..=10 {
        total += i
    }
    println(total)
}
```

```text
55
```

## for over an array

```kalinga
fn main() {
    let fruits = ["apple", "mango", "kiwi"]
    for f in fruits {
        println(f.upper())
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

With two names, the first is the position (from `0`) and the second the element:

```kalinga
fn main() {
    let podium = ["Asha", "Ravi", "Mira"]
    for i, name in podium {
        println(i + 1, name)
    }
}
```

```text
1 Asha
2 Ravi
3 Mira
```

`for i, ch in text` does the same for the characters of a string.

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
    println(vowels)
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
        println(i)
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
