//! The counting allocator the guarded corpus dumps install, so
//! `tsr_conformance::case_guard` can report and budget per-case peak memory
//! (`tsr-2zk.1041`, `docs/parity/notes/r5-harness.md`).
//!
//! Included with `#[path]` by `diagverdictdump.rs` and `verdictdump.rs`;
//! cargo does not build this directory as an example of its own.
//!
//! `unsafe` for the reason ADR-0011 lists for `examples/alloc_profile.rs`:
//! `GlobalAlloc` is an unsafe trait and there is no safe way to observe
//! allocation. It is tooling and ships in no binary.

use std::alloc::{GlobalAlloc, Layout, System};

use tsr_conformance::case_guard::charge;

/// `System` plus a per-case byte count.
pub struct Counting;

/// `usize` to the signed delta `charge` takes. No layout exceeds `isize::MAX`.
#[expect(clippy::cast_possible_wrap, reason = "a Layout size never exceeds isize::MAX")]
fn bytes(size: usize) -> i64 {
    size as i64
}

// SAFETY: every method forwards to `System` with the same arguments and
// returns its pointer unchanged; the count is a side effect that does not
// allocate (see `charge`).
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            charge(bytes(layout.size()));
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            charge(bytes(layout.size()));
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        charge(-bytes(layout.size()));
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            charge(bytes(new_size) - bytes(layout.size()));
        }
        moved
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;
