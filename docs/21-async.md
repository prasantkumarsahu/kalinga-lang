# 21. Async

Threads (chapter 20) run work on other cores. **Async** functions are for the other kind of
waiting: timers, cut-scenes and scripted sequences, many slow jobs that spend most of their
time waiting. An `async fn` can pause with `await` and let other async jobs run in the
meantime, all on one thread, so they can use objects freely.

Kalinga has its async runtime built in: no libraries to choose or set up.

## async fn and await

```kalinga
async fn countdown(name: str, n: int) -> str {
    for i in 0..n {
        print(name, n - i)
        await Time.wait(0.01)
    }
    return name + " done"
}

async fn main() {
    let result = await countdown("launch", 3)
    print(result)
}
```

Output:

```text
launch 3
launch 2
launch 1
launch done
```

- Calling an `async fn` gives an `Async<T>`: a job that has not run yet. `await job` runs
  it until it finishes and gives its `T`.
- `await` can only be used inside an `async fn`. `main` can be `async fn main()`.
- `Time.wait(seconds)` is the async pause. (`Time.sleep` stops the whole thread, including
  every other async job.)

## Running jobs at the same time

`Async.all` waits for several jobs at once. While one is waiting, the others run, so three
waits of 0.15 s, 0.05 s and 0.1 s take about 0.15 s in total, not 0.3 s:

```kalinga
class Log {
    lines: [str]

    fn add(s: str) {
        self.lines.push(s)
    }
}

async fn delayed(log: Log, name: str, seconds: float) -> str {
    await Time.wait(seconds)
    log.add(name)
    return name
}

async fn main() {
    let log = Log()
    let jobs = [delayed(log, "slow", 0.15), delayed(log, "fast", 0.05), delayed(log, "medium", 0.1)]
    let t0 = Time.now()
    let names = await Async.all(jobs)
    print(names)
    print(log.lines)
    print("concurrent:", Time.now() - t0 < 0.25)
}
```

Output:

```text
[slow, fast, medium]
[fast, medium, slow]
concurrent: true
```

The results come back in the order of the jobs; the log shows the order they finished in.
Note that all three jobs used the same `Log` object: async jobs share one thread, so
objects are fine here (unlike threads).

## Background jobs

`Async.spawn(job)` starts a job right away and lets it run in the background; `await` it
later to get the result:

```kalinga
async fn load_level(n: int) -> str {
    await Time.wait(0.05)
    return "level {n}"
}

async fn main() {
    let loading = Async.spawn(load_level(2))
    print("showing the menu while level 2 loads")
    await Time.wait(0.1)
    print("loaded:", await loading)
}
```

Output:

```text
showing the menu while level 2 loads
loaded: level 2
```

## Waiting for a thread

Heavy computation still belongs on a thread. `await task` waits for a `Thread.spawn` task
without blocking the other async jobs:

```kalinga
fn heavy(n: int) -> int {
    var acc = 0
    for i in 0..n {
        acc = (acc + i) % 1000003
    }
    return acc
}

async fn tick(n: int) -> int {
    for i in 0..n {
        await Time.wait(0.001)
    }
    return n
}

async fn main() {
    let task = Thread.spawn(fn() => heavy(3000000))
    let ticker = Async.spawn(tick(5))
    let v = await task
    print("thread result ok:", v == heavy(3000000), "ticks:", await ticker)
}
```

Output:

```text
thread result ok: true ticks: 5
```

## Async from normal code

`Async.run(job)` runs a job to the end from a function that is not `async`:

```kalinga
async fn answer() -> int {
    await Time.wait(0.01)
    return 42
}

fn main() {
    print("got", Async.run(answer()))
}
```

Output:

```text
got 42
```

## Static async methods

Classes can have `static async fn`:

```kalinga
class Loader {
    static async fn load(id: int) -> int {
        await Time.wait(0.01)
        return id * 10
    }
}

async fn main() {
    print(await Loader.load(4))
}
```

Output:

```text
40
```

Instance methods cannot be `async` yet; write an `async fn` that takes the object instead
(like `delayed(log, ...)` above).

## Mistakes the compiler catches

Calling an `async fn` without `await` does nothing, so it is an error:

```kalinga error
async fn job() -> int {
    return 1
}

async fn main() {
    job()
}
```

```text
main.kl:6:5: error: calling an `async fn` only prepares it: put `await` in front, or start it with `Async.spawn(...)`
```

```kalinga error
async fn job() -> int {
    return 1
}

fn main() {
    let x = await job()
}
```

```text
main.kl:6:13: error: `await` can only be used inside an `async fn` (not inside a lambda)
```

## Library summary

| Call | Gives | Meaning |
|---|---|---|
| `await job` | `T` | runs an `Async<T>` (or waits for a `Task<T>`) |
| `Time.wait(s)` | `Async<void>` | pause without blocking other jobs |
| `Async.spawn(job)` | `Async<T>` | start in the background |
| `Async.all([jobs])` | `Async<[T]>` | wait for all; results in order |
| `Async.run(job)` | `T` | run from non-async code |

## Async or threads?

| Work | Use |
|---|---|
| Waiting: timers, sequences, many slow jobs | async |
| Computing: path finding, generation, number crunching | threads (`Thread.spawn`, `par_map`) |
| Both | spawn the computation on a thread and `await` the task |

In a game made with `std/game` (chapter 22), `update` and `draw` are plain functions
called every frame; use threads for heavy work there.

---

Previous: [20. Threads](20-threads.md) · Next: [22. Making a Game](22-making-a-game.md)
