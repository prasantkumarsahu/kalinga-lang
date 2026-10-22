# 7. Classes

A class groups data (**fields**) with the code that works on it (**methods**).

## A first class

```kalinga
class BankAccount {
    owner: str
    balance: float = 0.0

    init(owner: str) {
        self.owner = owner
    }

    fn deposit(amount: float) {
        self.balance += amount
    }

    fn withdraw(amount: float) -> bool {
        if amount > self.balance {
            return false
        }
        self.balance -= amount
        return true
    }

    fn summary() -> str {
        return self.owner + ": " + str(self.balance)
    }
}

fn main() {
    let acct = BankAccount("Mira")
    acct.deposit(100.0)
    print(acct.withdraw(30.0), acct.withdraw(500.0))
    print(acct.summary())
}
```

```text
true false
Mira: 70.0
```

The parts of a class:

| Part | Syntax | Notes |
|---|---|---|
| Field | `name: type` or `name: type = default` | Every object gets its own copy |
| Constructor | `init(params) { ... }` | Optional, at most one per class |
| Method | `fn name(params) -> type { ... }` | Same rules as functions |
| `self` | `self.field`, `self.method()` | The object the method was called on |

## Creating objects

Call the class like a function. The arguments go to `init`:

```kalinga
class Point {
    x: int
    y: int

    init(x: int, y: int) {
        self.x = x
        self.y = y
    }
}

fn main() {
    let p = Point(3, 4)
    print(p.x, p.y)
}
```

```text
3 4
```

A class without `init` is created with no arguments, and every field starts at its default.

## Field defaults

A field with `= value` starts with that value. Without one, it starts at the default for
its type:

| Type | Default |
|---|---|
| `int` / `float` / `bool` | `0` / `0.0` / `false` |
| `str` | `""` |
| `[T]` | `[]` |
| `T?` | `none` |
| a class or interface | *no default — you must write one, or make the field optional* |

```kalinga
class Settings {
    volume: int = 50
    muted: bool
    theme: str = "dark"
    tags: [str]
}

fn main() {
    let s = Settings()
    print(s.volume, s.muted, s.theme, s.tags)
}
```

```text
50 false dark []
```

Defaults are filled in first, then `init` runs, so `init` only needs to set what differs.

## Methods and self

Inside a method, `self` is the object the method was called on. Use it to read and change
fields and to call other methods of the same object:

```kalinga
class Counter {
    count: int
    step: int = 1

    fn tick() {
        self.count += self.step
    }

    fn tick_many(n: int) {
        for i in 0..n {
            self.tick()
        }
    }
}

fn main() {
    let c = Counter()
    c.tick_many(5)
    c.step = 10
    c.tick()
    print(c.count)
}
```

```text
15
```

`self` can only be used for `self.field` and `self.method(...)`. It cannot be returned,
stored or passed as an argument.

## Objects are references

A variable of a class type refers to an object; it does not contain a copy. Assigning or
passing it shares the same object:

```kalinga
class Lamp {
    on: bool
}

fn switch_on(l: Lamp) {
    l.on = true
}

fn main() {
    let a = Lamp()
    let b = a          // b and a are the same lamp
    switch_on(b)
    print(a.on)
}
```

```text
true
```

`let` makes the *variable* fixed — it will always point at the same object — but the
object's fields can still change. (Compare arrays and strings, which are copied.)

## Fields that hold objects

A field of a class type needs a default value, or it must be optional:

```kalinga
class Engine {
    hp: int = 150
}

class Car {
    engine: Engine = Engine()   // every car gets its own new engine
    trailer: Car? = none        // may or may not exist
}

fn main() {
    let car = Car()
    car.engine.hp = 300
    print(car.engine.hp, car.trailer == none)
}
```

```text
300 true
```

Optional object fields make linked structures possible:

```kalinga
class Node {
    value: int
    next: Node? = none

    init(value: int) {
        self.value = value
    }
}

fn main() {
    let third = Node(3)
    let second = Node(2)
    second.next = third
    let head = Node(1)
    head.next = second

    var cur: Node? = head
    var total = 0
    while cur != none {
        if let n = cur {
            total += n.value
            cur = n.next
        }
    }
    print("sum of list:", total)
}
```

```text
sum of list: 6
```

## Rules at a glance

- Class names start with an uppercase letter.
- Fields and methods are public: any code can read and change them.
- A field and a method may share a name; the `()` decides which is meant.
- Two fields, or two methods, with the same name in one class are an error.
- Classes, like functions, can be declared in any order.

---

Previous: [6. Optionals](06-optionals.md) · Next: [8. Inheritance](08-inheritance.md)
