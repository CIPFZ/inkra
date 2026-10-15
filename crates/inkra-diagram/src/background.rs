//! Diagram layout off the UI thread (SPEC 2.4, AD-5).
//!
//! The live view asks [`poll`] when it lays out a diagram block, and
//! [`touch`]es blocks still waiting for their diagram in every frame they are
//! visible. A result is computed once per source and palette by a worker
//! thread and cached; until it arrives the caller shows the previous rendering.
//! Edits are debounced (150 ms after the block's previous source), a block
//! whose source changes cancels the layout of its old source, and layouts no
//! visible block asked for in the last second are abandoned (the block
//! scrolled away or was deleted).

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::{Error, Palette, Scene};

/// Debounce of edits to the same block.
pub const DEBOUNCE: Duration = Duration::from_millis(150);
/// A layout not asked for in this long is abandoned.
pub const EXPIRE: Duration = Duration::from_secs(1);

/// The state of a diagram.
#[derive(Debug, Clone)]
pub enum Poll {
    /// Laid out.
    Ready(Arc<Result<Scene, Error>>),
    /// Waiting or running.
    Pending,
}

type Key = u64;

struct Job {
    key: Key,
    src: String,
    pal: Palette,
    not_before: Instant,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    /// Latest key per block (slot) and when the block last asked for it.
    slots: HashMap<usize, (Key, Instant)>,
    running: Option<(Key, Arc<AtomicBool>)>,
    worker: bool,
}

impl State {
    /// Cancels queued and running layouts nobody wants, and forgets blocks
    /// that stopped asking.
    fn sweep(&mut self, now: Instant) {
        self.slots
            .retain(|_, (_, t)| now.duration_since(*t) < EXPIRE);
        let keep: Vec<Key> = self.slots.values().map(|(k, _)| *k).collect();
        self.queue.retain(|j| keep.contains(&j.key));
        if let Some((k, flag)) = &self.running
            && !keep.contains(k)
        {
            flag.store(true, Ordering::SeqCst);
        }
    }

    fn has_job(&self, key: Key) -> bool {
        self.queue.iter().any(|j| j.key == key)
            || self
                .running
                .as_ref()
                .is_some_and(|(k, f)| *k == key && !f.load(Ordering::SeqCst))
    }
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

type Waker = Box<dyn Fn() + Send + Sync>;

static SHARED: OnceLock<Shared> = OnceLock::new();
static WAKE: OnceLock<Mutex<Option<Waker>>> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn shared() -> &'static Shared {
    SHARED.get_or_init(|| Shared {
        state: Mutex::new(State::default()),
        cv: Condvar::new(),
    })
}

fn key_of(src: &str, pal: &Palette) -> Key {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (src, pal).hash(&mut h);
    h.finish()
}

/// Changes whenever a background layout finishes (layout caches hash it).
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Called from the worker thread when a layout finished (request a repaint).
pub fn on_ready(f: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut w) = WAKE.get_or_init(Default::default).lock() {
        *w = Some(Box::new(f));
    }
}

/// The diagram `src` of block `slot` (any stable id, e.g. its position): the
/// cached result, or `Pending` while it is laid out in the background.
pub fn poll(slot: usize, src: &str, pal: &Palette) -> Poll {
    let key = key_of(src, pal);
    let now = Instant::now();
    let cached = crate::cached(src, pal);
    let sh = shared();
    let Ok(mut st) = sh.state.lock() else {
        return Poll::Ready(cached.unwrap_or_else(|| crate::render(src, pal)));
    };
    let prev = st.slots.insert(slot, (key, now)).map(|(k, _)| k);
    st.sweep(now);
    if let Some(r) = cached {
        return Poll::Ready(r);
    }
    if st.has_job(key) {
        return Poll::Pending;
    }
    // A changed source of a block that had one is an edit: debounced.
    let edited = prev.is_some_and(|k| k != key);
    st.queue.push_back(Job {
        key,
        src: src.to_owned(),
        pal: pal.clone(),
        not_before: if edited { now + DEBOUNCE } else { now },
        cancel: Arc::new(AtomicBool::new(false)),
    });
    if !st.worker {
        st.worker = std::thread::Builder::new()
            .name("inkra-diagram".into())
            .spawn(worker)
            .is_ok();
        if !st.worker {
            drop(st);
            return Poll::Ready(crate::render(src, pal));
        }
    }
    sh.cv.notify_one();
    Poll::Pending
}

/// Block `slot` is visible and still waits for its diagram (every frame).
pub fn touch(slot: usize) {
    if let Ok(mut st) = shared().state.lock()
        && let Some((_, t)) = st.slots.get_mut(&slot)
    {
        *t = Instant::now();
    }
}

fn worker() {
    let sh = shared();
    loop {
        let job = {
            let Ok(mut st) = sh.state.lock() else { return };
            loop {
                let now = Instant::now();
                st.sweep(now);
                if let Some(i) = st.queue.iter().position(|j| j.not_before <= now) {
                    let job = st.queue.remove(i).expect("index from position");
                    st.running = Some((job.key, job.cancel.clone()));
                    break job;
                }
                let wait = st
                    .queue
                    .iter()
                    .map(|j| j.not_before.saturating_duration_since(now))
                    .min()
                    .unwrap_or(Duration::from_secs(3600));
                st = match sh.cv.wait_timeout(st, wait) {
                    Ok((g, _)) => g,
                    Err(_) => return,
                };
            }
        };
        let flag = job.cancel.clone();
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::render_cancellable(&job.src, &job.pal, &|| flag.load(Ordering::SeqCst))
        }))
        .unwrap_or_else(|_| {
            Some(Err(Error {
                line: 0,
                message: "internal error while drawing the diagram".into(),
            }))
        });
        if let Ok(mut st) = sh.state.lock() {
            st.running = None;
        }
        if let Some(r) = done {
            crate::store(&job.src, &job.pal, Arc::new(r));
            GENERATION.fetch_add(1, Ordering::SeqCst);
            if let Some(w) = WAKE.get()
                && let Ok(w) = w.lock()
                && let Some(f) = w.as_ref()
            {
                f();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_ready(slot: usize, src: &str, pal: &Palette) -> Arc<Result<Scene, Error>> {
        for _ in 0..500 {
            if let Poll::Ready(r) = poll(slot, src, pal) {
                return r;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no result for {src:?}");
    }

    #[test]
    fn results_arrive_in_the_background() {
        let pal = Palette::light();
        let src = "flowchart LR\n  bg_a --> bg_b\n";
        let g = generation();
        assert!(matches!(poll(9_000, src, &pal), Poll::Pending));
        assert!(wait_ready(9_000, src, &pal).is_ok());
        assert!(generation() > g);
        // Cached: ready at once, also for another block.
        assert!(matches!(poll(9_001, src, &pal), Poll::Ready(_)));
    }

    #[test]
    fn edits_supersede_and_are_debounced() {
        let pal = Palette::light();
        let slot = 9_100;
        let first = "flowchart LR\n  ed_1 --> ed_2\n";
        assert!(wait_ready(slot, first, &pal).is_ok());
        // Typing: each new source replaces the previous request of the block.
        let t = Instant::now();
        let mut last = String::new();
        for k in 0..5 {
            last = format!("flowchart LR\n  ed_1 --> ed_{k}x\n");
            assert!(matches!(poll(slot, &last, &pal), Poll::Pending));
        }
        assert!(wait_ready(slot, &last, &pal).is_ok());
        assert!(t.elapsed() >= DEBOUNCE, "an edit waits for the debounce");
        // The superseded intermediate sources were never laid out.
        assert!(crate::cached("flowchart LR\n  ed_1 --> ed_1x\n", &pal).is_none());
    }

    #[test]
    fn cancel_stops_a_flowchart_layout() {
        let g = crate::flowchart::parse("flowchart LR\n  a --> b\n").unwrap();
        assert!(crate::flowchart::render_with(&g, &Palette::light(), &|| true).is_none());
        assert!(crate::flowchart::render_with(&g, &Palette::light(), &|| false).is_some());
    }
}
