//! Private opt-in preparation experiment. Charges cover requested buffer
//! layouts, conservatively including old and replacement layouts during grow.
//! Allocator rounding, stack/coordination/parser memory and RSS are excluded.
//! Exact reported capacities are source-qualified to the inspected toolchain;
//! this is not a portable allocator contract or production admission policy.

use std::io::{self, Read};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct Budget {
    limit: usize,
    state: Arc<Mutex<Stats>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Stats {
    pub live: usize,
    pub peak: usize,
    pub acquired: usize,
    pub released: usize,
}

#[derive(Debug)]
pub(super) enum Error {
    Budget { requested: usize, live: usize, limit: usize },
    Io(io::Error),
    Allocation(std::collections::TryReserveError),
    Overflow,
    UnsupportedCapacity { requested: usize, actual: usize },
}

impl Budget {
    pub fn new(limit: usize) -> Self {
        Self { limit, state: Arc::new(Mutex::new(Stats::default())) }
    }

    pub fn stats(&self) -> Stats {
        *self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn reserve(&self, bytes: usize) -> Result<Lease, Error> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = state.live.checked_add(bytes).ok_or(Error::Overflow)?;
        if next > self.limit {
            return Err(Error::Budget { requested: bytes, live: state.live, limit: self.limit });
        }
        state.live = next;
        state.peak = state.peak.max(next);
        state.acquired += 1;
        Ok(Lease { budget: self.clone(), bytes })
    }
}

struct Lease {
    budget: Budget,
    bytes: usize,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.live -= self.bytes;
        state.released += 1;
    }
}

// Declaration order is intentional: drop the allocation before its charge.
struct Buffer<T> {
    data: Vec<T>,
    lease: Lease,
}

impl<T: Copy> Buffer<T> {
    fn new(budget: &Budget) -> Result<Self, Error> {
        Ok(Self { data: Vec::new(), lease: budget.reserve(0)? })
    }

    fn reserve_capacity(&mut self, capacity: usize) -> Result<(), Error> {
        if capacity <= self.data.capacity() {
            return Ok(());
        }
        let bytes = capacity.checked_mul(std::mem::size_of::<T>()).ok_or(Error::Overflow)?;
        // Charge the entire replacement while the old charge is still live.
        // realloc may keep the address, so this peak is conservative, not an
        // observed simultaneous-allocation peak.
        let replacement = self.lease.budget.reserve(bytes)?;
        self.data.try_reserve_exact(capacity - self.data.len()).map_err(Error::Allocation)?;
        let actual = self.data.capacity();
        self.lease = replacement;
        if actual != capacity {
            // No success or fallback is permitted on an unqualified allocator.
            // The differing capacity was observed after allocation: this path
            // does not establish a pre-allocation bound for such a toolchain.
            return Err(Error::UnsupportedCapacity { requested: capacity, actual });
        }
        Ok(())
    }

    fn extend(&mut self, values: &[T]) -> Result<(), Error> {
        let needed = self.data.len().checked_add(values.len()).ok_or(Error::Overflow)?;
        if needed > self.data.capacity() {
            let doubled = self.data.capacity().checked_mul(2).ok_or(Error::Overflow)?;
            self.reserve_capacity(needed.max(doubled))?;
        }
        // Capacity already covers the append; this cannot request growth.
        self.data.extend_from_slice(values);
        Ok(())
    }
}

// No Clone, mutable dereference or bare-String escape: moves retain the lease.
pub(super) struct PreparedText {
    text: String,
    _lease: Lease,
}

impl PreparedText {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn capacity(&self) -> usize {
        self.text.capacity()
    }
}

fn output_length(mut chars: impl Iterator<Item = char>) -> Result<usize, Error> {
    chars.try_fold(0usize, |len, value| len.checked_add(value.len_utf8()).ok_or(Error::Overflow))
}

fn decode(raw: &[u8], budget: &Budget) -> Result<Buffer<u8>, Error> {
    let mut output = Buffer::new(budget)?;
    if raw.starts_with(&[0xff, 0xfe]) || raw.starts_with(&[0xfe, 0xff]) {
        let little = raw[0] == 0xff;
        let body = &raw[2..];
        let mut units = Buffer::new(budget)?;
        units.reserve_capacity(body.len() / 2)?;
        for pair in body.chunks_exact(2) {
            let pair = [pair[0], pair[1]];
            let unit = if little { u16::from_le_bytes(pair) } else { u16::from_be_bytes(pair) };
            units.data.push(unit);
        }
        let chars =
            || char::decode_utf16(units.data.iter().copied()).map(|r| r.unwrap_or('\u{fffd}'));
        output.reserve_capacity(output_length(chars())?)?;
        for value in chars() {
            let mut encoded = [0; 4];
            output.extend(value.encode_utf8(&mut encoded).as_bytes())?;
        }
    } else {
        let body = raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw);
        let length = body.utf8_chunks().try_fold(0usize, |len, chunk| {
            len.checked_add(chunk.valid().len())
                .and_then(|n| n.checked_add(if chunk.invalid().is_empty() { 0 } else { 3 }))
                .ok_or(Error::Overflow)
        })?;
        output.reserve_capacity(length)?;
        for chunk in body.utf8_chunks() {
            output.extend(chunk.valid().as_bytes())?;
            if !chunk.invalid().is_empty() {
                output.extend("\u{fffd}".as_bytes())?;
            }
        }
    }
    Ok(output)
}

// Only the ordinary OS backing file is exposed as a qualified source. Other
// filesystem hosts need their own controlled raw-reader capability; never
// adapt an allocating read_file result behind this boundary.
pub(super) fn prepare_file(
    file: &mut std::fs::File,
    budget: &Budget,
) -> Result<PreparedText, Error> {
    prepare(file, budget)
}

fn prepare(reader: &mut impl Read, budget: &Budget) -> Result<PreparedText, Error> {
    let mut raw = Buffer::new(budget)?;
    // Unknown/stale metadata never authorizes uncharged growth. This prototype
    // ignores hints and grows from bytes actually read; the scratch is on stack.
    let mut scratch = [0; 4096];
    loop {
        let count = match reader.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(Error::Io(error)),
        };
        raw.extend(&scratch[..count])?;
    }
    let output = decode(&raw.data, budget)?;
    drop(raw);
    let Buffer { data, lease } = output;
    let text = String::from_utf8(data).expect("decoder writes only valid UTF-8");
    Ok(PreparedText { text, _lease: lease })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balanced(budget: &Budget) {
        let state = budget.stats();
        assert_eq!(state.live, 0);
        assert_eq!(state.acquired, state.released);
        assert!(state.peak <= budget.limit);
    }

    #[test]
    fn moves_and_channel_retention_keep_the_charge() {
        let budget = Budget::new(1000);
        let text = prepare(&mut &b"const x = 1;"[..], &budget).unwrap();
        let capacity = text.capacity();
        let (send, recv) = std::sync::mpsc::channel();
        std::thread::spawn(move || send.send(text).unwrap()).join().unwrap();
        assert_eq!(budget.stats().live, capacity);
        let retained = recv.recv().unwrap();
        assert_eq!(retained.text(), "const x = 1;");
        assert_eq!(budget.stats().live, capacity);
        drop(retained);
        balanced(&budget);
    }

    #[test]
    fn refusal_during_raw_units_and_output_releases_all_charges() {
        let controls: &[(&[u8], usize)] =
            &[(&[1; 32], 16), (&[0xff, 0xfe, 0x00, 0x08], 5), (&[0xff, 0xfe, 0x00, 0x08], 7)];
        for &(bytes, limit) in controls {
            let budget = Budget::new(limit);
            assert!(matches!(prepare(&mut bytes.as_ref(), &budget), Err(Error::Budget { .. })));
            balanced(&budget);
        }
    }

    #[test]
    fn read_failure_and_unwind_release_live_raw_storage() {
        struct Fails {
            first: bool,
            panic: bool,
        }
        impl Read for Fails {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.first {
                    self.first = false;
                    out[..8].copy_from_slice(b"// hello");
                    return Ok(8);
                }
                assert!(!self.panic, "controlled reader unwind");
                Err(io::Error::other("controlled read failure"))
            }
        }
        let budget = Budget::new(1000);
        assert!(matches!(
            prepare(&mut Fails { first: true, panic: false }, &budget),
            Err(Error::Io(_))
        ));
        balanced(&budget);
        assert!(
            std::panic::catch_unwind(|| {
                let _ = prepare(&mut Fails { first: true, panic: true }, &budget);
            })
            .is_err()
        );
        balanced(&budget);
    }

    #[test]
    fn consumer_unwind_releases_prepared_text() {
        let budget = Budget::new(1000);
        assert!(
            std::panic::catch_unwind(|| {
                let _text = prepare(&mut &b"// retained"[..], &budget).unwrap();
                panic!("controlled consumer unwind");
            })
            .is_err()
        );
        balanced(&budget);
    }

    #[test]
    fn malformed_utf8_keeps_existing_lossy_semantics() {
        for bytes in [&b"\xe2\x82x"[..], &b"\xf0\x80\x80\x80"[..], &b"\xff\xef\xbb\xbf"[..]] {
            let budget = Budget::new(1000);
            let text = prepare(&mut bytes.as_ref(), &budget).unwrap();
            assert_eq!(text.text(), tsr_vfs::decode_bytes(bytes));
            drop(text);
            balanced(&budget);
        }
    }
}
