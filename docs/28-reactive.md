# 28. Reactive Values

`std/reactive` keeps values that depend on each other up to date: change one, and what is
worked out from it, and the code that shows it, follow. It is how a user interface stays in
step with its data, and it needs no garbage collector.

```kalinga
import "std/reactive"

fn main() {
    let count = signal(1)
    let doubled = computed(fn() => count.get() * 2)
    effect(fn() {
        println("count is {count.get()}, doubled is {doubled.get()}")
    })
    count.set(5)
    batch(fn() {
        count.set(6)
        count.set(7)
    })
}
```

```text
count is 1, doubled is 2
count is 5, doubled is 10
count is 7, doubled is 14
```

## Signals, computed values and effects

| Make it | Type | Use it |
|---|---|---|
| `signal(value)` | `Signal<T>` | `get()`, `set(v)`, `update(fn(old) => new)`, `peek()` |
| `computed(fn() => ...)` | `Computed<T>` | `get()` |
| `effect(fn() { ... })` | `Effect` | `dispose()` |
| `scope()` | `Scope` | `run(fn() { ... })`, `dispose()` |
| `batch(fn() { ... })` | — | — |

- A **signal** holds a value. `get()` reads it; `set(v)` and `update(f)` change it.
- A **computed** value is worked out by its function from signals and other computed
  values. It is worked out when first read, and again only after something it read has
  changed, so reading it often is cheap.
- An **effect** runs its code at once, then again whenever a value it read changes.
- What counts is what was **read the last time** the code ran: an effect that reads `a`
  only when `b` is true starts or stops depending on `a` as `b` changes.
- `peek()` reads a signal without depending on it.
- Each effect runs at most once for each change, even when it depends on a value in
  several ways (for example through two computed values made from the same signal).
- Inside `batch`, effects wait until the batch ends, then each runs once with the final
  values.

## Handles, scopes and memory

Signals, computed values, effects and scopes are small handles: copying one, or capturing it
in a lambda, gives the same signal, not a new one. Their values and links live in one graph.

There is no garbage collector to find values nothing uses any more, so they are freed in
groups. A **scope** owns everything made while its `run` is running; `dispose()` frees all
of it at once, which suits a screen or a part of the user interface that goes away:

```kalinga
import "std/reactive"

fn main() {
    let ticks = signal(0)
    let page = scope()
    page.run(fn() {
        effect(fn() {
            println("page sees", ticks.get())
        })
    })
    ticks.set(1)
    page.dispose()
    ticks.set(2)
    println("done")
}
```

```text
page sees 0
page sees 1
done
```

- After `dispose()`, the scope's effects never run again, and reading or changing one of
  its signals or computed values stops the program with
  `this reactive value was disposed (its scope has ended)`.
- `effect(...)` returns an `Effect`; its `dispose()` stops just that effect.
- Values made outside any scope live as long as the program.

## Limits in this version

- Everything runs on one thread; signals cannot be shared with `Thread.spawn`.
- There is no syntax of its own yet: reactive values are ordinary function calls and
  methods.

---

Previous: [27. Stability](27-stability.md)
