// Time
static __START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
fn __time_now() -> f64 {
    __START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}
fn __time_millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
fn __time_sleep(seconds: f64) {
    if seconds > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(seconds))
    }
}
