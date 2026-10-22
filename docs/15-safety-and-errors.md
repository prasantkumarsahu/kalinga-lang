# 15. Safety and Errors

Kalinga is designed so that mistakes are caught as early as possible — ideally by the
compiler, otherwise by a clear runtime error — and never turn into silent memory
corruption.

## What Kalinga guarantees

| Guarantee | How |
|---|---|
| No null pointer crashes | There is no null; missing values are `T?` and must be unwrapped with `if let` |
| No reading or writing outside an array | Every index is checked |
| No use-after-free, dangling pointers or double frees | Memory is managed automatically (reference counting) |
| No silent integer overflow | Overflow stops the program |
| No accidental mutation | Variables are immutable unless declared with `var` |
| No surprise type conversions | Every conversion is written explicitly |
| Overrides and interfaces match | Signatures are checked at compile time |

The generated Rust code contains no `unsafe` blocks, so these guarantees are backed by the
Rust compiler as well.

## Compile errors

The compiler stops at the first error and shows the file, line, column and the offending
line:

```text
shop.kl:12:21: error: expected `int`, found `str`
12 |     let quantity: int = "three"
   |                         ^
```

Common compile errors and what to do:

| Message | Cause | Fix |
|---|---|---|
| `cannot change `x`: it was declared with `let`` | Assigning to an immutable variable | Declare it with `var` |
| `expected `int`, found `float`` | Mixing types | Convert with `int()`, `float()`, `str()` |
| `cannot apply `+` to `str` and `int`` | Joining text and numbers | `"n=" + str(n)` |
| `` `f` must return a `int` on every path`` | Some path has no `return` | Add an `else` or final `return` |
| `cannot infer the type of `[]`` / `` `none` `` | Value alone has no type | Annotate: `let xs: [int] = []` |
| `condition must be `bool`` | `if count { }` | `if count > 0 { }` |
| `` `A` has no field `b` `` / `has no method` | Typo, or member not on that type | Check the class (or interface) definition |
| `mark it `override fn`` | Same method name as the base class | Add `override`, or rename |
| `does not implement `I.m() -> T`` | Interface method missing or different | Add the method with the exact signature |
| `` `init` must call `super(...)` `` | Subclass skipped base construction | Call `super(args)` at the top of `init` |
| `needs a default value (or make it optional)` | Object field without a value | `= ClassName()` or `field: ClassName?` |
| `cannot print a `Dog` value` | Printing an object | Print a field or method result |
| `` `match` does not handle every case (missing: ...) `` | A variant has no arm | Add the arm, or a `_ =>` arm |
| `` field `x` is private to `A` `` | Using a private member from outside | Add a public method |
| `` `A` is abstract and cannot be created `` | Creating an abstract class | Create a subclass |
| `` cannot tell what `T` is `` | Generic call without enough information | Write the type: `Stack<int>()` |
| `` cannot change `n` inside a lambda `` | Assigning to a captured variable | Keep shared state in an object |

## Runtime errors

Some problems can only be detected while the program runs. Kalinga stops the program
immediately with a message on stderr and exit code `101`:

```kalinga
fn main() {
    let xs = [1, 2, 3]
    let i = 5
    print("before")
    print(xs[i])
    print("never printed")
}
```

```text
before
runtime error: index out of bounds: the len is 3 but the index is 5
```

| Runtime error | Example |
|---|---|
| `index out of bounds` | `xs[10]` on a 3-element array |
| `index -1 is negative` | `xs[-1]` |
| `attempt to add with overflow` (also subtract, multiply) | `9223372036854775807 + 1` |
| `attempt to divide by zero` | `n / 0` with integers |
| `cannot convert "abc" to int` | `int("abc")` |
| `RefCell already borrowed` | see *Re-entrant objects* below |
| `key not found: plums` | `m["plums"]` when the map has no such key (use `m.get(k)`) |
| `field `value` was read before it was set` | a generic class's `T` field read before `init` set it |

Float division by zero is not an error; it produces `inf` or `NaN` as in most languages.

## Re-entrant objects

While a method runs on an object, Kalinga holds that object for exclusive use. Reaching
the same object again through a *different* variable, while that method is still
running, is reported instead of silently corrupting data:

```kalinga
class Account {
    balance: int = 100

    fn transfer_to(other: Account, amount: int) {
        self.balance -= amount
        other.balance += amount
    }
}

fn main() {
    let a = Account()
    let b = Account()
    a.transfer_to(b, 30)    // fine: two different objects
    print(a.balance, b.balance)
    a.transfer_to(a, 10)    // the same object twice: runtime error
}
```

```text
70 130
runtime error: RefCell already borrowed (an object was used while one of its own methods was running)
```

Inside a method, always reach the current object through `self`. If an operation may
involve the same object twice, check for it before calling, or restructure the method.

## Memory and reference cycles

Objects are freed automatically when nothing refers to them any more (reference counting).
Two objects that refer to each other — a child pointing at its parent and the parent
holding the child — keep each other alive, so that memory is only returned when the
program ends. This is safe (nothing is ever used after being freed), but long-running
programs that build and drop many such cycles will use more memory than necessary.

## Debugging tips

- Use `kalinga check file.kl` (or the VS Code extension) for instant feedback while
  editing.
- Add `print(...)` calls; it accepts any number of values of any printable type.
- Use `kalinga emit file.kl` to see the exact Rust code your program becomes.

---

Previous: [14. Modules](14-modules.md) · Next: [16. Tooling](16-tooling.md)
