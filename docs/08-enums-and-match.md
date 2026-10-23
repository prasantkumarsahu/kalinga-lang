# 8. Enums and match

An **enum** is a type with a fixed list of possible values, called **variants**. Variants
can carry data. `match` runs different code for each variant — and the compiler checks
that you handled them all.

## Simple enums

```kalinga
enum Direction {
    North
    East
    South
    West
}

fn turn_right(d: Direction) -> Direction {
    match d {
        North => return Direction.East
        East => return Direction.South
        South => return Direction.West
        West => return Direction.North
    }
}

fn main() {
    var d = Direction.North
    for i in 0..3 {
        d = turn_right(d)
        println(d)
    }
    println(d == Direction.West)
}
```

```text
East
South
West
true
```

- Variants are written one per line, or separated by commas: `enum Light { Red, Amber, Green }`.
- Create a value with `EnumName.Variant`.
- Variant names start with an uppercase letter.

## Variants with data

A variant can carry values, declared like parameters:

```kalinga
enum Shape {
    Circle(r: float)
    Rect(w: float, h: float)
    Empty
}

fn area(s: Shape) -> float {
    match s {
        Circle(r) => return 3.14159 * r * r
        Rect(w, h) => return w * h
        Empty => return 0.0
    }
}

fn main() {
    let shapes = [Shape.Circle(1.0), Shape.Rect(2.0, 3.0), Shape.Empty]
    for s in shapes {
        println(s, area(s))
    }
}
```

```text
Circle(1.0) 3.14159
Rect(2.0, 3.0) 6.0
Empty 0.0
```

In a pattern, the names in parentheses (`r`, `w`, `h`) are new variables holding that
variant's data. Use `_` to ignore one: `Rect(w, _) => ...`. A pattern may also be written
with the enum name: `Shape.Circle(r) =>`.

## match

```kalinga
enum Command {
    Move(steps: int)
    Say(text: str)
    Stop
}

fn run(c: Command) {
    match c {
        Move(n) => {
            if n > 0 {
                println("moving", n, "steps")
            } else {
                println("staying put")
            }
        }
        Say(t) => println("saying: {t}")
        _ => println("other command")
    }
}

fn main() {
    run(Command.Move(3))
    run(Command.Move(0))
    run(Command.Say("hi"))
    run(Command.Stop)
}
```

```text
moving 3 steps
staying put
saying: hi
other command
```

- Each arm is `pattern => statement` or `pattern => { block }`.
- `_` matches anything that earlier arms did not.
- A `match` must handle **every** variant, or have a `_` arm:

```kalinga error
enum Light { Red, Amber, Green }

fn main() {
    match Light.Red {
        Red => println("stop")
        Green => println("go")
    }
}
```

```text
main.kl:4:5: error: `match` does not handle every case (missing: Amber)
```

When you later add a variant to an enum, every `match` that forgot it becomes a compile
error — so nothing silently falls through.

A `match` whose arms all `return` counts as returning, so a function can end with one
(like `area` above).

### match as a value

A `match` can give a value, too: each arm is `Pattern => value`, or a block that ends with
its value. Arms on one line are separated by commas:

```kalinga
enum Dir {
    Up
    Down
    Left
    Right
}

fn main() {
    let d = Dir.Left
    let dx = match d { Left => -1, Right => 1, _ => 0 }
    let name = match d {
        Up => "up"
        Down => "down"
        Left => {
            let arrow = "<-"
            "left {arrow}"
        }
        Right => "right"
    }
    println(dx, name)
}
```

```text
-1 left <-
```

As with `if`, every arm must give the same type, and the `match` must handle every case.

## Matching numbers, strings and booleans

`match` also works on `int`, `str` and `bool` values, using literal patterns. These need a
`_` arm (except a `bool` match with both `true` and `false`):

```kalinga
fn describe(n: int) -> str {
    match n {
        0 => return "zero"
        1 => return "one"
        -1 => return "minus one"
        _ => return "many"
    }
}

fn main() {
    println(describe(0), describe(-1), describe(42))
    match "kiwi" {
        "apple" => println("red")
        "kiwi" => println("green")
        _ => println("unknown")
    }
}
```

```text
zero minus one many
green
```

## Enums are values

Enums are copied when assigned, like numbers and strings. An enum whose data is all
printable can be printed and compared with `==`. An enum may carry objects too:

```kalinga
class User {
    name: str
    init(name: str) {
        self.name = name
    }
}

enum Owner {
    Nobody
    Person(u: User)
}

fn main() {
    let o = Owner.Person(User("Asha"))
    match o {
        Person(u) => println("owned by", u.name)
        Nobody => println("free")
    }
}
```

```text
owned by Asha
```

(Enums that hold objects cannot be printed or compared with `==`; use `match` instead.)

## Recursive enums

A variant can hold its own enum, which describes trees and lists naturally:

```kalinga
enum Expr {
    Num(value: int)
    Add(left: Expr, right: Expr)
    Neg(inner: Expr)
}

fn eval(e: Expr) -> int {
    match e {
        Num(n) => return n
        Add(a, b) => return eval(a) + eval(b)
        Neg(x) => return -eval(x)
    }
}

fn main() {
    let e = Expr.Add(Expr.Num(2), Expr.Neg(Expr.Num(5)))
    println(e)
    println(eval(e))
}
```

```text
Add(Num(2), Neg(Num(5)))
-3
```

(Kalinga stores such fields behind a pointer, a `Box` in the generated Rust, so the
enum keeps a fixed size.) For any number of children, use an array:

```kalinga
enum Tree {
    Leaf(value: int)
    Node(children: [Tree])
}

fn total(t: Tree) -> int {
    match t {
        Leaf(v) => return v
        Node(kids) => {
            var sum = 0
            for k in kids {
                sum += total(k)
            }
            return sum
        }
    }
}

fn main() {
    let t = Tree.Node([Tree.Leaf(1), Tree.Node([Tree.Leaf(2), Tree.Leaf(3)])])
    println(total(t))
}
```

```text
6
```

---

Previous: [7. Optionals](07-optionals.md) · Next: [9. Classes](09-classes.md)
