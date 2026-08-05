//! Circularity detection: the type resolution stack.
//!
//! Ported from `Checker.pushTypeResolution`, `popTypeResolution` and
//! `findResolutionCycleStartIndex` (`internal/checker/checker.go:18758`–`18794`).
//!
//! # Why a stack rather than a flag
//!
//! The obvious implementation is a "currently resolving" bit per symbol, and it
//! is wrong in a way that only shows on a cycle of length greater than one.
//! Upstream, on finding a cycle, marks **every frame from the cycle start
//! onward** as failed:
//!
//! ```go
//! for i := resolutionCycleStartIndex; i < len(c.typeResolutions); i++ {
//!     c.typeResolutions[i].result = false
//! }
//! ```
//!
//! So in `const a = b, b = c, c = a`, all three participants resolve to an error
//! — not only `a`, the one that closed the loop. A per-symbol flag gives the
//! symbol that closed the cycle an error and lets the others quietly succeed with
//! whatever half-built answer was on the stack, which is both wrong and
//! order-dependent.
//!
//! # What is not ported
//!
//! `findResolutionCycleStartIndex` also consults `typeResolutionHasProperty`,
//! which lets a resolution that has *already* been memoised short-circuit the
//! search. That is an optimisation over a stack that is at most a few deep in
//! practice, and it needs the memo of every property kind to exist first. Left
//! out deliberately, with the behaviour unchanged: without it the search simply
//! scans further, and a memoised entry never reaches the stack anyway because the
//! caller returns before pushing.

/// Which lazily-computed property of an entity is being resolved.
///
/// Upstream's `TypeSystemPropertyName`. Only the variants this port reaches are
/// listed; adding one means porting the code that resolves it, not inventing a
/// name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyName {
    /// `TypeSystemPropertyNameType` — the type of a symbol.
    Type,
}

/// One frame of the resolution stack.
#[derive(Debug, Clone, Copy)]
struct Resolution<K> {
    target: K,
    property: PropertyName,
    /// `false` once this frame is known to be part of a cycle.
    succeeded: bool,
}

/// The resolution stack, `Checker.typeResolutions`.
///
/// Generic in the key because upstream's is: `pushTypeResolution` takes a
/// `TypeSystemEntity`, which is a symbol for `TypeSystemPropertyNameType` and a
/// **type** for `TypeSystemPropertyNameResolvedTypeArguments`. The checker uses
/// `Resolutions<SymbolId>` today; the other keys arrive with the code that needs
/// them.
#[derive(Debug)]
pub struct Resolutions<K> {
    stack: Vec<Resolution<K>>,
}

impl<K> Default for Resolutions<K> {
    fn default() -> Self {
        Self { stack: Vec::new() }
    }
}

impl<K: Copy + PartialEq> Resolutions<K> {
    /// An empty stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How deep the stack is. For tests that assert it is left balanced.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Begin resolving `property` of `symbol`.
    ///
    /// Returns `false` if that is already in progress — a cycle — having first
    /// marked every frame from the cycle's start as failed, so that all of its
    /// participants report the circularity rather than only the one that closed
    /// it. The caller must **not** pop when this returns `false`; nothing was
    /// pushed.
    pub fn push(&mut self, target: K, property: PropertyName) -> bool {
        if let Some(start) =
            self.stack.iter().rposition(|r| r.target == target && r.property == property)
        {
            for frame in &mut self.stack[start..] {
                frame.succeeded = false;
            }
            return false;
        }
        self.stack.push(Resolution { target, property, succeeded: true });
        true
    }

    /// Finish the innermost resolution, reporting whether it was cycle-free.
    ///
    /// # Panics
    ///
    /// If the stack is empty, which means a [`Resolutions::push`] that returned
    /// `false` was popped anyway.
    pub fn pop(&mut self) -> bool {
        self.stack.pop().expect("popped a resolution that was never pushed").succeeded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resolution_that_does_not_recurse_succeeds() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.pop());
        assert_eq!(r.depth(), 0, "the stack must be left balanced");
    }

    #[test]
    fn nested_resolutions_of_different_symbols_both_succeed() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.push(1u32, PropertyName::Type));
        assert!(r.pop());
        assert!(r.pop());
    }

    #[test]
    fn a_direct_self_reference_is_a_cycle() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(!r.push(0u32, PropertyName::Type), "`const a = a` must not recurse forever");
        assert!(!r.pop(), "the frame it re-entered is now known to have failed");
    }

    #[test]
    fn every_participant_in_a_longer_cycle_fails_not_only_the_one_that_closed_it() {
        // `a -> b -> c -> a`. This is the case a per-symbol "in progress" flag
        // gets wrong: it would fail `a` and let `b` and `c` succeed with a
        // half-built answer, which is both incorrect and dependent on which
        // symbol was asked for first.
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.push(1u32, PropertyName::Type));
        assert!(r.push(2u32, PropertyName::Type));
        assert!(!r.push(0u32, PropertyName::Type), "closes the cycle");

        assert!(!r.pop(), "c is part of the cycle");
        assert!(!r.pop(), "b is part of the cycle");
        assert!(!r.pop(), "a is part of the cycle");
    }

    #[test]
    fn a_frame_outside_the_cycle_still_succeeds() {
        // `outer -> a -> b -> a`. `outer` is not a participant: the cycle starts
        // at `a`, so only frames from there on are marked. Marking the whole
        // stack would turn an unrelated enclosing resolution into an error.
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(9u32, PropertyName::Type)); // outer
        assert!(r.push(0u32, PropertyName::Type)); // a
        assert!(r.push(1u32, PropertyName::Type)); // b
        assert!(!r.push(0u32, PropertyName::Type));

        assert!(!r.pop(), "b is in the cycle");
        assert!(!r.pop(), "a is in the cycle");
        assert!(r.pop(), "outer merely contained it");
    }
}
