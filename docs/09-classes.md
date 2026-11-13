# 9. Classes

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
    println(acct.withdraw(30.0), acct.withdraw(500.0))
    println(acct.summary())
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
    println(p.x, p.y)
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
    println(s.volume, s.muted, s.theme, s.tags)
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
    println(c.count)
}
```

```text
15
```

`self` can also be used as a value — returned, stored or passed to another function —
everywhere except inside `init` (where the object is not finished yet):

```kalinga
class Node {
    value: int
    children: [Node]
    parent: Node? = none

    init(value: int) {
        self.value = value
    }

    fn add(child: Node) {
        child.parent = self
        self.children.push(child)
    }
}

fn main() {
    let root = Node(1)
    let kid = Node(2)
    root.add(kid)
    if let p = kid.parent {
        println("parent of", kid.value, "is", p.value)
    }
}
```

```text
parent of 2 is 1
```

## Private members

Mark a field or method `private` to make it usable only inside the class's own methods:

```kalinga
class Account {
    owner: str
    private balance: int = 0

    init(owner: str) {
        self.owner = owner
    }

    fn deposit(amount: int) {
        if self.valid(amount) {
            self.balance += amount
        }
    }

    fn balance_text() -> str {
        return "{self.owner}: {self.balance}"
    }

    private fn valid(amount: int) -> bool {
        return amount > 0
    }
}

fn main() {
    let a = Account("Ravi")
    a.deposit(50)
    a.deposit(-10)
    println(a.balance_text())
}
```

```text
Ravi: 50
```

Using a private member from outside is a compile error:

```text
error: field `balance` is private to `Account`
```

Subclasses cannot use their base class's private members either.

### Read-only from outside: `private(set)`

A `private(set)` field can be read from anywhere, but only the class's own methods can
change it (assign it, `+=` it, or change it in place, as with `push`). It saves writing a
getter:

```kalinga
class Account {
    owner: str
    private(set) balance: int = 0
    private(set) history: [int] = []

    init(owner: str) {
        self.owner = owner
    }

    fn deposit(amount: int) {
        self.balance += amount
        self.history.push(amount)
    }
}

fn main() {
    let a = Account("Ravi")
    a.deposit(50)
    a.deposit(25)
    println("{a.owner}: {a.balance} {a.history}")
}
```

```text
Ravi: 75 [50, 25]
```

Changing it from outside is a compile error, and so is changing it from a subclass:

```text
error: `balance` can only be changed inside `Account` (it is `private(set)`)
```

Struct fields can be `private(set)` too; only the struct's own `mut fn` methods change them.

## Printing objects: `Show`

Objects cannot be printed until their class says how. List the built-in interface `Show`
after `:` and the class gets an `as_str() -> str` method, written by the compiler from its
fields. `println`, `str(...)`, `"{...}"` and `assert_eq` then use it, also for arrays,
optionals and maps of the objects, and for structs and enums holding them:

```kalinga
class Dog : Show {
    name: str
    age: int

    init(name: str, age: int) {
        self.name = name
        self.age = age
    }
}

class Point : Show {
    x: int
    y: int

    init(x: int, y: int) {
        self.x = x
        self.y = y
    }

    override fn as_str() -> str {
        return "({self.x}, {self.y})"
    }
}

fn main() {
    let rex = Dog("Rex", 3)
    println(rex)
    println("my dog: {rex}")
    println([Point(1, 2), Point(3, 4)])
}
```

```text
Dog(name: Rex, age: 3)
my dog: Dog(name: Rex, age: 3)
[(1, 2), (3, 4)]
```

- The generated text is the class name and every field (base class fields first), each
  shown as `println` would show it.
- `override fn as_str() -> str` replaces it. A subclass inherits `Show`; without its own
  `as_str` it gets a generated one with its own name and fields, and an override may call
  `super.as_str()`.
- Every field must be printable for the generated `as_str` (objects need `Show` too), and a
  class whose fields can lead back to an object of the same class (a linked list, a tree)
  must write its own `as_str`, since the generated one could never finish.
- Inside the class's own methods, print `self.as_str()`, not `self`: printing `self` while
  one of its methods runs uses the object twice (see
  [re-entrant objects](23-safety-and-errors.md#re-entrant-objects)).
- A program that declares its own type named `Show`, `Eq` or `Ord` keeps meaning that
  type after `:`.

## Comparing objects: `Eq` and `Ord`

`==` on objects is a compile error until the class lists `Eq`; then it gets
`equals(other) -> bool`, which compares every field. With `Ord` it also gets
`compare(other) -> int` (negative, zero or positive), comparing the fields in order, and
works with `<`, `<=`, `>`, `>=` and `sort`. `Ord` includes `Eq`. Either method can be
overridden:

```kalinga
class Version : Show, Ord {
    major: int
    minor: int

    init(major: int, minor: int) {
        self.major = major
        self.minor = minor
    }
}

class User : Eq {
    id: int
    name: str

    init(id: int, name: str) {
        self.id = id
        self.name = name
    }

    // The same user whatever the name.
    override fn equals(other: User) -> bool {
        return self.id == other.id
    }
}

fn main() {
    println(Version(1, 2) == Version(1, 2))
    println(Version(1, 2) < Version(1, 10))
    var versions = [Version(1, 10), Version(0, 9), Version(1, 2)]
    versions.sort()
    println(versions)
    println(User(1, "Ann") == User(1, "Annie"))
    println([User(2, "Bo")].contains(User(2, "Bob")))
}
```

```text
true
true
[Version(major: 0, minor: 9), Version(major: 1, minor: 2), Version(major: 1, minor: 10)]
true
true
```

- `==`, `!=`, `contains`, `assert_eq` and `<T: Eq>` use `equals`; `<`, `sort` and
  `<T: Ord>` use `compare`. The same object is always equal to itself.
- `other` has the type of the class that lists `Eq` / `Ord`; subclasses inherit its
  `equals` and `compare`, and may override them with the same signature.
- The generated methods need fields that can be compared (`Eq`) or ordered (`Ord`).

## Static methods

A `static fn` belongs to the class itself, not to an object. It has no `self` and is
called on the class name:

```kalinga
class Temperature {
    celsius: float

    init(celsius: float) {
        self.celsius = celsius
    }

    static fn from_fahrenheit(f: float) -> Temperature {
        return Temperature((f - 32.0) * 5.0 / 9.0)
    }

    static fn freezing() -> Temperature {
        return Temperature(0.0)
    }
}

fn main() {
    let t = Temperature.from_fahrenheit(212.0)
    println(t.celsius, Temperature.freezing().celsius)
}
```

```text
100.0 0.0
```

Static methods are handy as named constructors ("factories") and for helpers that do not
need an object.

## Constants and static variables

A class or struct can hold values that belong to the type rather than to one object:

- `const NAME: Type = value` is a constant, used as `Type.NAME`.
- `static var name: Type = value` is one variable shared by the whole type, used and
  changed as `Type.name`.

```kalinga
class Enemy {
    const MAX_HP: int = 30
    static var alive: int = 0

    hp: int = Enemy.MAX_HP

    init() {
        Enemy.alive += 1
    }
}

fn main() {
    let a = Enemy()
    let b = Enemy()
    println(Enemy.alive, a.hp, Enemy.MAX_HP)
}
```

```text
2 30 30
```

Rules:

- The type is always written out, as for fields.
- A constant's value is computed each time it is used, so a constant that holds an object
  gives a new object each time. Use `static var` for one shared object.
- A `static var` gets its starting value the first time it is used. Each thread has its
  own copy (see [Threads](20-threads.md)).
- A value cannot be defined in terms of itself (`const A: int = A + 1`).
- Generic classes and structs cannot have constants or static variables (yet).

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
    let b = a // b and a are the same lamp
    switch_on(b)
    println(a.on)
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
    engine: Engine = Engine() // every car gets its own new engine
    trailer: Car? = none      // may or may not exist
}

fn main() {
    let car = Car()
    car.engine.hp = 300
    println(car.engine.hp, car.trailer == none)
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
    println("sum of list:", total)
}
```

```text
sum of list: 6
```

For small pieces of data that are copied around a lot — vectors, colours, particles — a
[struct](14-structs-and-operators.md) is usually a better fit than a class: it is a value,
stored inline, and supports operators.

## Rules at a glance

- Class names start with an uppercase letter.
- Fields and methods are public unless marked `private`; `private(set)` fields are
  readable everywhere and changed only inside the class.
- `: Show`, `: Eq` and `: Ord` make objects printable and comparable (`as_str`, `equals`,
  `compare`, generated from the fields unless overridden).
- `static fn` methods are called as `ClassName.method(...)` and have no `self`.
- A field and a method may share a name; the `()` decides which is meant.
- Two fields, or two methods, with the same name in one class are an error.
- Classes, like functions, can be declared in any order.

---

Previous: [8. Enums and match](08-enums-and-match.md) · Next: [10. Inheritance](10-inheritance.md)
