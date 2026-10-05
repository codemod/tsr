//! Archive-only selected allocation origins and actual checker-pool activity.
use std::cell::Cell;
use std::io::Write;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
pub mod meter;
#[global_allocator]
static ALLOCATOR: meter::Meter = meter::Meter;
static ENABLED: OnceLock<bool> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
static EPOCH: OnceLock<u128> = OnceLock::new();
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static BUSY: AtomicUsize = AtomicUsize::new(0);
static BUSY_PEAK: AtomicUsize = AtomicUsize::new(0);
static ROWS: Mutex<Vec<WorkerRecord>> = Mutex::new(Vec::new());
struct Site {
    calls: AtomicUsize,
    outer: AtomicUsize,
    ns: AtomicU64,
}
impl Site {
    const fn new() -> Self {
        Self { calls: AtomicUsize::new(0), outer: AtomicUsize::new(0), ns: AtomicU64::new(0) }
    }
}
static SITES: [Site; 3] = [const { Site::new() }; 3];
static SPELL: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
thread_local! { static DEPTH: Cell<[usize; 3]> = const { Cell::new([0; 3]) }; }
/// Enable only from the process-selected archive receipt path.
#[must_use]
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        START.get_or_init(Instant::now);
        EPOCH.get_or_init(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
        std::env::var_os("TSR_REFERENCE_ORIGINS").is_some()
    })
}
fn elapsed() -> u64 {
    START.get().unwrap().elapsed().as_nanos().try_into().unwrap()
}
/// One selected origin. Timers coalesce same-site recursion, but different sites overlap.
pub struct SiteGuard {
    kind: usize,
    outer: bool,
    started: Instant,
    _origin: meter::OwnerGuard,
}
/// Enter a selected function without changing its semantic result.
#[must_use]
pub fn site(kind: usize) -> Option<SiteGuard> {
    if !enabled() {
        return None;
    }
    assert!((1..=3).contains(&kind));
    SITES[kind - 1].calls.fetch_add(1, Ordering::Relaxed);
    let outer = DEPTH.with(|slot| {
        let mut depths = slot.get();
        let first = depths[kind - 1] == 0;
        depths[kind - 1] += 1;
        slot.set(depths);
        first
    });
    if outer {
        SITES[kind - 1].outer.fetch_add(1, Ordering::Relaxed);
    }
    Some(SiteGuard {
        kind,
        outer,
        started: Instant::now(),
        _origin: meter::OwnerGuard::enter(kind),
    })
}
impl Drop for SiteGuard {
    fn drop(&mut self) {
        if self.outer {
            SITES[self.kind - 1].ns.fetch_add(
                self.started.elapsed().as_nanos().try_into().unwrap(),
                Ordering::Relaxed,
            );
        }
        DEPTH.with(|slot| {
            let mut depths = slot.get();
            depths[self.kind - 1] -= 1;
            slot.set(depths);
        });
    }
}
/// Record returned strings after the unchanged spelling preparation loop.
pub fn spelling(rendered: usize, written: usize, strings: &[String], retained: bool) {
    if !enabled() {
        return;
    }
    let bytes = strings.iter().map(String::len).sum::<usize>();
    let capacity = strings.iter().map(String::capacity).sum::<usize>();
    let values = [
        1,
        rendered,
        written,
        bytes,
        capacity,
        usize::from(retained),
        if retained { 0 } else { bytes },
        if retained { 0 } else { capacity },
    ];
    for (counter, value) in SPELL.iter().zip(values) {
        counter.fetch_add(value, Ordering::Relaxed);
    }
}
struct WorkerRecord {
    owner: usize,
    begin: u64,
    end: u64,
    checks: Vec<(usize, u64, u64)>,
}
/// Private lifetime guard; metadata is allocated outside tagged compiler origins.
pub struct WorkerGuard {
    row: WorkerRecord,
    _origin: meter::OwnerGuard,
}
/// Observe one constructor/lifetime without forcing a different pool size.
#[must_use]
pub fn worker(owner: usize, file_capacity: usize) -> Option<WorkerGuard> {
    if !enabled() {
        return None;
    }
    assert!(owner < 256);
    let checks = Vec::with_capacity(file_capacity);
    let live = LIVE.fetch_add(1, Ordering::SeqCst) + 1;
    PEAK.fetch_max(live, Ordering::SeqCst);
    Some(WorkerGuard {
        row: WorkerRecord { owner, begin: elapsed(), end: 0, checks },
        _origin: meter::OwnerGuard::enter(4),
    })
}
/// The actual synchronous full-file call interval.
pub struct CheckGuard<'a> {
    row: &'a mut WorkerRecord,
    index: usize,
    begin: u64,
}
impl WorkerGuard {
    /// Enter immediately before `check_source_file`, recording its program index.
    #[must_use]
    pub fn check(&mut self, index: usize) -> CheckGuard<'_> {
        let busy = BUSY.fetch_add(1, Ordering::SeqCst) + 1;
        BUSY_PEAK.fetch_max(busy, Ordering::SeqCst);
        CheckGuard { row: &mut self.row, index, begin: elapsed() }
    }
}
impl Drop for CheckGuard<'_> {
    fn drop(&mut self) {
        self.row.checks.push((self.index, self.begin, elapsed()));
        BUSY.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.row.end = elapsed();
        let _unclassified = meter::OwnerGuard::enter(0);
        let checks = std::mem::take(&mut self.row.checks);
        ROWS.lock().unwrap().push(WorkerRecord {
            owner: self.row.owner,
            begin: self.row.begin,
            end: self.row.end,
            checks,
        });
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}
/// Quiescent receipt written after ordinary CLI compilation and private release.
pub fn finish() {
    if !enabled() {
        return;
    }
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
    assert_eq!(BUSY.load(Ordering::SeqCst), 0);
    let path = std::env::var_os("TSR_REFERENCE_ORIGINS").unwrap();
    let mut output = std::fs::File::create(path).unwrap();
    writeln!(output, "tsr-reference-origins-v1\t{}\t{}", std::process::id(), EPOCH.get().unwrap())
        .unwrap();
    writeln!(
        output,
        "activity\t{}\t{}",
        PEAK.load(Ordering::SeqCst),
        BUSY_PEAK.load(Ordering::SeqCst)
    )
    .unwrap();
    for row in ROWS.lock().unwrap().iter() {
        writeln!(output, "worker\t{}\t{}\t{}", row.owner, row.begin, row.end).unwrap();
        for &(index, begin, end) in &row.checks {
            writeln!(output, "check\t{}\t{}\t{}\t{}", row.owner, index, begin, end).unwrap();
        }
    }
    for (index, site) in SITES.iter().enumerate() {
        writeln!(
            output,
            "site\t{}\t{}\t{}\t{}",
            index + 1,
            site.calls.load(Ordering::Relaxed),
            site.outer.load(Ordering::Relaxed),
            site.ns.load(Ordering::Relaxed)
        )
        .unwrap();
    }
    for owner in 0..=4 {
        let s = meter::snapshot(owner);
        writeln!(
            output,
            "memory\t{owner}\t{}\t{}\t{}\t{}\t{}",
            s.allocation_requests,
            s.cumulative_requested_bytes,
            s.peak_requested_bytes,
            s.live_requested_bytes,
            s.live_padded_layout_bytes
        )
        .unwrap();
    }
    write!(output, "spelling").unwrap();
    for counter in &SPELL {
        write!(output, "\t{}", counter.load(Ordering::Relaxed)).unwrap();
    }
    writeln!(output).unwrap();
}
