# Reduced finally paths in flow types

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline: 80ba5796, 457,508/478,855 matching checker assertions.
Issue: bd tsr-8.5.

## The missing path distinction

Native bindTryStatement (internal/binder/binder.go:1986) connects the start of
a finally block to normal completion, exceptions and pending returns. References
inside finally must see all those paths. References after finally can be reached
only by normal completion. The binder represents the latter with a ReduceLabel
that temporarily replaces the pre-finally label's antecedents. This graph was
already present in the port.

The type walk skipped ReduceLabel even though the reachability walk handled it.
Consequently getTypeAtFlowBranchLabel joined an exception-before-assignment path
into a read after finally. This produced both excessive union constituents and
TS2454 on a variable assigned in try: an exception prevents the read from being
reached, but the skipped reduction made the checker consider that path.

Native getTypeAtFlowNode (internal/checker/flow.go:181) pushes the reduction onto
FlowState.reduceLabels, recursively evaluates its antecedent and pops the
reduction. getBranchLabelAntecedents reads the innermost matching replacement.
The port now follows this protocol, sharing antecedent selection with
isReachableFlowNodeWorker. The helper returns an iterator borrowing the graph,
so the mutable reduction stack is not borrowed across recursion. Single-path
junctions retain their allocation-free fast path.

## Alternatives and controls

Suppressing TS2454 after every try would hide genuinely unassigned catch paths
and would leave flow types wrong. Globally removing exception or return paths
from the binder would make reads inside finally unsound. Mutating the shared
graph during checking would replace per-query state with global state. The
native reduction stack preserves all three distinctions without a syntactic
exception for this project.

Pinned native declarations distinguish a number after normal completion from
string | number inside finally. They also retain number after nested finally
blocks and number | undefined for a function with a pending return. Native
diagnostics report TS2454 for an unassigned catch path and for a read inside
finally that an exception can reach before assignment.

Four added narrowing tests and three diagnostic tests cover those paths. Before
the implementation, three new narrowing tests fail while the inside-finally
control passes. The original CLI reproduction reports TS2454. The implemented
walker passes both type and diagnostic controls, including the two genuine
unassigned reads.

## Review and validation boundaries

Reuse, quality and efficiency review ran sequentially in the primary context
under the user's AGENTS override. The efficiency pass replaced temporary
antecedent vectors with graph iterators. Correctness and adversarial review
traced nested stack restoration, shared-flow cache lifetime, normal and caught
exception paths, pending returns and the native recursion-depth bound. No
independent or cross-model review is claimed.

The full-project CLI experiment on 80ba5796 was stopped after 16 minutes
25 seconds without diagnostics. Its sample places all 753 samples in recursive
conditional/mapped instantiation beneath class-signature construction. This
predates the finally change and is recorded on tsr-6.3; the earlier 196-error
report does not describe this newer compiler. Evidence is
/tmp/tsr-nextjs-main-80ba5796.log and
/tmp/tsr-nextjs-main-80ba5796-sample.txt. This unit does not complete recursive
conditional instantiation or the 99% objective.

## Measured landing

The committed checker at **04268474** matches **457,525/478,855 assertions
(95.55%)** and **7,007/9,538 complete cases (73.46%)**. Against 80ba5796,
all 17 changed aligned assertions are WRONG-to-RIGHT in
compiler/tryCatchFinallyControlFlow. There are no RIGHT losses, GAP transitions
or changed already-WRONG rows. Aligned totals are 474,243 rows: 457,525 RIGHT,
2,568 GAP and 14,150 WRONG. Diagnostics remain 2,784/5,488. The 99% objective
still requires 16,542 matching assertions.

A fresh unfiltered verdict dump on 04268474 is byte-identical to the verified
candidate. Evidence: /tmp/tsr-99-finally-before.tsv,
/tmp/tsr-99-finally-04268474.tsv and /tmp/tsr-99-finally-04268474.log.
The checker source and trace_case.rs hash (sorted paths and contents) is
3bb96ffa6772ba7f7c73463b13bad81efd97bf484c2cf00703fc1fd4629b5123.

Release workspace tests pass all 220 result blocks; workspace clippy with
all targets and -D warnings, formatting, 3,358 anchor references and 16,638
section citations pass. The issue-id gate still reports 190 historical
unresolved identifiers, tracked by tsr-10; the new tsr-8.5 reference resolves.
The reduced CLI reproduction removes the false TS2454 while retaining the
assignment-in-condition TS18047, now tracked by tsr-8.6. Native reports no
diagnostics for that combined reproduction.
