# 6. Maps

A **map** stores values under keys, like a dictionary. Its type is written `{K: V}`:
`{str: int}` maps text to numbers.

```kalinga
fn main() {
    var stock = {"apples": 3, "pears": 5}
    stock["kiwis"] = 12   // add a new key
    stock["apples"] += 10 // change an existing value
    println(stock)
    println(stock["pears"], len(stock))
}
```

```text
{apples: 13, kiwis: 12, pears: 5}
5 3
```

- Keys can be `int`, `str` or `bool`. Values can be any type.
- Maps keep their keys **sorted**, so printing and looping always give the same order.
- Like arrays, maps are values: assigning or passing a map copies it.
- An empty map needs a type: `var m: {str: int} = {}`.

## Reading safely

`m[key]` stops the program if the key is missing:

```text
runtime error: key not found: plums
```

When a key may be missing, use `get`, which returns an optional:

```kalinga
fn main() {
    let ages = {"Asha": 31, "Ravi": 27}
    if let a = ages.get("Asha") {
        println("Asha is {a}")
    }
    println(ages.get("Mira"), ages.has("Mira"))
}
```

```text
Asha is 31
none false
```

## Map methods

| Method | Returns | Description |
|---|---|---|
| `m.get(k)` | `V?` | The value for `k`, or `none` |
| `m.has(k)` | `bool` | Whether `k` is present |
| `m.remove(k)` | `V?` | Removes `k` and returns its value (needs `var`) |
| `m.keys()` | `[K]` | All keys, sorted |
| `m.values()` | `[V]` | All values, in key order |
| `m.len()` | `int` | Number of entries (same as `len(m)`) |

## Looping

`for k in m` visits the keys in sorted order:

```kalinga
fn main() {
    let scores = {"b": 2, "a": 1, "c": 3}
    for name in scores {
        println("{name} = {scores[name]}")
    }
    println(scores.keys(), scores.values())
}
```

```text
a = 1
b = 2
c = 3
[a, b, c] [1, 2, 3]
```

With two names, the loop gives each key and its value:

```kalinga
fn main() {
    let stock = {"pens": 12, "ink": 3}
    for item, count in stock {
        println("{item}: {count}")
    }
}
```

```text
ink: 3
pens: 12
```

## Counting with a map

A common pattern: count how often each item appears.

```kalinga
fn count_words(text: [str]) -> {str: int} {
    var counts: {str: int} = {}
    for w in text {
        if counts.has(w) {
            counts[w] += 1
        } else {
            counts[w] = 1
        }
    }
    return counts
}

fn main() {
    let words = ["to", "be", "or", "not", "to", "be"]
    println(count_words(words))
}
```

```text
{be: 2, not: 1, or: 1, to: 2}
```

## Maps of arrays (and other nesting)

Values can be arrays, maps or objects, and can be changed in place:

```kalinga
fn main() {
    var groups: {str: [str]} = {}
    groups["fruit"] = ["apple"]
    groups["fruit"].push("mango")
    groups["veg"] = ["leek"]
    println(groups)
    if let removed = groups.remove("veg") {
        println("removed", removed)
    }
    println(groups)
}
```

```text
{fruit: [apple, mango], veg: [leek]}
removed [leek]
{fruit: [apple, mango]}
```

---

Previous: [5. Arrays and Strings](05-arrays-and-strings.md) · Next: [7. Optionals](07-optionals.md)
