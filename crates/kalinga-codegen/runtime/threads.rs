// Threads: share-nothing tasks, channels and shared cells.
fn __cores() -> i64 {
    std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1)
}
struct __Task<T> {
    handle: Rc<RefCell<Option<std::thread::JoinHandle<T>>>>,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    waker: std::sync::Arc<std::sync::Mutex<Option<std::task::Waker>>>,
}
impl<T> Clone for __Task<T> {
    fn clone(&self) -> Self {
        __Task { handle: self.handle.clone(), done: self.done.clone(), waker: self.waker.clone() }
    }
}
impl<T: Send + 'static> __Task<T> {
    fn spawn<F: FnOnce() -> T + Send + 'static>(f: F) -> __Task<T> {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waker: std::sync::Arc<std::sync::Mutex<Option<std::task::Waker>>> = Default::default();
        let (d, w) = (done.clone(), waker.clone());
        let h = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                struct Finish(
                    std::sync::Arc<std::sync::atomic::AtomicBool>,
                    std::sync::Arc<std::sync::Mutex<Option<std::task::Waker>>>,
                );
                impl Drop for Finish {
                    fn drop(&mut self) {
                        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                        if let Some(w) = self.1.lock().unwrap_or_else(|e| e.into_inner()).take() {
                            w.wake();
                        }
                    }
                }
                let _finish = Finish(d, w);
                f()
            })
            .expect("cannot start a thread");
        __Task { handle: Rc::new(RefCell::new(Some(h))), done, waker }
    }
    fn join(&self) -> T {
        let h = self.handle.borrow_mut().take().unwrap_or_else(|| panic!("this task was already joined"));
        match h.join() {
            Ok(v) => v,
            Err(_) => panic!("a thread stopped with an error (see above)"),
        }
    }
    fn is_done(&self) -> bool {
        self.done.load(std::sync::atomic::Ordering::SeqCst)
    }
}
