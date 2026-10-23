# 10. Inheritance

A class can **extend** one other class. It inherits all of its fields and methods, can
add new ones, and can **override** methods to change their behaviour.

## Extending a class

Write the base class after `:`:

```kalinga
class Animal {
    name: str
    sound: str = "..."

    init(name: str) {
        self.name = name
    }

    fn speak() -> str {
        return self.name + " says " + self.sound
    }
}

class Dog : Animal {
    init(name: str) {
        super(name)          // run Animal's init first
        self.sound = "woof"
    }

    fn fetch() -> str {
        return self.name + " fetches the ball"
    }
}

fn main() {
    let d = Dog("Rex")
    print(d.speak())    // inherited from Animal
    print(d.fetch())    // defined in Dog
}
```

```text
Rex says woof
Rex fetches the ball
```

Terms used below: `Animal` is the **base class** (parent), `Dog` is the **subclass**
(child).

A class has at most one base class. Listing two is an error:

```text
error: `C` already has a base class; a class can extend only one class (but any number of interfaces)
```

## Constructors and super(...)

- If the base class has an `init`, the subclass's `init` **must** call `super(...)` with
  the base constructor's arguments. Put it first, so the base part is ready before the
  subclass uses it.
- If a subclass has no `init` of its own, it reuses the base class's constructor:

```kalinga
class Shape {
    label: str

    init(label: str) {
        self.label = label
    }
}

class Blob : Shape {
    // no init: Blob("x") calls Shape's init
}

fn main() {
    let b = Blob("blobby")
    print(b.label)
}
```

```text
blobby
```

## Overriding methods

A subclass replaces an inherited method by declaring it with `override fn`. The override
must have exactly the same parameter types and return type:

```kalinga
class Shape {
    fn area() -> float {
        return 0.0
    }

    fn describe() -> str {
        return "area " + str(self.area())
    }
}

class Rect : Shape {
    w: float
    h: float

    init(w: float, h: float) {
        self.w = w
        self.h = h
    }

    override fn area() -> float {
        return self.w * self.h
    }
}

class Circle : Shape {
    r: float

    init(r: float) {
        self.r = r
    }

    override fn area() -> float {
        return 3.14159 * self.r * self.r
    }
}

fn main() {
    print(Rect(2.0, 3.0).describe())
    print(Circle(1.0).describe())
}
```

```text
area 6.0
area 3.14159
```

Note how `describe()`, written once in `Shape`, calls the right `area()` for each
object. That is **dynamic dispatch**: the method that runs depends on the object's real
class, not on where the call was written.

`override` is required and checked, which catches two common bugs:

| Mistake | Error |
|---|---|
| Overriding without the keyword | ``method `area` overrides `Shape.area`; mark it `override fn` `` |
| `override` on a method the base does not have (often a typo) | ``` `aera` is marked override but no base class or interface has this method ``` |
| Different parameter or return types | ``override of `area` must match `Shape.area() -> float` `` |

## Calling the base version: super.method()

Inside an override, `super.name(...)` calls the base class's version:

```kalinga
class Logger {
    fn format(msg: str) -> str {
        return "[log] " + msg
    }
}

class TimedLogger : Logger {
    override fn format(msg: str) -> str {
        return "12:00 " + super.format(msg)
    }
}

fn main() {
    print(TimedLogger().format("started"))
}
```

```text
12:00 [log] started
```

## Polymorphism: using a subclass as its base class

Anywhere an `Animal` is expected, any subclass of `Animal` can be used. This works for
variables, parameters, return values, arrays and optionals:

```kalinga
class Animal {
    name: str
    init(name: str) {
        self.name = name
    }
    fn speak() -> str {
        return self.name + " makes a sound"
    }
}

class Cat : Animal {
    override fn speak() -> str {
        return self.name + " meows"
    }
}

class Cow : Animal {
    override fn speak() -> str {
        return self.name + " moos"
    }
}

fn introduce(a: Animal) {
    print(a.speak())
}

fn main() {
    let farm: [Animal] = [Cat("Tom"), Cow("Bella"), Animal("Generic")]
    for a in farm {
        introduce(a)
    }
}
```

```text
Tom meows
Bella moos
Generic makes a sound
```

When mixing subclasses in one array, annotate the array's type (`let farm: [Animal] = ...`).
Without it, the type is taken from the first element.

The conversion is automatic only one way: a `Cat` is always an `Animal`, but an `Animal`
might not be a `Cat`. To go the other way, ask — see `is` and `as` below.

## Checking the real class: is and as

- `x is Cat` is `true` when the object is a `Cat` (or a subclass of `Cat`).
- `x as Cat` gives a `Cat?`: the same object seen as a `Cat`, or `none`. Use it with `if let`.

```kalinga
class Animal {
    name: str
    init(name: str) {
        self.name = name
    }
}

class Dog : Animal {
    fn fetch() -> str {
        return self.name + " fetches the ball"
    }
}

class Cat : Animal {}

fn main() {
    let pets: [Animal] = [Dog("Rex"), Cat("Tom")]
    for p in pets {
        if let d = p as Dog {
            print(d.fetch())
        } else {
            print(p.name, "is not a dog")
        }
        print(p is Cat)
    }
}
```

```text
Rex fetches the ball
false
Tom is not a dog
true
```

`as` and `is` also work on interface values. A check that can never succeed — for
example asking whether a `Dog` is a `Cat` when neither extends the other — is a compile
error.

## Abstract classes

An **abstract class** is a base class that is not meant to be created on its own. It can
declare **abstract methods** — methods with no body that every concrete subclass must
implement:

```kalinga
abstract class Shape {
    name: str

    init(name: str) {
        self.name = name
    }

    abstract fn area() -> float

    fn describe() -> str {
        return "{self.name} with area {self.area()}"
    }
}

class Square : Shape {
    side: float

    init(side: float) {
        super("square")
        self.side = side
    }

    override fn area() -> float {
        return self.side * self.side
    }
}

fn main() {
    let shapes: [Shape] = [Square(2.0), Square(3.0)]
    for s in shapes {
        print(s.describe())
    }
}
```

```text
square with area 4.0
square with area 9.0
```

The compiler enforces the rules:

| Mistake | Error |
|---|---|
| `Shape("x")` | `` `Shape` is abstract and cannot be created; create one of its subclasses `` |
| a subclass forgets `area` | `` `Circle` must implement abstract method `Shape.area` (or be declared `abstract class`) `` |
| `abstract fn` in a normal class | `` `Thing` has an abstract method, so it must be declared `abstract class Thing` `` |

## Multi-level hierarchies

Inheritance can go several levels deep. Each level can override again, and `super`
always refers to the next class up:

```kalinga
class A {
    fn who() -> str {
        return "A"
    }
}

class B : A {
    override fn who() -> str {
        return "B>" + super.who()
    }
}

class C : B {
    override fn who() -> str {
        return "C>" + super.who()
    }
}

fn main() {
    let x: A = C()
    print(x.who())
}
```

```text
C>B>A
```

## Rules at a glance

- `class Child : Base` — one base class; interfaces may also be listed (next chapter).
- Subclasses inherit every field and method. A subclass may not declare a field with the
  same name as an inherited one.
- Overrides need `override fn` and the same signature.
- `super(...)` is required in `init` when the base has an `init`.
- Inheritance cycles (`A : B` and `B : A`) are errors.
- `abstract class` cannot be created; its `abstract fn`s must be implemented by subclasses.
- `x is C` tests the real class; `x as C` converts to `C?`.
- Through a base-class variable, you can read and assign fields (`a.name = "x"`) and call
  methods, but not modify *inside* a field (`a.list.push(1)`). Add a method for that.

---

Previous: [9. Classes](09-classes.md) · Next: [11. Interfaces](11-interfaces.md)
