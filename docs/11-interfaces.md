# 11. Interfaces

An **interface** is a list of method signatures with no code. A class that lists the
interface promises to provide all of those methods. Code can then work with "anything
that implements `Drawable`" without knowing the actual classes.

Use inheritance for "is a" relationships (`Dog` is an `Animal`). Use interfaces for "can
do" relationships (a `Document`, a `Chart` and a `Dog` can all be `Printable`).

## Declaring and implementing an interface

```kalinga
interface Describable {
    fn describe() -> str
}

class Book : Describable {
    title: str

    init(title: str) {
        self.title = title
    }

    override fn describe() -> str {
        return "Book: " + self.title
    }
}

class Planet : Describable {
    name: str
    moons: int

    init(name: str, moons: int) {
        self.name = name
        self.moons = moons
    }

    override fn describe() -> str {
        return self.name + " with " + str(self.moons) + " moons"
    }
}

fn main() {
    let things: [Describable] = [Book("Dune"), Planet("Mars", 2)]
    for t in things {
        print(t.describe())
    }
}
```

```text
Book: Dune
Mars with 2 moons
```

- Interface methods are written like method headers: `fn name(params) -> type`, no body.
- Implementing methods are marked `override fn`, exactly like methods that replace a base
  class method. In `class Book : Describable` you cannot tell from the `:` line whether
  `Describable` is a class or an interface, so every method that comes from something after
  `:` is marked the same way. Forgetting it is an error:

```text
error: method `describe` implements `Describable.describe`; mark it `override fn`
```

- The class must provide **every** method with the **same** signature:

```text
error: `Book` does not implement `Describable.describe() -> str`
```

## Interfaces as types

An interface name can be used as a type for variables, parameters, return values, array
elements and optionals. Only the interface's methods can be called through it:

```kalinga
interface Scorer {
    fn score(word: str) -> int
}

class LengthScorer : Scorer {
    override fn score(word: str) -> int {
        return len(word)
    }
}

class VowelScorer : Scorer {
    override fn score(word: str) -> int {
        var n = 0
        for ch in word {
            if "aeiou".contains(ch) {
                n += 1
            }
        }
        return n
    }
}

fn best(words: [str], s: Scorer) -> str {
    var top = words[0]
    for w in words {
        if s.score(w) > s.score(top) {
            top = w
        }
    }
    return top
}

fn main() {
    let words = ["sky", "banana", "queue"]
    print(best(words, LengthScorer()))
    print(best(words, VowelScorer()))
}
```

```text
banana
queue
```

Interfaces cannot be created directly (`Scorer()` is an error) and have no fields.

## Base class and interfaces together

After `:`, list at most one base class plus any number of interfaces, separated by
commas. Writing the base class first is the convention:

```kalinga
interface Printable {
    fn to_text() -> str
}

interface Priced {
    fn price() -> float
}

class Item {
    name: str
    init(name: str) {
        self.name = name
    }
}

class Laptop : Item, Printable, Priced {
    override fn to_text() -> str {
        return "Laptop(" + self.name + ")"
    }

    override fn price() -> float {
        return 999.0
    }
}

fn total(items: [Priced]) -> float {
    var sum = 0.0
    for i in items {
        sum += i.price()
    }
    return sum
}

fn main() {
    let l = Laptop("Zen")
    let p: Printable = l
    print(p.to_text(), total([l, Laptop("Pro")]))
}
```

```text
Laptop(Zen) 1998.0
```

## Interfaces and inheritance

- A method required by an interface may be inherited from the base class.
- Subclasses automatically implement every interface their base class implements, and may
  override those methods.

```kalinga
interface Speaker {
    fn speak() -> str
}

class Robot {
    fn speak() -> str {
        return "beep"
    }
}

class Droid : Robot, Speaker {
    // speak() is inherited from Robot, which satisfies Speaker
}

class Chatbot : Droid {
    override fn speak() -> str {
        return "hello there"
    }
}

fn main() {
    let voices: [Speaker] = [Droid(), Chatbot()]
    for v in voices {
        print(v.speak())
    }
}
```

```text
beep
hello there
```

## Interfaces extending interfaces

An interface can build on others with `:`. A class implementing it must provide all the
methods, and its values can be used as any of the interfaces:

```kalinga
interface Named {
    fn name() -> str
}

interface Pet : Named {
    fn sound() -> str
}

class Dog : Pet {
    override fn name() -> str {
        return "Rex"
    }

    override fn sound() -> str {
        return "woof"
    }
}

fn greet(n: Named) {
    print("hello", n.name())
}

fn main() {
    let p: Pet = Dog()
    print(p.name(), p.sound())
    greet(p)    // a Pet is also a Named
}
```

```text
Rex woof
hello Rex
```

## Limits in this version

- Interfaces contain only methods: no fields, no default method bodies.
- Two interfaces listed by one class should not declare methods with the same name.

---

Previous: [10. Inheritance](10-inheritance.md) · Next: [12. Lambdas and Function Values](12-lambdas.md)
