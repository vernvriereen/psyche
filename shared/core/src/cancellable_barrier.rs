use std::fmt::Display;
use std::sync::{Arc, Condvar, Mutex};

pub struct CancellableBarrier {
    state: Mutex<BarrierState>,
    cvar: Condvar,
    total: usize,
}

struct BarrierState {
    count: usize,
    generation: usize,
    cancelled: bool,
}

#[derive(Debug)]
pub struct CancelledBarrier;

impl Display for CancelledBarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cancelled barrier")
    }
}

impl CancellableBarrier {
    pub fn new(n: usize) -> Arc<Self> {
        assert!(n > 0, "Barrier size must be greater than 0");
        Arc::new(Self {
            state: Mutex::new(BarrierState {
                count: 0,
                generation: 0,
                cancelled: false,
            }),
            cvar: Condvar::new(),
            total: n,
        })
    }

    pub fn wait(&self) -> Result<(), CancelledBarrier> {
        let mut state = self.state.lock().unwrap();
        let generation = state.generation;

        if state.cancelled {
            return Err(CancelledBarrier);
        }

        state.count += 1;

        if state.count < self.total {
            // Not all threads have arrived yet, wait
            while !state.cancelled && generation == state.generation {
                state = self.cvar.wait(state).unwrap();
            }

            // If cancelled, we need to clean up
            if state.cancelled {
                // Only decrease if we haven't reset the count already
                if state.count > 0 {
                    state.count -= 1;
                }
                self.cvar.notify_all();
                return Err(CancelledBarrier);
            }

            Ok(())
        } else {
            // Last thread to arrive
            state.count = 0;
            state.generation = state.generation.wrapping_add(1);
            self.cvar.notify_all();
            Ok(())
        }
    }

    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap();
        state.cancelled = true;
        state.count = 0;
        state.generation = state.generation.wrapping_add(1);
        self.cvar.notify_all();
    }

    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.cancelled = false;
        state.count = 0;
        state.generation = state.generation.wrapping_add(1);
        self.cvar.notify_all();
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_barrier_basic() {
        let barrier = CancellableBarrier::new(3);
        let barrier2 = barrier.clone();
        let barrier3 = barrier.clone();

        let t1 = thread::spawn(move || barrier.wait().unwrap());
        let t2 = thread::spawn(move || barrier2.wait().unwrap());
        let t3 = thread::spawn(move || barrier3.wait().unwrap());

        t1.join().unwrap();
        t2.join().unwrap();
        t3.join().unwrap();
    }

    #[test]
    fn test_barrier_cancel() {
        let barrier = CancellableBarrier::new(3);
        let barrier2 = barrier.clone();
        let barrier3 = barrier.clone();

        let t1 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            barrier.wait()
        });

        let t2 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            barrier2.wait()
        });

        let t3 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            barrier3.cancel();
        });

        assert!(t1.join().unwrap().is_err());
        assert!(t2.join().unwrap().is_err());
        t3.join().unwrap();
    }

    #[test]
    fn test_barrier_reset_and_reuse() {
        let barrier = CancellableBarrier::new(2);
        let barrier1 = barrier.clone();
        let barrier2 = barrier.clone();

        // First use
        let t1 = thread::spawn(move || barrier1.wait().unwrap());
        let t2 = thread::spawn(move || barrier2.wait().unwrap());
        t1.join().unwrap();
        t2.join().unwrap();

        // Reset and reuse
        barrier.reset();
        let barrier2 = barrier.clone();

        let t1 = thread::spawn(move || barrier.wait().unwrap());
        let t2 = thread::spawn(move || barrier2.wait().unwrap());
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[test]
    fn test_barrier_multiple_generations() {
        let barrier = CancellableBarrier::new(2);
        let (tx, rx) = mpsc::channel();

        let mut handles = vec![];
        for _ in 0..2 {  // Changed from 3 to 2 to match barrier size
            let barrier = barrier.clone();
            let tx = tx.clone();
            handles.push(thread::spawn(move || {
                for i in 0..3 {
                    barrier.wait().unwrap();
                    tx.send(i).unwrap();
                }
            }));
        }

        // We should receive the numbers in order for each thread
        let mut results = vec![];
        for _ in 0..6 {  // Changed from 9 to 6 (2 threads * 3 iterations)
            results.push(rx.recv().unwrap());
        }

        // Check that we got two sequences of 0,1,2
        assert_eq!(results.iter().filter(|&&x| x == 0).count(), 2);
        assert_eq!(results.iter().filter(|&&x| x == 1).count(), 2);
        assert_eq!(results.iter().filter(|&&x| x == 2).count(), 2);

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_barrier_cancel_and_reset() {
        let barrier = CancellableBarrier::new(3);
        
        // Cancel before any waits
        barrier.cancel();
        assert!(barrier.wait().is_err());

        // Reset and try again
        barrier.reset();
        let barrier2 = barrier.clone();
        let barrier3 = barrier.clone();

        let t1 = thread::spawn(move || barrier.wait().unwrap());
        let t2 = thread::spawn(move || barrier2.wait().unwrap());
        let t3 = thread::spawn(move || barrier3.wait().unwrap());

        t1.join().unwrap();
        t2.join().unwrap();
        t3.join().unwrap();
    }

    #[test]
    #[should_panic(expected = "Barrier size must be greater than 0")]
    fn test_barrier_zero_size() {
        CancellableBarrier::new(0);
    }

    #[test]
    fn test_barrier_single_thread() {
        let barrier = CancellableBarrier::new(1);
        barrier.wait().unwrap(); // Should complete immediately
    }

    #[test]
    fn test_barrier_cancel_after_partial_arrival() {
        let barrier = CancellableBarrier::new(3);
        let barrier2 = barrier.clone();
        let barrier3 = barrier.clone();

        let t1 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            barrier.wait()
        });

        let t2 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            barrier2.wait()
        });

        // Let the other threads start waiting
        thread::sleep(Duration::from_millis(100));
        barrier3.cancel();

        assert!(t1.join().unwrap().is_err());
        assert!(t2.join().unwrap().is_err());
    }

    #[test]
    fn test_barrier_stress() {
        let barrier = CancellableBarrier::new(10);
        let mut handles = vec![];

        for _ in 0..10 {
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    barrier.wait().unwrap();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_barrier_cancel_stress() {
        let barrier = CancellableBarrier::new(10);
        let mut handles = vec![];

        // Spawn 9 waiting threads
        for _ in 0..9 {
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                loop {
                    match barrier.wait() {
                        Ok(()) => continue,
                        Err(_) => break,
                    }
                }
            }));
        }

        // Spawn a thread that alternates between cancel and reset
        let cancel_barrier = barrier.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..10 {
                thread::sleep(Duration::from_millis(1));
                cancel_barrier.cancel();
                thread::sleep(Duration::from_millis(1));
                cancel_barrier.reset();
            }
        }));

        for handle in handles {
            handle.join().unwrap();
        }
    }
}