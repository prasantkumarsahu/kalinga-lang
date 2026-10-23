// Random: xorshift64*, seeded from the clock (fast, not for cryptography).
thread_local! {
    static __RNG: std::cell::Cell<u64> = std::cell::Cell::new({
        use std::hash::{Hash, Hasher};
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::thread::current().id().hash(&mut h);
        (t ^ h.finish() ^ 0x9E37_79B9_7F4A_7C15) | 1
    });
}
fn __rand_u64() -> u64 {
    __RNG.with(|s| {
        let mut x = s.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}
fn __rand_seed(n: i64) {
    __RNG.with(|s| s.set(((n as u64) ^ 0x9E37_79B9_7F4A_7C15) | 1));
}
fn __rand_int(lo: i64, hi: i64) -> i64 {
    if hi <= lo {
        panic!("Random.int({}, {}): the second number must be greater than the first", lo, hi)
    }
    let span = (hi as i128 - lo as i128) as u128;
    (lo as i128 + (__rand_u64() as u128 % span) as i128) as i64
}
fn __rand_float() -> f64 {
    (__rand_u64() >> 11) as f64 / (1u64 << 53) as f64
}
fn __shuffle<T>(xs: &mut [T]) {
    for i in (1..xs.len()).rev() {
        let j = __rand_int(0, i as i64 + 1) as usize;
        xs.swap(i, j);
    }
}
