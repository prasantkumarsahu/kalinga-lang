// Rust side of `std/game`: connects Kalinga's Game interface to macroquad.
use macroquad::prelude as mq;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

thread_local! {
    static QUIT: Cell<bool> = const { Cell::new(false) };
    static SCREENSHOTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SOUNDS_TO_LOAD: RefCell<Vec<GkSound>> = const { RefCell::new(Vec::new()) };
}

fn color(c: &crate::Color) -> mq::Color {
    mq::Color::new(c.r as f32, c.g as f32, c.b as f32, c.a as f32)
}

pub fn gk_run(title: String, width: i64, height: i64, game: Rc<RefCell<dyn crate::Game>>) {
    let conf = mq::Conf { window_title: title, window_width: width as i32, window_height: height as i32, ..Default::default() };
    macroquad::Window::from_config(conf, async move {
        let input = crate::Input::__new();
        let screen = crate::Screen::__new();
        loop {
            // Sounds decode asynchronously, so they are loaded here, inside the frame loop.
            let pending: Vec<GkSound> = SOUNDS_TO_LOAD.with(|s| std::mem::take(&mut *s.borrow_mut()));
            for s in pending {
                if let Ok(snd) = macroquad::audio::load_sound_from_bytes(&s.bytes).await {
                    *s.sound.borrow_mut() = Some(snd);
                    if let Some((looped, volume)) = s.play_when_loaded.take() {
                        gk_sound_play(s.clone(), looped, volume as f64);
                    }
                }
            }
            let dt = mq::get_frame_time() as f64;
            crate::Game::update(&mut *game.borrow_mut(), input.clone(), dt);
            crate::Game::draw(&mut *game.borrow_mut(), screen.clone());
            for path in SCREENSHOTS.with(|s| std::mem::take(&mut *s.borrow_mut())) {
                mq::get_screen_data().export_png(&path);
            }
            if QUIT.with(|q| q.get()) {
                QUIT.with(|q| q.set(false));
                break;
            }
            mq::next_frame().await;
        }
    });
}

fn key(name: &str) -> mq::KeyCode {
    use mq::KeyCode as K;
    match name.to_lowercase().as_str() {
        "a" => K::A, "b" => K::B, "c" => K::C, "d" => K::D, "e" => K::E, "f" => K::F, "g" => K::G,
        "h" => K::H, "i" => K::I, "j" => K::J, "k" => K::K, "l" => K::L, "m" => K::M, "n" => K::N,
        "o" => K::O, "p" => K::P, "q" => K::Q, "r" => K::R, "s" => K::S, "t" => K::T, "u" => K::U,
        "v" => K::V, "w" => K::W, "x" => K::X, "y" => K::Y, "z" => K::Z,
        "0" => K::Key0, "1" => K::Key1, "2" => K::Key2, "3" => K::Key3, "4" => K::Key4,
        "5" => K::Key5, "6" => K::Key6, "7" => K::Key7, "8" => K::Key8, "9" => K::Key9,
        "left" => K::Left, "right" => K::Right, "up" => K::Up, "down" => K::Down,
        "space" => K::Space, "enter" => K::Enter, "escape" => K::Escape, "tab" => K::Tab,
        "shift" => K::LeftShift, "ctrl" => K::LeftControl, "backspace" => K::Backspace,
        other => panic!("unknown key name \"{}\" (use letters, digits, arrows, space, enter, escape, shift, ctrl, tab, backspace)", other),
    }
}

fn button(name: &str) -> mq::MouseButton {
    match name.to_lowercase().as_str() {
        "left" => mq::MouseButton::Left,
        "right" => mq::MouseButton::Right,
        "middle" => mq::MouseButton::Middle,
        other => panic!("unknown mouse button \"{}\" (use left, right or middle)", other),
    }
}

pub fn gk_key_down(k: String) -> bool { mq::is_key_down(key(&k)) }
pub fn gk_key_pressed(k: String) -> bool { mq::is_key_pressed(key(&k)) }
pub fn gk_mouse_x() -> f64 { mq::mouse_position().0 as f64 }
pub fn gk_mouse_y() -> f64 { mq::mouse_position().1 as f64 }
pub fn gk_mouse_down(b: String) -> bool { mq::is_mouse_button_down(button(&b)) }
pub fn gk_mouse_pressed(b: String) -> bool { mq::is_mouse_button_pressed(button(&b)) }
pub fn gk_quit() { QUIT.with(|q| q.set(true)) }

pub fn gk_screen_width() -> f64 { mq::screen_width() as f64 }
pub fn gk_screen_height() -> f64 { mq::screen_height() as f64 }
pub fn gk_clear(c: crate::Color) { mq::clear_background(color(&c)) }
pub fn gk_rect(r: crate::Rect, c: crate::Color) {
    mq::draw_rectangle(r.x as f32, r.y as f32, r.w as f32, r.h as f32, color(&c))
}
pub fn gk_rect_outline(r: crate::Rect, thickness: f64, c: crate::Color) {
    mq::draw_rectangle_lines(r.x as f32, r.y as f32, r.w as f32, r.h as f32, thickness as f32, color(&c))
}
pub fn gk_circle(p: crate::Vec2, radius: f64, c: crate::Color) {
    mq::draw_circle(p.x as f32, p.y as f32, radius as f32, color(&c))
}
pub fn gk_circle_outline(p: crate::Vec2, radius: f64, thickness: f64, c: crate::Color) {
    mq::draw_circle_lines(p.x as f32, p.y as f32, radius as f32, thickness as f32, color(&c))
}
pub fn gk_line(a: crate::Vec2, b: crate::Vec2, thickness: f64, c: crate::Color) {
    mq::draw_line(a.x as f32, a.y as f32, b.x as f32, b.y as f32, thickness as f32, color(&c))
}
pub fn gk_triangle(a: crate::Vec2, b: crate::Vec2, c: crate::Vec2, col: crate::Color) {
    mq::draw_triangle(
        mq::vec2(a.x as f32, a.y as f32),
        mq::vec2(b.x as f32, b.y as f32),
        mq::vec2(c.x as f32, c.y as f32),
        color(&col),
    )
}
pub fn gk_text(s: String, p: crate::Vec2, size: f64, c: crate::Color) {
    mq::draw_text(&s, p.x as f32, p.y as f32, size as f32, color(&c));
}
pub fn gk_text_width(s: String, size: f64) -> f64 {
    mq::measure_text(&s, None, size as u16, 1.0).width as f64
}
pub fn gk_fps() -> i64 { mq::get_fps() as i64 }
pub fn gk_screenshot(path: String) { SCREENSHOTS.with(|s| s.borrow_mut().push(path)) }

/// An image decoded on load; it becomes a GPU texture the first time it is drawn.
#[derive(Clone)]
pub struct GkImage {
    image: Rc<mq::Image>,
    texture: Rc<RefCell<Option<mq::Texture2D>>>,
}

pub fn gk_image_load(path: String) -> Result<GkImage, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read image {}: {}", path, e))?;
    let image = mq::Image::from_file_with_format(&bytes, None).map_err(|e| format!("cannot decode image {}: {}", path, e))?;
    Ok(GkImage { image: Rc::new(image), texture: Rc::new(RefCell::new(None)) })
}
pub fn gk_image_width(i: GkImage) -> f64 { i.image.width() as f64 }
pub fn gk_image_height(i: GkImage) -> f64 { i.image.height() as f64 }
pub fn gk_image(i: GkImage, dest: crate::Rect, rotation: f64) {
    let mut slot = i.texture.borrow_mut();
    let tex = slot.get_or_insert_with(|| mq::Texture2D::from_image(&i.image));
    mq::draw_texture_ex(
        tex,
        dest.x as f32,
        dest.y as f32,
        mq::WHITE,
        mq::DrawTextureParams { dest_size: Some(mq::vec2(dest.w as f32, dest.h as f32)), rotation: rotation as f32, ..Default::default() },
    );
}

/// A sound file; it is decoded inside the frame loop and can be played before that.
#[derive(Clone)]
pub struct GkSound {
    bytes: Rc<Vec<u8>>,
    sound: Rc<RefCell<Option<macroquad::audio::Sound>>>,
    play_when_loaded: Rc<Cell<Option<(bool, f32)>>>,
}

pub fn gk_sound_load(path: String) -> Result<GkSound, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read sound {}: {}", path, e))?;
    let s = GkSound { bytes: Rc::new(bytes), sound: Rc::new(RefCell::new(None)), play_when_loaded: Rc::new(Cell::new(None)) };
    SOUNDS_TO_LOAD.with(|q| q.borrow_mut().push(s.clone()));
    Ok(s)
}
pub fn gk_sound_play(s: GkSound, looped: bool, volume: f64) {
    match &*s.sound.borrow() {
        Some(snd) => macroquad::audio::play_sound(snd, macroquad::audio::PlaySoundParams { looped, volume: volume as f32 }),
        None => s.play_when_loaded.set(Some((looped, volume as f32))),
    }
}
pub fn gk_sound_stop(s: GkSound) {
    if let Some(snd) = &*s.sound.borrow() {
        macroquad::audio::stop_sound(snd);
    }
}
