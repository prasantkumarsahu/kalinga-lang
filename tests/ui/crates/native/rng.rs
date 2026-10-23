// Glue between Kalinga and the `fastrand` crate.
pub fn rng_between(lo: i64, hi: i64) -> i64 {
    fastrand::i64(lo..hi)
}
