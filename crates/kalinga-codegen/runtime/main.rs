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
        let p = info.payload();
        let msg = if let Some(s) = p.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = p.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown error".to_string()
        };
        let msg = if msg.contains("already borrowed") || msg.contains("already mutably borrowed") {
            format!("{} (an object was used while one of its own methods was running)", msg)
        } else {
            msg
        };
        eprintln!("runtime error: {}", msg);
    }));
}
