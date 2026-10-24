fn __kl_run_main(f: fn()) {
    __START.get_or_init(std::time::Instant::now);
    __kl_panic_hook();
    if std::panic::catch_unwind(f).is_err() {
        std::process::exit(101);
    }
}
fn __kl_run(f: fn()) {
    __START.get_or_init(std::time::Instant::now);
    __kl_panic_hook();
    let worker = std::thread::Builder::new().stack_size(256 * 1024 * 1024).spawn(f).expect("failed to start");
    if worker.join().is_err() {
        std::process::exit(101);
    }
}
fn __kl_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("runtime error: {}", __kl_panic_message(info.payload()));
    }));
}
/// What a runtime error says.
fn __kl_panic_message(p: &(dyn std::any::Any + Send)) -> String {
    let msg = if let Some(s) = p.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown error".to_string()
    };
    if msg.contains("already borrowed") || msg.contains("already mutably borrowed") {
        format!("{} (an object was used while one of its own methods was running)", msg)
    } else {
        msg
    }
}
/// A failed `assert` / `assert_eq`.
fn __kl_fail(msg: String) {
    std::panic::panic_any(msg);
}
/// `kalinga test`: runs each test on a fresh thread (so `static var`s start over), and
/// reports like `cargo test`. Exits with 1 when a test failed.
fn __kl_run_tests(tests: &[(&str, fn())]) {
    __START.get_or_init(std::time::Instant::now);
    std::panic::set_hook(Box::new(|_| {}));
    println!("running {} test{}", tests.len(), if tests.len() == 1 { "" } else { "s" });
    let mut failures: Vec<(&str, String)> = vec![];
    for (name, f) in tests {
        let run = std::thread::Builder::new().stack_size(256 * 1024 * 1024).spawn(*f).expect("failed to start");
        match run.join() {
            Ok(()) => println!("test {} ... ok", name),
            Err(p) => {
                println!("test {} ... FAILED", name);
                failures.push((name, __kl_panic_message(&*p)));
            }
        }
    }
    if !failures.is_empty() {
        println!("\nfailures:");
        for (name, msg) in &failures {
            println!("    {}: {}", name, msg);
        }
    }
    let passed = tests.len() - failures.len();
    let result = if failures.is_empty() { "ok" } else { "FAILED" };
    println!("\ntest result: {}. {} passed; {} failed", result, passed, failures.len());
    if !failures.is_empty() {
        std::process::exit(1);
    }
}
