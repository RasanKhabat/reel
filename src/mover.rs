//! Applies window moves off the main thread.
//!
//! Every move is a synchronous request into another app; a slow app (a busy Electron
//! window, say) can take tens of milliseconds to answer. Each app therefore gets its own
//! worker thread, and the main thread only drops the *latest* wanted position into that
//! worker's mailbox and carries on. A slow app skips intermediate frames and lags on its
//! own; the animation loop and every other app stay smooth.

use crate::ax::Element;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

struct Mailbox {
    /// window key → (element, x, y); newer posts overwrite older ones.
    pending: Mutex<HashMap<u64, (Element, f64, f64)>>,
    wake: Condvar,
    working: AtomicBool,
    stop: AtomicBool,
}

#[derive(Default)]
pub struct Mover {
    boxes: HashMap<i32, Arc<Mailbox>>,
}

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

impl Mover {
    pub fn post(&mut self, pid: i32, key: u64, element: &Element, x: f64, y: f64) {
        let mb = self.boxes.entry(pid).or_insert_with(|| spawn(pid)).clone();
        let mut p = mb.pending.lock().unwrap();
        if p.insert(key, (element.clone(), x, y)).is_none() {
            IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        }
        drop(p);
        mb.wake.notify_one();
    }

    /// Stops the worker for an app that quit.
    pub fn forget(&mut self, pid: i32) {
        if let Some(mb) = self.boxes.remove(&pid) {
            mb.stop.store(true, Ordering::SeqCst);
            mb.wake.notify_one();
        }
    }

    /// Waits (up to `timeout`) for every posted move to land. Used before Reel moves
    /// windows itself synchronously, so a stale queued move can't land afterwards.
    pub fn flush(timeout: std::time::Duration) {
        let end = std::time::Instant::now() + timeout;
        while !Self::is_idle() && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// True when no move is waiting or being applied (used by the tests).
    pub fn is_idle() -> bool {
        IN_FLIGHT.load(Ordering::SeqCst) == 0
    }
}

fn spawn(pid: i32) -> Arc<Mailbox> {
    let mb = Arc::new(Mailbox {
        pending: Mutex::new(HashMap::new()),
        wake: Condvar::new(),
        working: AtomicBool::new(false),
        stop: AtomicBool::new(false),
    });
    let worker = mb.clone();
    std::thread::Builder::new()
        .name(format!("reel-mover-{pid}"))
        .spawn(move || loop {
            let batch: Vec<(Element, f64, f64)> = {
                let mut p = worker.pending.lock().unwrap();
                while p.is_empty() && !worker.stop.load(Ordering::SeqCst) {
                    p = worker.wake.wait(p).unwrap();
                }
                if worker.stop.load(Ordering::SeqCst) {
                    IN_FLIGHT.fetch_sub(p.len(), Ordering::SeqCst);
                    return;
                }
                worker.working.store(true, Ordering::SeqCst);
                p.drain().map(|(_, v)| v).collect()
            };
            for (el, x, y) in &batch {
                el.set_position(*x, *y);
            }
            IN_FLIGHT.fetch_sub(batch.len(), Ordering::SeqCst);
            worker.working.store(false, Ordering::SeqCst);
        })
        .expect("spawn mover thread");
    mb
}
