//! Archived conservative plain-documentation attribution; no production deferral.
//! Used by the accompanying source-qualified JSDoc cost patch.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tsr_core::Arena;

#[derive(Clone, Copy, Default)]
struct Counters {
    ns: u64,
    bytes: usize,
    leading: usize,
    comments: usize,
    range_bytes: usize,
    plain_ns: u64,
    plain_bytes: usize,
    plain_comments: usize,
    plain_ranges: usize,
}

#[derive(Default)]
struct State {
    depth: usize,
    comment_depth: usize,
    plain_allowed: bool,
    counters: Counters,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

static OUTPUT: OnceLock<Option<Mutex<File>>> = OnceLock::new();

fn output() -> Option<&'static Mutex<File>> {
    OUTPUT
        .get_or_init(|| {
            let path = std::env::var_os("TSR_JSDOC_COST_PROBE")?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .expect("JSDoc cost trace must be a new writable file");
            let created_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            writeln!(file, "tsr-jsdoc-eligible-v1\t{}\t{created_at}", std::process::id()).unwrap();
            Some(Mutex::new(file))
        })
        .as_ref()
}

/// Coalesce nested documentation parsing so time/arena bytes are not added twice.
pub struct JSDocGuard<'a> {
    arena: &'a Arena,
    start: Option<Instant>,
    bytes: usize,
}

impl<'a> JSDocGuard<'a> {
    /// Start only when the archived observer is enabled.
    pub fn start(arena: &'a Arena) -> Option<Self> {
        output()?;
        let outer = STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let outer = state.depth == 0;
            state.depth += 1;
            state.counters.leading += 1;
            outer
        });
        Some(Self { arena, start: outer.then(Instant::now), bytes: arena.allocated_bytes() })
    }
}

impl Drop for JSDocGuard<'_> {
    fn drop(&mut self) {
        let ns = self.start.map(|start| u64::try_from(start.elapsed().as_nanos()).unwrap());
        STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.depth -= 1;
            if let Some(ns) = ns {
                state.counters.ns += ns;
                state.counters.bytes += self.arena.allocated_bytes() - self.bytes;
            }
        });
    }
}

/// Conservative classifier: any at-sign keeps the full grammar eager.
#[must_use]
pub fn plain_eligible(name: &str, kind: crate::ScriptKind, text: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(kind, crate::ScriptKind::TypeScript | crate::ScriptKind::Tsx)
        && [".ts", ".tsx", ".mts", ".cts"].iter().any(|suffix| name.ends_with(suffix))
        && !text.as_bytes().contains(&b'@')
}

/// Measure outer comment bodies separately from range discovery/attachment.
pub struct PlainGuard<'a> {
    arena: &'a Arena,
    start: Option<Instant>,
    bytes: usize,
}

impl<'a> PlainGuard<'a> {
    /// Begin only when enabled; nested bodies never add overlapping cost.
    pub fn start(arena: &'a Arena, text: &str) -> Option<Self> {
        output()?;
        let eligible = STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let outer = state.comment_depth == 0;
            state.comment_depth += 1;
            let eligible = outer && state.plain_allowed && !text.as_bytes().contains(&b'@');
            if eligible {
                state.counters.plain_comments += 1;
                state.counters.plain_ranges += text.len();
            }
            eligible
        });
        Some(Self { arena, start: eligible.then(Instant::now), bytes: arena.allocated_bytes() })
    }
}

impl Drop for PlainGuard<'_> {
    fn drop(&mut self) {
        let ns = self.start.map(|start| u64::try_from(start.elapsed().as_nanos()).unwrap());
        STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.comment_depth -= 1;
            if let Some(ns) = ns {
                state.counters.plain_ns += ns;
                state.counters.plain_bytes += self.arena.allocated_bytes() - self.bytes;
            }
        });
    }
}

/// Record the input range length, not copied or retained comment bytes.
pub fn comment(bytes: usize) {
    if output().is_some() {
        STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.counters.comments += 1;
            state.counters.range_bytes += bytes;
        });
    }
}

/// Only compiler file-parse sites are enveloped; filename/text copies are excluded.
pub struct FileGuard<'a> {
    arena: &'a Arena,
    name: String,
    source_bytes: usize,
    kind: String,
    start: Instant,
    bytes: usize,
    before: Counters,
}

impl<'a> FileGuard<'a> {
    /// Begin after compiler filename/text copies and before parser entry.
    pub fn start(
        arena: &'a Arena,
        name: &str,
        source: &str,
        kind: crate::ScriptKind,
    ) -> Option<Self> {
        output()?;
        let before = STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            assert_eq!(state.comment_depth, 0);
            state.plain_allowed = plain_eligible(name, kind, "");
            assert_eq!(state.depth, 0, "overlapping compiler file parses are unsupported");
            state.counters
        });
        let bytes = arena.allocated_bytes();
        // Observer path/kind allocations precede the inclusive parser timer.
        let name = name.to_owned();
        let kind = format!("{kind:?}");
        Some(Self {
            arena,
            name,
            source_bytes: source.len(),
            kind,
            start: Instant::now(),
            bytes,
            before,
        })
    }

    /// Record only a successfully returned parser result, outside its timer.
    pub fn finish(self, table: &crate::JSDocTable<'_>) {
        let ns = self.start.elapsed().as_nanos();
        let bytes = self.arena.allocated_bytes() - self.bytes;
        let after = STATE.with(|cell| {
            let state = cell.borrow();
            assert_eq!(state.depth, 0, "unbalanced JSDoc intervals");
            assert_eq!(state.comment_depth, 0);
            state.counters
        });
        // Hex encoding preserves arbitrary UTF-8 filename bytes without TSV escaping.
        let mut name_hex = String::with_capacity(self.name.len() * 2);
        for byte in self.name.as_bytes() {
            write!(name_hex, "{byte:02x}").unwrap();
        }
        writeln!(
            output().unwrap().lock().unwrap(),
            "{name_hex}\t{}\t{}\t{ns}\t{bytes}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.source_bytes,
            self.kind,
            after.ns - self.before.ns,
            after.bytes - self.before.bytes,
            after.leading - self.before.leading,
            after.comments - self.before.comments,
            after.range_bytes - self.before.range_bytes,
            table.len(),
            table.probe_capacity_bytes(),
            after.plain_ns - self.before.plain_ns,
            after.plain_bytes - self.before.plain_bytes,
            after.plain_comments - self.before.plain_comments,
            after.plain_ranges - self.before.plain_ranges,
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::plain_eligible;
    use crate::ScriptKind;

    #[test]
    fn conservative_plain_documentation_domain() {
        for name in ["x.ts", "x.TSX", "x.mts", "x.cts", "x.d.ts", "x.d.mts", "x.d.cts"] {
            assert!(plain_eligible(name, ScriptKind::TypeScript, "/** ordinary λ docs */"));
        }
        for name in ["x.js", "x.jsx", "x.mjs", "x.cjs", "x.json", "x.ts.js"] {
            assert!(!plain_eligible(name, ScriptKind::TypeScript, "/** ordinary docs */"));
        }
        assert!(!plain_eligible("x.ts", ScriptKind::Json, "/** ordinary docs */"));
        for text in [
            "/** @import X */",
            "/** {@link X} */",
            "/** @deprecated */",
            "/** @see X */",
            "/** user@example.com */",
            "/** @unknown */",
            "/** ```@param``` */",
        ] {
            assert!(!plain_eligible("x.ts", ScriptKind::TypeScript, text));
        }
    }
}
