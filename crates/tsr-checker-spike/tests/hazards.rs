//! The failure modes each style admits, demonstrated rather than asserted.
//!
//! A design comparison that only lists risks in prose is easy to agree with and
//! easy to be wrong about. These tests make each hazard concrete: what the bad
//! code looks like, and what it does when run.

use std::cell::RefCell;

use tsr_checker_spike::{Program, Resolved, handles, refs_cell, refs_refcell};

/// The hazard that `RefCell`-guarded memo tables carry.
///
/// This reproduces, in miniature, the mistake the real checker invites: hold the
/// borrow while recursing. It compiles cleanly, passes review unless someone is
/// looking for it, and panics only on inputs that actually recurse.
#[test]
#[should_panic(expected = "already borrowed")]
fn holding_a_refcell_borrow_across_recursion_panics() {
    struct Recursive {
        memo: RefCell<Vec<Option<u32>>>,
    }

    impl Recursive {
        fn value(&self, i: usize) -> u32 {
            // The bug: the borrow lives to the end of the block, and `value`
            // below re-enters and takes `borrow_mut`.
            let memo = self.memo.borrow();
            if let Some(v) = memo[i] {
                return v;
            }
            let computed = if i == 0 { 1 } else { self.value(i - 1) + 1 };
            drop(memo);
            self.memo.borrow_mut()[i] = Some(computed);
            computed
        }
    }

    let r = Recursive { memo: RefCell::new(vec![None; 8]) };
    // `i == 0` never recurses, so a test that only covered the base case would
    // pass. The panic needs depth, which is exactly why this survives review.
    assert_eq!(r.value(0), 1);
    r.value(4);
}

/// The same shape with a `Cell` cannot be written wrongly: `get` copies out, so
/// there is no borrow to hold, and the compiler rejects the alternative.
#[test]
fn a_cell_memo_has_no_equivalent_hazard() {
    use std::cell::Cell;

    struct Recursive {
        memo: Vec<Cell<Option<u32>>>,
    }

    impl Recursive {
        fn value(&self, i: usize) -> u32 {
            if let Some(v) = self.memo[i].get() {
                return v;
            }
            let computed = if i == 0 { 1 } else { self.value(i - 1) + 1 };
            self.memo[i].set(Some(computed));
            computed
        }
    }

    let r = Recursive { memo: (0..8).map(|_| Cell::new(None)).collect() };
    assert_eq!(r.value(7), 8);
}

/// Every style survives a recursion depth well past anything a real declaration
/// chain reaches, and none of them silently blows the stack.
#[test]
fn deep_chains_do_not_overflow() {
    // TypeScript's own alias chains are short; 5,000 is far past realistic.
    let program = Program::chain(5_000);
    assert_eq!(handles::resolve_all(&program).len(), 5_000);
    assert_eq!(refs_refcell::resolve_all(&program).len(), 5_000);
    assert_eq!(refs_cell::resolve_all(&program).len(), 5_000);
}

/// A cycle is reported for every symbol that participates, not just the first.
///
/// The tempting optimisation — memoise the circular result — makes the second
/// symbol in a cycle silently resolve to a cached error rather than reporting its
/// own, which is a diagnostics regression no type test would catch.
#[test]
fn every_symbol_in_a_cycle_reports_it() {
    let program = Program::circular_pair();
    for resolved in handles::resolve_all(&program) {
        assert!(
            matches!(resolved, Resolved::Circular),
            "both symbols in the cycle should report circularity"
        );
    }
}
