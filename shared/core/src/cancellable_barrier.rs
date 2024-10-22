use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

pub struct CancellableBarrier {
    mutex: Mutex<()>,
    cvar: Condvar,
    count: AtomicUsize,
    total: usize,
    generation: AtomicUsize,
    cancelled: AtomicBool,
}

pub struct CancelledBarrier;

impl Display for CancelledBarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cancelled barrier")
    }
}

impl CancellableBarrier {
    pub fn new(n: usize) -> Arc<Self> {
        Arc::new(Self {
            mutex: Mutex::new(()),
            cvar: Condvar::new(),
            count: AtomicUsize::new(0),
            total: n,
            generation: AtomicUsize::new(0),
            cancelled: AtomicBool::new(false),
        })
    }

    pub fn wait(&self) -> Result<(), CancelledBarrier> {
        let mut guard = self.mutex.lock().unwrap();
        let local_gen = self.generation.load(Ordering::Acquire);

        if self.cancelled.load(Ordering::Acquire) {
            return Err(CancelledBarrier);
        }

        let count = self.count.fetch_add(1, Ordering::Acquire) + 1;
        if count < self.total {
            loop {
                guard = self.cvar.wait(guard).unwrap();
                if self.cancelled.load(Ordering::Acquire) {
                    return Err(CancelledBarrier {});
                }
                if local_gen != self.generation.load(Ordering::Acquire) {
                    return Ok(());
                }
            }
        } else {
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            self.cvar.notify_all();
            Ok(())
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.cvar.notify_all();
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Release);
        self.count.store(0, Ordering::Release);
        self.generation.fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_barrier_normal_operation() {
        let num_threads = 3;
        let barrier = CancellableBarrier::new(num_threads);
        let counter = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    assert!(barrier.wait().is_ok());
                    counter.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), num_threads);
    }

    #[test]
    fn test_barrier_cancellation() {
        let num_threads = 3;
        let barrier = CancellableBarrier::new(num_threads);
        let counter = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(10));
                    if barrier.wait().is_err() {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        thread::sleep(Duration::from_millis(5));
        barrier.cancel();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), num_threads);
    }

    #[test]
    fn test_barrier_reset_and_reuse() {
        let num_threads = 3;
        let barrier = CancellableBarrier::new(num_threads);
        let counter = Arc::new(AtomicUsize::new(0));

        // First use
        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    assert!(barrier.wait().is_ok());
                    counter.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), num_threads);

        // Reset and second use
        barrier.reset();
        counter.store(0, Ordering::SeqCst);

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    assert!(barrier.wait().is_ok());
                    counter.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), num_threads);
    }

    #[test]
    fn test_barrier_cancel_then_reset() {
        let num_threads = 3;
        let barrier = CancellableBarrier::new(num_threads);
        let counter = Arc::new(AtomicUsize::new(0));

        // First use with cancellation
        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(10));
                    if barrier.wait().is_err() {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        thread::sleep(Duration::from_millis(5));
        barrier.cancel();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), num_threads);

        // Reset and second use
        barrier.reset();
        counter.store(0, Ordering::SeqCst);

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let counter = Arc::clone(&counter);
                thread::spawn(move || {
                    assert!(barrier.wait().is_ok());
                    counter.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), num_threads);
    }
}
