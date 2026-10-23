// Hand-written Rust used by main.kl.
use std::collections::BTreeMap;

/// An opaque type: Kalinga can store and pass it, but only Rust looks inside.
#[derive(Clone)]
pub struct Histogram {
    buckets: BTreeMap<i64, i64>,
}

pub fn histogram_new() -> Histogram {
    Histogram { buckets: BTreeMap::new() }
}

pub fn histogram_add(mut h: Histogram, value: f64, width: f64) -> Histogram {
    *h.buckets.entry((value / width).floor() as i64).or_insert(0) += 1;
    h
}

pub fn histogram_render(h: Histogram, width: f64) -> String {
    h.buckets
        .iter()
        .map(|(b, n)| format!("{:>5.1}: {}", *b as f64 * width, "#".repeat(*n as usize)))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn sum_squares(xs: Vec<f64>) -> f64 {
    xs.iter().map(|x| x * x).sum()
}

pub fn first_word(s: String) -> Option<String> {
    s.split_whitespace().next().map(|w| w.to_string())
}

/// Kalinga structs are ordinary Rust structs in the same program.
pub fn length(v: crate::Vec2) -> f64 {
    (v.x * v.x + v.y * v.y).sqrt()
}
