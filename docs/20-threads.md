# 20. Threads

A thread runs code at the same time as the rest of the program, on another CPU core. Games
use threads for path finding, procedural generation and loading; AI code uses them to
spread number crunching over every core.

Kalinga threads **share nothing**. Only *values* move between threads: numbers, strings,
arrays, maps, structs and enums made of values. Objects (classes) always stay on the
thread that created them. This rules out data races at compile time, and it keeps
single-threaded code exactly as fast as before, because objects never need locks.

## Spawning a thread

`Thread.spawn` runs a lambda on a new thread and gives back a `Task<T>`. `join()` waits for
it and returns the lambda's result:

```kalinga
fn work(n: int) -> int {
    var acc = 0
    for i in 0..n {
        acc = (acc + i * i) % 1000003
    }
    return acc
}

fn main() {
    let a = Thread.spawn(fn() => work(1000000))
    let b = Thread.spawn(fn() => work(2000000))
    // both run at the same time, on two cores
    println(a.join() == work(1000000), b.join() == work(2000000))
}
```

Output:

```text
true true
```

- The lambda gets a **copy** of every outside variable it uses, so the thread cannot
  change the caller's variables. (A `var` that some other lambda changes is shared, not
  copied, so a thread cannot use it; share state with threads through a `Shared`.)
- `task.is_done()` tells whether the thread has finished, without waiting.
- If the thread stops with a runtime error, `join()` stops the program with that error.

## What can cross threads

| Can move to another thread | Cannot |
|---|---|
| `int`, `float`, `bool`, `str` | objects of classes |
| arrays, maps, optionals and `Result`s of those | interfaces |
| structs and enums whose fields can | function values (except the lambda given to `spawn`) |
| `Channel<T>`, `Shared<T>`, `extern type`s | |

Using an object inside a thread lambda is a compile error that names it:

```kalinga error
class Player {
    hp: int = 10
}

fn main() {
    let p = Player()
    let t = Thread.spawn(fn() => p.hp)
}
```

```text
main.kl:7:26: error: `Thread.spawn` cannot use `p` here: a `Player` cannot move to another thread (pass plain values, a `Channel` or a `Shared`)
```

Copy what the thread needs into a value first (`let hp = p.hp`), or describe the data with a
`struct`.

## Channels

A `Channel<T>` is a queue that threads use to send values to each other. Any number of
threads can send and receive; a value goes to exactly one receiver.

```kalinga
struct Job {
    id: int
    size: int
}

fn main() {
    let jobs = Channel<Job>()
    let results = Channel<int>()
    let worker = Thread.spawn(fn() -> int {
        var count = 0
        while true {
            if let j = jobs.receive() {
                results.send(j.id * j.size)
                count += 1
            } else {
                return count
            }
        }
    })
    for i in 1..6 {
        jobs.send(Job(i, 10))
    }
    jobs.close()
    println("worker handled", worker.join(), "jobs")

    var total = 0
    while true {
        if let v = results.try_receive() {
            total += v
        } else {
            break
        }
    }
    println("total", total)
}
```

Output:

```text
worker handled 5 jobs
total 150
```

| Method | Does |
|---|---|
| `ch.send(v)` | puts `v` in the queue |
| `ch.receive()` | waits for a value; returns `none` once the channel is closed and empty |
| `ch.try_receive()` | a value if one is waiting, otherwise `none` straight away |
| `ch.close()` | no more values will be sent; waiting receivers get `none` |

A channel variable can be copied into many threads: all copies are the same channel.

## Shared values

`Shared<T>` holds one value that several threads can read and change. `update` changes it
with no other thread in between, so increments are never lost:

```kalinga
fn main() {
    let counter = Shared(0)
    var tasks: [Task<void>] = []
    for i in 0..8 {
        tasks.push(Thread.spawn(fn() {
            for k in 0..1000 {
                counter.update(fn(c) => c + 1)
            }
        }))
    }
    for t in tasks {
        t.join()
    }
    println("counter", counter.get())
}
```

Output:

```text
counter 8000
```

| Method | Does |
|---|---|
| `Shared(v)` / `Shared<T>(v)` | a new shared value |
| `s.get()` | a copy of the value |
| `s.set(v)` | replaces it |
| `s.update(fn(old) => new)` | replaces it with a function of the old value, atomically |

Prefer channels for passing work around, and `Shared` for a few counters or settings.

## par_map: every core at once

`xs.par_map(f)` is `xs.map(f)` split over all CPU cores. The result is the same as `map`,
in the same order:

```kalinga
fn cost(n: int) -> int {
    var acc = 0
    for i in 0..n {
        acc = (acc + i) % 1000003
    }
    return acc
}

fn main() {
    let sizes = [300000, 300000, 300000, 300000, 300000, 300000]
    let a = sizes.map(fn(n) => cost(n))
    let b = sizes.par_map(fn(n) => cost(n))
    println(a == b, Thread.cores() >= 1)
}
```

Output:

```text
true true
```

On a 12-core machine this runs about 5x faster than `map` for heavy work. For tiny work per
element, plain `map` is faster: starting threads costs a little.

The elements, the result and everything the lambda uses must be able to cross threads, as
with `Thread.spawn`.

## Rules worth knowing

- Lambdas get read-only copies of outside variables, so a thread cannot change the
  caller's `var`s; send results back through `join()`, a `Channel` or a `Shared`.
- `Thread.spawn` needs the lambda written right there, `Thread.spawn(fn() => ...)`, so the
  compiler can check what it captures.
- Each thread has its own `Random` numbers and the same big stack as the main program.
- Rust's own checks run on the generated code too: an `extern type` that is not safe to
  send between threads is rejected when the program is built.

---

Previous: [19. Rust Interop](19-rust-interop.md) · Next: [21. Async](21-async.md)
