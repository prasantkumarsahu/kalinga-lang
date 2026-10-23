# 22. Making a Game

Kalinga comes with `std/game`: a window, 2D drawing, keyboard and mouse input, images and
sounds. It is built on the Rust library [macroquad](https://macroquad.rs), so games run at
native speed: drawing 10,000 sprites takes about as long per frame as the same program
written directly in Rust.

## A first game

```kalinga
import "std/game"

class Collector : Game {
    player: Vec2 = Vec2(400.0, 300.0)
    star: Vec2 = Vec2(100.0, 100.0)
    score: int

    fn update(input: Input, dt: float) {
        let speed = 300.0
        var dir = Vec2.zero()
        if input.key_down("left") { dir.x -= 1.0 }
        if input.key_down("right") { dir.x += 1.0 }
        if input.key_down("up") { dir.y -= 1.0 }
        if input.key_down("down") { dir.y += 1.0 }
        self.player = self.player + dir * speed * dt

        if self.player.distance(self.star) < 30.0 {
            self.score += 1
            self.star = Vec2(Random.float() * 760.0 + 20.0, Random.float() * 560.0 + 20.0)
        }
        if input.key_pressed("escape") {
            input.quit()
        }
    }

    fn draw(screen: Screen) {
        screen.clear(Color.rgb(15, 20, 35))
        screen.circle(self.star, 10.0, Color.YELLOW)
        screen.circle(self.player, 20.0, Color.SKY)
        screen.text("score: {self.score}", Vec2(20.0, 40.0), 32.0, Color.WHITE)
    }
}

fn main() {
    Window.run("Collector", 800, 600, Collector())
}
```

```bash
kalinga run collector.kl
```

The first build downloads and compiles macroquad (a minute or two, once); later builds take
seconds. `kalinga build collector.kl` makes a standalone executable you can give to
anyone: on Windows it needs no extra runtime files.

## How a game runs

A game is a class that implements the `Game` interface:

```kalinga
interface Game {
    fn update(input: Input, dt: float)
    fn draw(screen: Screen)
}
```

`Window.run(title, width, height, game)` opens the window and then, every frame (usually 60
times per second):

1. calls `update(input, dt)`: move things, check collisions. `dt` is the time since the
   last frame in seconds; multiply speeds by it so the game runs at the same speed on
   every computer.
2. calls `draw(screen)`: draw everything, back to front.

It returns when the window is closed or `input.quit()` is called.

## Vectors and rectangles

`std/game` includes `std/math2d`, two structs with operators (chapter 14):

| `Vec2` | |
|---|---|
| `Vec2(x, y)`, `Vec2.zero()`, `Vec2.from_angle(radians, length)` | create |
| `a + b`, `a - b`, `v * k`, `k * v`, `v / k`, `-v` | arithmetic |
| `length()`, `distance(o)`, `dot(o)`, `normalized()`, `angle()`, `lerp(to, t)` | geometry |

| `Rect` | |
|---|---|
| `Rect(x, y, w, h)` | top-left corner and size |
| `center()`, `contains(point)`, `overlaps(other)`, `moved(by)` | helpers |

They are value types: `var p = player.pos` copies, and arrays of them are stored densely,
which is what makes thousands of particles cheap.

## Drawing

| `screen.` | Draws |
|---|---|
| `clear(color)` | fills the window |
| `rect(r, color)`, `rect_outline(r, thickness, color)` | rectangles |
| `circle(center, radius, color)`, `circle_outline(center, radius, thickness, color)` | circles |
| `line(from, to, thickness, color)`, `triangle(a, b, c, color)` | lines and triangles |
| `text(s, pos, size, color)` | text; `pos` is the start of the baseline |
| `text_width(s, size)` | width in pixels, for centring |
| `image(img, pos)`, `image_in(img, dest_rect, rotation)` | images |
| `width()`, `height()`, `size()`, `fps()` | window size, frames per second |
| `screenshot(path)` | saves a PNG of what was drawn so far |

Coordinates are pixels from the top-left corner; `y` grows downwards. Colours are `Color(r, g,
b)` or `Color(r, g, b, a)` with values from 0.0 to 1.0, `Color.rgb(255, 128, 0)`, or one of
`Color.WHITE`, `BLACK`, `GRAY`, `DARK_GRAY`, `RED`, `GREEN`, `BLUE`, `YELLOW`, `ORANGE`,
`PURPLE`, `SKY`, `CLEAR`; `c.with_alpha(0.5)` makes one see-through.

## Input

| `input.` | |
|---|---|
| `key_down(name)` | is the key held down? (movement) |
| `key_pressed(name)` | was it pressed this frame? (jump, shoot, menus) |
| `mouse()` | the mouse position as a `Vec2` |
| `mouse_down(button)`, `mouse_pressed(button)` | `"left"`, `"right"`, `"middle"` |
| `quit()` | close the window after this frame |

Key names: `"a"`…`"z"`, `"0"`…`"9"`, `"left"`, `"right"`, `"up"`, `"down"`, `"space"`,
`"enter"`, `"escape"`, `"shift"`, `"ctrl"`, `"tab"`, `"backspace"`.

## Images and sounds

Loading can fail (a missing or broken file), so `load` returns a `Result` (chapter 16):

```kalinga
import "std/game"

class Shooter : Game {
    ship: Image?
    laser: Sound?
    pos: Vec2 = Vec2(400.0, 500.0)

    init() {
        match Image.load("assets/ship.png") {
            Ok(img) => self.ship = img
            Err(e) => print("no ship image:", e)
        }
        match Sound.load("assets/laser.wav") {
            Ok(s) => self.laser = s
            Err(e) => print("no laser sound:", e)
        }
    }

    fn update(input: Input, dt: float) {
        if input.key_pressed("space") {
            if let s = self.laser {
                s.play()
            }
        }
    }

    fn draw(screen: Screen) {
        screen.clear(Color.BLACK)
        if let img = self.ship {
            screen.image(img, self.pos)
        } else {
            screen.triangle(self.pos, self.pos + Vec2(-15.0, 30.0), self.pos + Vec2(15.0, 30.0), Color.WHITE)
        }
    }
}

fn main() {
    Window.run("Shooter", 800, 600, Shooter())
}
```

- Images: PNG or JPEG. `img.width()` and `img.height()` give the size.
- Sounds: WAV or OGG. `play()`, `play_looped(volume)` for music, `stop()`.
- Paths are relative to the folder the game is started from.

## Structuring a bigger game

Keep the rules in plain functions and structs, separate from drawing, so they can be tested
without a window. The Pong example does this:

```text
examples/pong/
├── main.kl     fn main() { Window.run("Kalinga Pong", 800, 600, Pong()) }
├── pong.kl     class Pong : Game: input, scores, drawing
└── logic.kl    struct Ball, move_ball, bounce_off, follow, serve: pure game rules
```

`tests/game/pong_logic.kl` checks the rules (does the ball bounce off the top wall? does the
computer paddle stay on screen?) with ordinary `print`s, and runs in the normal test suite.

There is also `examples/asteroids/`: a ship with thrust and wrap-around, bullets with a
lifetime, and rocks that split, in about 250 lines.

```bash
kalinga run examples/pong/main.kl
kalinga run examples/asteroids/main.kl
```

## Speed tips

- Use `struct`s for things you have many of (particles, bullets, tiles): they are stored
  inline and copied cheaply.
- Do heavy work (path finding, level generation) on a thread (chapter 20) and pick up the
  result in a later frame with `task.is_done()` and `task.join()`.
- Keep `draw` for drawing only.

## As a utkal project

A game with packages and Rust crates is easiest as a utkal project (chapter 18):

```bash
utkal new my-game
cd my-game
utkal run
```

`import "std/game"` works the same inside a project.

---

Previous: [21. Async](21-async.md) · Next: [23. Safety and Errors](23-safety-and-errors.md)
