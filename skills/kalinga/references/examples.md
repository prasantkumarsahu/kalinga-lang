# Kalinga example programs

Complete programs in the standard layout, each with its output. Copy patterns from them.

## Contents

- [Inheritance, abstract classes and casts](#inheritance-abstract-classes-and-casts)
- [Structs with operators](#structs-with-operators)
- [Generic classes and interfaces](#generic-classes-and-interfaces)
- [Constants, static variables and private members](#constants-static-variables-and-private-members)
- [A word counter (strings and maps)](#a-word-counter-strings-and-maps)
- [Callbacks that change state](#callbacks-that-change-state)

## Inheritance, abstract classes and casts

```kalinga
abstract class Animal {
    name: str

    init(name: str) {
        self.name = name
    }

    abstract fn sound() -> str

    fn speak() -> str {
        return "{self.name} says {self.sound()}"
    }
}

class Dog : Animal {
    tricks: [str] = []

    init(name: str) {
        super(name)
    }

    override fn sound() -> str {
        return "woof"
    }

    fn learn(trick: str) {
        self.tricks.push(trick)
    }
}

class Cat : Animal {
    init(name: str) {
        super(name)
    }

    override fn sound() -> str {
        return "meow"
    }
}

fn main() {
    let rex = Dog("Rex")
    rex.learn("sit")
    let pets: [Animal] = [rex, Cat("Tom")]
    for p in pets {
        println(p.speak())
        if let d = p as Dog {
            println("  knows", d.tricks)
        }
        println("  is a cat:", p is Cat)
    }
}
```

```text
Rex says woof
  knows [sit]
  is a cat: false
Tom says meow
  is a cat: true
```

- `abstract class` cannot be created; `abstract fn` has no body and subclasses must
  `override` it. `super(name)` comes first in a subclass `init`.
- `x as Dog` gives a `Dog?`; `x is Cat` a `bool`.

## Structs with operators

```kalinga
struct Vec2 {
    x: float
    y: float

    fn length() -> float {
        return Math.sqrt(self.x * self.x + self.y * self.y)
    }

    mut fn scale(k: float) {
        self.x *= k
        self.y *= k
    }

    op +(o: Vec2) -> Vec2 {
        return Vec2(self.x + o.x, self.y + o.y)
    }

    op *(k: float) -> Vec2 {
        return Vec2(self.x * k, self.y * k)
    }
}

fn main() {
    let a = Vec2(3, 4)
    var b = a
    b.scale(2.0)
    println(a, b, a.length())
    println(a + b * 0.5)
}
```

```text
Vec2(x: 3.0, y: 4.0) Vec2(x: 6.0, y: 8.0) 5.0
Vec2(x: 6.0, y: 8.0)
```

- Structs are values: `var b = a` copies, so scaling `b` leaves `a` alone. A `mut fn`
  changes `self` and needs a `var`.
- A struct is created with its fields in order (`Vec2(3, 4)`; whole numbers are accepted
  for `float` fields).

## Generic classes and interfaces

```kalinga
interface Store<T> {
    fn get() -> T
    fn put(x: T)
}

class Counter : Store<int> {
    n: int = 0

    override fn get() -> int {
        return self.n
    }

    override fn put(x: int) {
        self.n = x
    }
}

class Stack<T> {
    items: [T] = []

    fn push(x: T) {
        self.items.push(x)
    }

    fn pop() -> T? {
        return self.items.pop()
    }
}

fn bump<S: Store<int>>(s: S, by: int) {
    s.put(s.get() + by)
}

fn describe<T: Show>(xs: [T]) -> str {
    var parts: [str] = []
    for x in xs {
        parts.push("<{x}>")
    }
    return parts.join(" ")
}

fn main() {
    let c = Counter()
    bump(c, 5)
    bump(c, 2)
    println(c.get())

    let s = Stack<str>()
    s.push("a")
    s.push("b")
    println(s.pop(), s.pop(), s.pop())
    println(describe([1, 2, 3]))
}
```

```text
7
b a none
<1> <2> <3>
```

- `Stack<str>()` names the type argument when nothing else tells it (an empty stack).
- `T: Show` allows printing a `T`; `S: Store<int>` allows calling `Store`'s methods.

## Constants, static variables and private members

```kalinga
class Account {
    const LIMIT: int = 1000
    static var opened: int = 0
    private balance: int = 0
    owner: str

    init(owner: str) {
        self.owner = owner
        Account.opened += 1
    }

    fn deposit(amount: int) -> Result<int, str> {
        if self.balance + amount > Account.LIMIT {
            return Err("over the limit of {Account.LIMIT}")
        }
        self.balance += amount
        return Ok(self.balance)
    }
}

fn main() {
    let a = Account("Asha")
    let b = Account("Ravi")
    println(a.deposit(600), a.deposit(600), b.deposit(50))
    println(Account.opened, "accounts")
}
```

```text
Ok(600) Err(over the limit of 1000) Ok(50)
2 accounts
```

## A word counter (strings and maps)

```kalinga
fn count_words(text: str) -> {str: int} {
    var counts: {str: int} = {}
    for raw in text.lower().split(" ") {
        let w = raw.trim()
        if w == "" {
            continue
        }
        if let n = counts.get(w) {
            counts[w] = n + 1
        } else {
            counts[w] = 1
        }
    }
    return counts
}

fn main() {
    let counts = count_words("the cat and the hat  and THE bat")
    println(counts)
    var pairs: [(str, int)] = []
    for word, n in counts {
        pairs.push((word, n))
    }
    pairs.sort_by(fn(a, b) => a.1 > b.1)
    let (top, times) = pairs[0]
    println("most common:", top, times)
}
```

```text
{and: 2, bat: 1, cat: 1, hat: 1, the: 3}
most common: the 3
```

- Maps iterate in key order. `sort_by` takes "should `a` come before `b`".

## Callbacks that change state

```kalinga
class Button {
    label: str
    handlers: [fn()] = []

    init(label: str) {
        self.label = label
    }

    fn on_click(f: fn()) {
        self.handlers.push(f)
    }

    fn click() {
        for h in self.handlers {
            h()
        }
    }
}

fn main() {
    var clicks = 0
    var log: [str] = []
    let b = Button("ok")
    b.on_click(fn() {
        clicks += 1
    })
    b.on_click(fn() {
        log.push("clicked")
    })
    b.click()
    b.click()
    println(clicks, log)
}
```

```text
2 [clicked, clicked]
```

- The lambdas change `clicks` and `log`, so those `var`s are shared with them; a variable
  no lambda changes is copied when the lambda is made.
- The handlers must not call back into `b` itself while `b.click()` runs (that reaches the
  same object twice and stops the program).
