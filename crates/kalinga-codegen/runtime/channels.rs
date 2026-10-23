struct __ChanState<T> {
    queue: std::collections::VecDeque<T>,
    closed: bool,
}
struct __Channel<T> {
    inner: std::sync::Arc<(std::sync::Mutex<__ChanState<T>>, std::sync::Condvar)>,
}
impl<T> Clone for __Channel<T> {
    fn clone(&self) -> Self {
        __Channel { inner: self.inner.clone() }
    }
}
impl<T> __Channel<T> {
    fn new() -> Self {
        __Channel {
            inner: std::sync::Arc::new((
                std::sync::Mutex::new(__ChanState { queue: Default::default(), closed: false }),
                std::sync::Condvar::new(),
            )),
        }
    }
    fn send(&self, v: T) {
        let mut st = self.inner.0.lock().unwrap();
        if st.closed {
            panic!("send on a closed channel")
        }
        st.queue.push_back(v);
        self.inner.1.notify_one();
    }
    fn receive(&self) -> Option<T> {
        let mut st = self.inner.0.lock().unwrap();
        loop {
            if let Some(v) = st.queue.pop_front() {
                return Some(v);
            }
            if st.closed {
                return None;
            }
            st = self.inner.1.wait(st).unwrap();
        }
    }
    fn try_receive(&self) -> Option<T> {
        self.inner.0.lock().unwrap().queue.pop_front()
    }
    fn close(&self) {
        self.inner.0.lock().unwrap().closed = true;
        self.inner.1.notify_all();
    }
}
struct __Shared<T> {
    inner: std::sync::Arc<std::sync::Mutex<T>>,
}
impl<T> Clone for __Shared<T> {
    fn clone(&self) -> Self {
        __Shared { inner: self.inner.clone() }
    }
}
impl<T: Clone> __Shared<T> {
    fn new(v: T) -> Self {
        __Shared { inner: std::sync::Arc::new(std::sync::Mutex::new(v)) }
    }
    fn get(&self) -> T {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn set(&self, v: T) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = v;
    }
    fn update(&self, f: impl FnOnce(T) -> T) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let v = (*g).clone();
        *g = f(v);
    }
}
fn __par_map<T: Clone + Sync, U: Send, F: Fn(T) -> U + Sync>(xs: &[T], f: &F) -> Vec<U> {
    let workers = (__cores() as usize).max(1);
    let chunk = xs.len().div_ceil(workers).max(1);
    std::thread::scope(|s| {
        let parts: Vec<_> =
            xs.chunks(chunk).map(|c| s.spawn(move || c.iter().map(|x| f(x.clone())).collect::<Vec<U>>())).collect();
        parts
            .into_iter()
            .flat_map(|p| p.join().unwrap_or_else(|_| panic!("a par_map worker stopped with an error (see above)")))
            .collect()
    })
}
