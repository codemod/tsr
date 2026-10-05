//! Archive-only JSDoc boundary timing and allocation-origin observer.
//! Timers are observer-scale costs. Plain is a subset of body, not additive.
use std::cell::Cell;
use std::io::Write;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub mod meter;
#[global_allocator]
static ALLOCATOR: meter::Meter = meter::Meter;

const NAMES: [&str; 12] = [
    "parse",
    "leading",
    "range",
    "body",
    "plain",
    "list",
    "attach",
    "nested_attach",
    "bind_copy",
    "set_jsdoc",
    "constructor",
    "typedef_scan",
];
struct Counter {
    calls: AtomicUsize,
    ns: AtomicU64,
    arena: AtomicUsize,
}
impl Counter {
    const fn new() -> Self {
        Self { calls: AtomicUsize::new(0), ns: AtomicU64::new(0), arena: AtomicUsize::new(0) }
    }
}
static COUNTERS: [Counter; 12] = [const { Counter::new() }; 12];
static ENABLED: OnceLock<bool> = OnceLock::new();
static START: OnceLock<u128> = OnceLock::new();
type MapRecord = (usize, usize, usize, usize, usize);
static MAPS: Mutex<Vec<MapRecord>> = Mutex::new(Vec::new());
static DECLINED: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static LEADING: Cell<usize> = const { Cell::new(0) };
    static BODY: Cell<usize> = const { Cell::new(0) };
    static PLAIN: Cell<bool> = const { Cell::new(false) };
    static OWNER: Cell<usize> = const { Cell::new(1) };
}
/// Whether this fresh process enabled the archived observer.
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        START.get_or_init(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
        std::env::var_os("TSR_JSDOC_SETUP_PROBE").is_some()
    })
}
/// Assign a worker-local map allocation owner.
///
/// # Panics
/// Panics for an invalid archived owner index.
pub fn private_owner(owner: usize) {
    assert!((1..=127).contains(&owner));
    OWNER.with(|s| s.set(owner));
}
/// Current private map owner.
pub fn owner() -> usize {
    OWNER.with(Cell::get)
}
/// Envelope a compiler parse and select conservative plain TS documentation.
pub fn file(name: &str) -> Option<Guard> {
    if !enabled() {
        return None;
    }
    assert_eq!(LEADING.with(Cell::get), 0);
    assert_eq!(BODY.with(Cell::get), 0);
    let name = name.to_ascii_lowercase();
    PLAIN.with(|s| s.set([".ts", ".tsx", ".mts", ".cts"].iter().any(|x| name.ends_with(x))));
    Some(Guard::new(0, None, None))
}
/// Envelope a named measured boundary and optional allocation owner.
#[must_use]
pub fn start(kind: usize, owner: Option<usize>) -> Option<Guard> {
    enabled().then(|| Guard::new(kind, owner, None))
}
/// Coalesce nested leading-documentation intervals.
#[must_use]
pub fn leading() -> Option<Guard> {
    if !enabled() {
        return None;
    }
    let outer = LEADING.with(|s| {
        let n = s.get();
        s.set(n + 1);
        n == 0
    });
    Some(Guard::new(if outer { 1 } else { 12 }, None, Some(1)))
}
/// Measure top-level leading pieces outside comment bodies.
pub fn piece(kind: usize) -> Option<Guard> {
    (enabled() && LEADING.with(Cell::get) == 1 && BODY.with(Cell::get) == 0)
        .then(|| Guard::new(kind, None, None))
}
/// Coalesce bodies and classify the plain subset.
pub fn body(text: &str) -> Option<Guard> {
    if !enabled() {
        return None;
    }
    let outer = BODY.with(|s| {
        let n = s.get();
        s.set(n + 1);
        n == 0
    });
    let plain = outer && PLAIN.with(Cell::get) && !text.as_bytes().contains(&b'@');
    let mut guard = Guard::new(if outer { 3 } else { 12 }, None, Some(2));
    guard.plain = plain;
    Some(guard)
}
/// Separate outer attachment from attachment nested in a comment body.
pub fn attach() -> Option<Guard> {
    start(if LEADING.with(Cell::get) == 0 { 6 } else { 7 }, Some(128))
}
/// Record final setup map capacities outside the timed region.
pub fn maps(entries: usize, entry_capacity: usize, hosts: usize, host_capacity: usize) {
    if enabled() {
        MAPS.lock().unwrap().push((owner(), entries, entry_capacity, hosts, host_capacity));
    }
}
/// Count ordinary references declined before typedef scanning.
pub fn declined() {
    if enabled() {
        DECLINED.fetch_add(1, Ordering::Relaxed);
    }
}
/// A non-cloning interval with nested-depth and owner restoration.
pub struct Guard {
    kind: usize,
    started: Instant,
    owner: Option<meter::OwnerGuard>,
    depth: Option<usize>,
    plain: bool,
    arena: usize,
}
impl Guard {
    fn new(kind: usize, owner: Option<usize>, depth: Option<usize>) -> Self {
        Self {
            kind,
            started: Instant::now(),
            owner: owner.map(meter::OwnerGuard::enter),
            depth,
            plain: false,
            arena: 0,
        }
    }
    /// Record requested arena payload bytes, excluding arena block capacity.
    pub fn arena_bytes(&mut self, bytes: usize) {
        self.arena = bytes;
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let ns = u64::try_from(self.started.elapsed().as_nanos()).unwrap();
        // Drop the owner before accounting/formatting; observer storage is unclassified.
        self.owner.take();
        for kind in [self.kind, if self.plain { 4 } else { 12 }] {
            if let Some(c) = COUNTERS.get(kind) {
                c.calls.fetch_add(1, Ordering::Relaxed);
                c.ns.fetch_add(ns, Ordering::Relaxed);
                c.arena.fetch_add(self.arena, Ordering::Relaxed);
            }
        }
        if let Some(depth) = self.depth {
            if depth == 1 {
                LEADING.with(|s| s.set(s.get() - 1));
            } else {
                BODY.with(|s| s.set(s.get() - 1));
            }
        }
    }
}
/// Call only after the compiler invocation and private workers have returned.
pub fn finish() {
    if !enabled() {
        return;
    }
    let path = std::env::var_os("TSR_JSDOC_SETUP_PROBE").unwrap();
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(path).unwrap();
    writeln!(file, "tsr-jsdoc-setup-v1\t{}\t{}", std::process::id(), START.get().unwrap()).unwrap();
    for (name, c) in NAMES.iter().zip(&COUNTERS) {
        writeln!(
            file,
            "timer\t{name}\t{}\t{}\t{}",
            c.calls.load(Ordering::Relaxed),
            c.ns.load(Ordering::Relaxed),
            c.arena.load(Ordering::Relaxed)
        )
        .unwrap();
    }
    writeln!(file, "declined\t{}", DECLINED.load(Ordering::Relaxed)).unwrap();
    for &(id, a, b, c, d) in MAPS.lock().unwrap().iter() {
        writeln!(file, "maps\t{id}\t{a}\t{b}\t{c}\t{d}").unwrap();
    }
    for id in [0, 1, 2, 3, 4, 128, 129] {
        let s = meter::snapshot(id);
        writeln!(
            file,
            "memory\t{id}\t{}\t{}\t{}\t{}\t{}",
            s.live_requested_bytes,
            s.peak_requested_bytes,
            s.allocation_requests,
            s.live_padded_layout_bytes,
            s.cumulative_requested_bytes
        )
        .unwrap();
    }
    writeln!(file, "complete").unwrap();
}
