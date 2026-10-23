fn __unset<T>(v: Option<T>, name: &str) -> T {
    match v {
        Some(x) => x,
        None => panic!("field `{}` was read before it was set", name),
    }
}
fn __parse_int(s: &str) -> i64 {
    match s.trim().parse::<i64>() {
        Ok(v) => v,
        Err(_) => panic!("cannot convert \"{}\" to int", s),
    }
}
fn __parse_float(s: &str) -> f64 {
    match s.trim().parse::<f64>() {
        Ok(v) => v,
        Err(_) => panic!("cannot convert \"{}\" to float", s),
    }
}
/// `print`: no newline, so the text is flushed to appear at once.
fn __print(text: String) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}
fn __input(prompt: &str) -> String {
    use std::io::Write;
    print!("{}", prompt);
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).ok();
    s.trim_end_matches(['\r', '\n']).to_string()
}
