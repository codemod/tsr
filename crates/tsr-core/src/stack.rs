//! Growing the stack for deeply recursive tree walks.
//!
//! # Why this exists
//!
//! A TypeScript AST is nested as deeply as its input allows, and the corpus is a
//! *compiler* test suite: `compiler/binderBinaryExpressionStress` is one
//! left-leaning `'' + '' + …` chain 4,958 operands deep. Every recursive walk over
//! such a tree — binding, printing, visiting, and in time checking — costs a stack
//! frame per level.
//!
//! Upstream has no equivalent problem and therefore nothing to port. Go's runtime
//! grows a goroutine's stack on demand (roughly 8 KB initially, copy-and-double,
//! ceiling 1 GB on 64-bit), so `internal/binder/binder.go:2212` recursing 5,000
//! deep costs it nothing and typescript-go contains no stack policy at all.
//!
//! A Rust thread's stack is fixed at spawn. [`ensure_sufficient`] closes that gap
//! the way rustc does with its own `ensure_sufficient_stack`: check how much room
//! is left, and allocate a fresh segment before recursing further if it is
//! running out.
//!
//! # wasm32 is different, and deliberately unhandled
//!
//! wasm32 has **one linear stack, sized at link time** (`-z stack-size`, wasm-ld
//! defaults to 1 MiB), which cannot grow. Overflow arrives as a trap that kills
//! the instance and is not catchable. `stacker` cannot help there, so on wasm this
//! function is a plain call and deep input **traps**.
//!
//! That is an accepted, documented divergence: native has no nesting ceiling,
//! wasm has one it cannot report. See
//! [ADR-0030](../../../docs/adr/0030-grow-the-stack-natively-wasm-traps.md),
//! which supersedes ADR-0029's depth-guard policy and records why guards were
//! tried first and abandoned. wasm builds must set an 8 MiB stack
//! (`-C link-arg=-zstack-size=8388608`); see `bd tsr-el3.3`.

/// Stack headroom below which a new segment is allocated.
///
/// 100 KiB, matching rustc's `ensure_sufficient_stack`. The value only has to
/// exceed the largest single frame between two checks; the deepest measured here
/// is the binder's, at roughly 800 bytes per level in debug.
#[cfg(not(target_arch = "wasm32"))]
const RED_ZONE: usize = 100 * 1024;

/// How much stack to allocate when the red zone is reached.
///
/// 16 MiB, again matching rustc. Larger than the red zone by enough that
/// allocation is rare: a 5,000-deep binder walk costs about 4 MiB in debug, so
/// most pathological inputs never trigger a second segment.
#[cfg(not(target_arch = "wasm32"))]
const STACK_GROWTH: usize = 16 * 1024 * 1024;

/// Run `f`, growing the stack first if little room is left.
///
/// Call this at the entry to a recursive tree walk — once per level, not once per
/// walk. The check is a pointer comparison when there is room, which is the
/// overwhelmingly common case; allocation happens only on genuinely deep input.
///
/// On wasm32 this is exactly `f()`: see the module docs.
#[inline]
pub fn ensure_sufficient<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(not(target_arch = "wasm32"))]
    {
        stacker::maybe_grow(RED_ZONE, STACK_GROWTH, f)
    }
    #[cfg(target_arch = "wasm32")]
    {
        f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_recursion_survives_a_small_thread_stack() {
        // The property the whole module exists for, stated as a test: recursion far
        // past what the thread was given must return rather than abort. 1 MiB is
        // wasm-ld's default and well under what 200,000 frames need.
        fn descend(n: u32) -> u32 {
            if n == 0 {
                return 0;
            }
            ensure_sufficient(|| descend(n - 1) + 1)
        }

        let handle = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(|| descend(200_000))
            .expect("spawn");
        assert_eq!(handle.join().expect("no overflow"), 200_000);
    }
}
