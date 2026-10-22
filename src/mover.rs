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

/// Where a window should be, and optionally its size (experiment "live resizing").
type Wanted = (Element, f64, f64, Option<(f64, f64)>);

struct Mailbox {
    /// window key → latest wanted frame; newer posts overwrite older ones.
    pending: Mutex<HashMap<u64, Wanted>>,
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
        self.post_frame(pid, key, element, x, y, None);
    }

    /// Like `post`, also setting the size. A later position-only post keeps the size.
    pub fn post_frame(&mut self, pid: i32, key: u64, element: &Element, x: f64, y: f64, size: Option<(f64, f64)>) {
        let mb = self.boxes.entry(pid).or_insert_with(|| spawn(pid)).clone();
        let mut p = mb.pending.lock().unwrap();
        let size = size.or_else(|| p.get(&key).and_then(|w| w.3));
        if p.insert(key, (element.clone(), x, y, size)).is_none() {
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
        .spawn(move || {
            let mut boosted = false;
            loop {
            // Experiment "high-priority move threads".
            let want = crate::experiments::on(crate::experiments::Exp::PriorityMovers);
            if want != boosted {
                let class = if want { libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE } else { libc::qos_class_t::QOS_CLASS_DEFAULT };
                unsafe { libc::pthread_set_qos_class_self_np(class, 0) };
                boosted = want;
            }
            let batch: Vec<Wanted> = {
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
            for (el, x, y, size) in &batch {
                el.set_position(*x, *y);
                if let Some((w, h)) = size {
                    el.set_size(*w, *h);
                }
            }
            IN_FLIGHT.fetch_sub(batch.len(), Ordering::SeqCst);
            worker.working.store(false, Ordering::SeqCst);
            }
        })
        .expect("spawn mover thread");
    mb
}
