// Async: a small single-threaded executor (ready queue + timers + wakers).
type __Fut<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T>>>;
struct __Async<T> {
    fut: Rc<RefCell<Option<__Fut<T>>>>,
}
impl<T> Clone for __Async<T> {
    fn clone(&self) -> Self {
        __Async { fut: self.fut.clone() }
    }
}
impl<T: 'static> __Async<T> {
    fn new(f: impl std::future::Future<Output = T> + 'static) -> Self {
        __Async { fut: Rc::new(RefCell::new(Some(Box::pin(f)))) }
    }
}
impl<T> std::future::Future for __Async<T> {
    type Output = T;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<T> {
        let mut slot = self.fut.borrow_mut();
        let f = slot.as_mut().unwrap_or_else(|| panic!("this async value was already awaited"));
        match f.as_mut().poll(cx) {
            std::task::Poll::Ready(v) => {
                *slot = None;
                std::task::Poll::Ready(v)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}
type __Queue = std::sync::Arc<(std::sync::Mutex<Vec<usize>>, std::sync::Condvar)>;
struct __Wake {
    queue: __Queue,
    id: usize,
}
impl std::task::Wake for __Wake {
    fn wake(self: std::sync::Arc<Self>) {
        let (q, cv) = &*self.queue;
        q.lock().unwrap_or_else(|e| e.into_inner()).push(self.id);
        cv.notify_one();
    }
}
struct __Exec {
    queue: __Queue,
    tasks: Vec<Option<__Fut<()>>>,
    timers: Vec<(std::time::Instant, std::task::Waker)>,
}
thread_local! {
    static __EXEC: RefCell<Option<__Exec>> = const { RefCell::new(None) };
}
fn __block_on<T: 'static>(main: impl std::future::Future<Output = T> + 'static) -> T {
    use std::task::{Context, Poll, Waker};
    if __EXEC.with(|e| e.borrow().is_some()) {
        panic!("Async.run cannot be used inside async code; use `await` instead");
    }
    let queue: __Queue = std::sync::Arc::new((std::sync::Mutex::new(vec![0]), std::sync::Condvar::new()));
    __EXEC.with(|e| *e.borrow_mut() = Some(__Exec { queue: queue.clone(), tasks: vec![None], timers: vec![] }));
    let mut main = Box::pin(main);
    let main_waker = Waker::from(std::sync::Arc::new(__Wake { queue: queue.clone(), id: 0 }));
    let result = loop {
        let ready: Vec<usize> = std::mem::take(&mut *queue.0.lock().unwrap_or_else(|e| e.into_inner()));
        let mut finished = None;
        for id in ready {
            if id == 0 {
                if let Poll::Ready(v) = main.as_mut().poll(&mut Context::from_waker(&main_waker)) {
                    finished = Some(v);
                    break;
                }
                continue;
            }
            let task = __EXEC.with(|e| e.borrow_mut().as_mut().unwrap().tasks.get_mut(id).and_then(|t| t.take()));
            if let Some(mut t) = task {
                let w = Waker::from(std::sync::Arc::new(__Wake { queue: queue.clone(), id }));
                if t.as_mut().poll(&mut Context::from_waker(&w)).is_pending() {
                    __EXEC.with(|e| e.borrow_mut().as_mut().unwrap().tasks[id] = Some(t));
                }
            }
        }
        if let Some(v) = finished {
            break v;
        }
        let now = std::time::Instant::now();
        let next = __EXEC.with(|e| {
            let mut b = e.borrow_mut();
            let ex = b.as_mut().unwrap();
            let (due, later): (Vec<_>, Vec<_>) = ex.timers.drain(..).partition(|(t, _)| *t <= now);
            ex.timers = later;
            for (_, w) in due {
                w.wake();
            }
            ex.timers.iter().map(|(t, _)| *t).min()
        });
        let (q, cv) = &*queue;
        let guard = q.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_empty() {
            let wait = match next {
                Some(t) => t.saturating_duration_since(std::time::Instant::now()),
                None => std::time::Duration::from_millis(500),
            };
            let _ = cv.wait_timeout(guard, wait);
        }
    };
    __EXEC.with(|e| *e.borrow_mut() = None);
    result
}
fn __sleep(seconds: f64) -> impl std::future::Future<Output = ()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds.max(0.0));
    std::future::poll_fn(move |cx| {
        if std::time::Instant::now() >= deadline {
            return std::task::Poll::Ready(());
        }
        __EXEC.with(|e| match e.borrow_mut().as_mut() {
            Some(ex) => ex.timers.push((deadline, cx.waker().clone())),
            None => panic!("Time.wait needs async code to be running (`async fn main` or `Async.run`)"),
        });
        std::task::Poll::Pending
    })
}
fn __async_spawn<T: 'static>(job: __Async<T>) -> __Async<T> {
    let state: Rc<RefCell<(Option<T>, Option<std::task::Waker>)>> = Rc::new(RefCell::new((None, None)));
    let s2 = state.clone();
    let task: __Fut<()> = Box::pin(async move {
        let v = job.await;
        let w = {
            let mut st = s2.borrow_mut();
            st.0 = Some(v);
            st.1.take()
        };
        if let Some(w) = w {
            w.wake();
        }
    });
    __EXEC.with(|e| {
        let mut b = e.borrow_mut();
        let ex = b
            .as_mut()
            .unwrap_or_else(|| panic!("Async.spawn needs async code to be running (`async fn main` or `Async.run`)"));
        ex.tasks.push(Some(task));
        let id = ex.tasks.len() - 1;
        ex.queue.0.lock().unwrap_or_else(|e| e.into_inner()).push(id);
    });
    __Async::new(std::future::poll_fn(move |cx| {
        let mut st = state.borrow_mut();
        match st.0.take() {
            Some(v) => std::task::Poll::Ready(v),
            None => {
                st.1 = Some(cx.waker().clone());
                std::task::Poll::Pending
            }
        }
    }))
}
fn __async_all<T: 'static>(jobs: Vec<__Async<T>>) -> __Async<Vec<T>> {
    let mut jobs: Vec<Option<__Async<T>>> = jobs.into_iter().map(Some).collect();
    let mut out: Vec<Option<T>> = (0..jobs.len()).map(|_| None).collect();
    __Async::new(std::future::poll_fn(move |cx| {
        let mut pending = false;
        for (i, j) in jobs.iter_mut().enumerate() {
            if let Some(f) = j {
                match std::pin::Pin::new(f).poll(cx) {
                    std::task::Poll::Ready(v) => {
                        out[i] = Some(v);
                        *j = None;
                    }
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(out.iter_mut().map(|o| o.take().unwrap()).collect())
        }
    }))
}
fn __task_wait<T: Send + 'static>(t: __Task<T>) -> impl std::future::Future<Output = T> {
    std::future::poll_fn(move |cx| {
        if t.is_done() {
            return std::task::Poll::Ready(t.join());
        }
        *t.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(cx.waker().clone());
        if t.is_done() {
            return std::task::Poll::Ready(t.join());
        }
        std::task::Poll::Pending
    })
}
