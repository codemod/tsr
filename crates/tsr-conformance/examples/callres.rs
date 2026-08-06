//! Call and `new` resolution: what the three call-shaped terminal rows are
//! actually waiting on.
//!
//! # The rows
//!
//! `docs/architecture/checker-notes-recvgap.md` §4 names three terminal
//! `gap_reason` strings as the call-shaped blockers behind the receiver-is-a-gap
//! population, measured at `7602d6b`:
//!
//! | terminal reason | lines |
//! |---|---:|
//! | `BLOCK_SCOPED_VARIABLE / VariableDeclaration / initialiser CallExpression` | 1,124 |
//! | `expression answered error: CallExpression` | 791 |
//! | `FUNCTION_SCOPED_VARIABLE / VariableDeclaration / initialiser NewExpression` | 526 |
//!
//! Those counts are the sub-population **reachable from a receiver-gap walk**.
//! This probe counts the same reasons over the **whole gradient**, which is the
//! larger and more honest denominator, and says so on every table. The unit is
//! an **assertion line** everywhere on this page.
//!
//! # The question, and the bucket that answers it
//!
//! A row population is a **ceiling on the row, never a conversion**
//! (`docs/conventions.md`). The question is not "how many lines carry this
//! reason" — that is already known — but *"if this port resolved calls better,
//! would these lines convert?"*. The row is named after a `CallExpression`, and
//! `docs/architecture/checker-notes-calls.md` already measured that the majority
//! of unresolvable callees are **not** a `calls.rs` defect: 33.9% have an `error`
//! receiver and 22.6% are identifiers whose symbol `get_type_of_symbol` cannot
//! type. So the discriminating question is one hop down: **does the blocking
//! call's callee already have a type?**
//!
//! If it does, the signature step is the only thing missing and `calls.rs` owns
//! the line. If it does not, the line is kind 2 — a chain whose next link is
//! somebody else's row — and building call resolution converts nothing.
//!
//! ## Pre-registered decision rule
//!
//! Written before the first run, on the most direct bucket this probe prints
//! rather than on a quantity derived from it (`docs/conventions.md`, *"pre-register
//! on the most direct bucket your instrument produces, not on a proxy"*):
//!
//! - **R1 (prerequisite, direct).** Build in `calls.rs` / `signatures.rs` only if
//!   **≥25%** of the three sub-rows' lines have a blocking call/`new` node whose
//!   **callee already has a non-`error` type** — bucket `callee has a type`.
//!   Below 25%, the row is kind 2, its size is a ceiling owned by other rows, and
//!   I refuse the build.
//! - **R2 (spellability, direct).** Of the lines R1 admits, **≥70%** of the
//!   *baseline's own* right-hand side for the blocked line must be
//!   **plain-shaped** — no `<`, `{`, `=>`, `[`, `|`, `&`, `(` and not `any`.
//!   A slice that resolves a type this port cannot name turns a gap into a
//!   *wrong* line, which is worse than leaving it alone; the same check killed a
//!   692-line item and a 1,618-line one.
//! - **R3 (concentration).** Any sub-row whose top-10 case share is ≥68% is sized
//!   by **naming its cases**, not by quoting its line count: such a row converts
//!   as a step function.
//!
//! Both R1 and R2 must fire for a build. R1 is a bucket this probe prints
//! literally; R2 is computed from the baseline text and not from any answer of
//! ours.
//!
//! # The cascade, both ways
//!
//! A fix converts its own row **plus** what the row was blocking, and the sign of
//! the collateral is a property of what the newly computable type can be *named*
//! (`docs/conventions.md`). So for the two `initialiser` rows this probe also
//! counts the **downstream** lines: gap lines whose receiver chain bottoms out at
//! one of the very symbols these rows block. Those lines are in the
//! `member name, …` / `property access, …` rows and are disjoint from these three
//! by the reason prefix — control C4 asserts that disjointness rather than
//! assuming it.
//!
//! # Controls, printed unconditionally
//!
//! - **C1 — the blocking node's kind is the kind the reason names.** For the two
//!   `initialiser` rows `gap_reason` prints `nodes.kind(initializer)`, and this
//!   probe navigates to that same initialiser by an independent route (symbol →
//!   `value_declaration` → `initializer_id`). A mismatch is **0** only if the two
//!   routes agree; it is not vacuous, because the navigation could land elsewhere.
//! - **C2 — where the reason says `/ VariableDeclaration /`, the blocking node's
//!   parent is one.** An initialiser's parent *is* its declaration, so this is
//!   pinned by the subject, and it fires on a navigation slip that C1 would miss
//!   (a `CallExpression` in the wrong place is still a `CallExpression`).
//!   **It fired on the first run, at 272**, because it was first written
//!   unconditionally and these rows also carry `PropertyAssignment`,
//!   `PropertyDeclaration` and `Parameter` initialisers. The defect was in the
//!   control's scope, not in the navigation; it was fixed by reading the
//!   declaration kind out of the reason rather than by loosening the test.
//! - **C3 — the `expression answered error:` rows' line node carries the kind
//!   the row is named after.** `gap_reason` prints that kind from
//!   `nodes.kind(id)`, so it is pinned by the subject and fires if `row_of` and
//!   the node table disagree about what the line is.
//! - **C4 — no downstream line is itself in one of these rows.** The row reasons
//!   start `reference, ` / `declaration name, ` / `expression answered error: `
//!   and the downstream ones start `member name, ` / `property access, `; these
//!   prefixes are disjoint in `gap_reason` by construction.
//! - **C5 — the four rows are mutually exclusive.** `/ initialiser X` and
//!   `expression answered error: X` cannot both be substrings of one reason.
//! - **A1 — the buckets partition the rows.** Arithmetic. Every arm of
//!   [`Blocked`] carries a positive test; there is no `else` holding a
//!   semantically loaded label.
//!
//! # R2 was a shape test, and this page's refusal rested on it
//!
//! Added 2026-08-06, after `3f140c2` made *"spellability is a match test, not a
//! shape test"* a convention — a correction that came out of this workstream's
//! own `ArrowFunction` measurement, where the shape test read **99.6%** and the
//! match test read **17.8%**.
//!
//! **R2 above is the shape test.** The question is whether its 37.7% still bounds
//! the truth. It does **not**, and the answer is a property of the predicate
//! rather than of the corpus, so it is settled by reading
//! [`spell_of`] rather than by measuring:
//!
//! `Spell::Structural` rejects every right-hand side containing `<`, `{`, `[`,
//! `|`, `&`, `(` or `=>`. **Those are exactly the shapes
//! `printing::type_to_string` exists to print**: it renders `TypeData::Union`,
//! `TypeData::Anonymous` and `TypeData::Named` from a stored `text`
//! (`crates/tsr-checker/src/printing.rs:45`), so `string | number`,
//! `{ a: string; }`, `() => void`, `string[]` and `Promise<number>` are all
//! rendered by construction. `Promise<boolean>` appears in this probe's own
//! callee-reason histogram as a type this port **built and printed**.
//!
//! So `spell_of` is a *"is this a bare name"* test wearing a spellability label.
//! It errs in **both** directions:
//!
//! - it rejects `string[]`, `A | B`, `Promise<number>` — which this port prints;
//! - it accepts `unique symbol` (276 admitted lines) — which this port cannot
//!   produce, as §5 already recorded.
//!
//! **Therefore 37.7% is not an upper bound, not a lower bound, and not a bound.
//! The refusal's second leg has to be re-derived.**
//!
//! ## R2′, pre-registered before it was computed
//!
//! An exact match against the baseline needs the build, and the build is call
//! resolution. What is affordable without it is the **necessary** condition, by
//! exact string equality and no heuristic:
//!
//! > **R2′ — the vocabulary test.** For each admitted line, is the baseline's
//! > right-hand side a string this port **demonstrably renders elsewhere in the
//! > same case**? If the port never produces that exact string anywhere in the
//! > program, it cannot produce it here.
//!
//! That is a strict **upper bound** on the achievable match rate, and it replaces
//! a proxy with a string comparison.
//!
//! - **R2′ < 70%** → the refusal is confirmed on the corrected instrument, and
//!   this page's verdict stands for the reason it claimed rather than by luck.
//! - **R2′ ≥ 70%** → the shape test materially misled, the refusal does not stand
//!   on its stated grounds, and the row has to be re-derived with a
//!   counterfactual.
//!
//! 70% is the threshold R2 was registered at, kept deliberately so the two are
//! comparable.
//!
//! **Registered with it, and independent of the number:** R2′ is *necessary and
//! not sufficient* — a line can be in the vocabulary and still be computed
//! wrongly — so **no value of R2′ licenses a build this round.** A build needs a
//! counterfactual, and under `c592d0f` it also needs the collateral of whatever
//! half of the mechanism stays unported.
//!
//! Run: `cargo run --release -p tsr-conformance --example callres`

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The phrase the downstream rows are named after, verbatim from
/// `types_producer::access_reason`.
const RECEIVER_GAP: &str = "the receiver is a gap";

/// The four call-shaped terminal reasons. Three are the assignment; `ExprNew` is
/// its companion, printed because leaving it out would make the `new` half look
/// smaller than it is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Row {
    /// `var x = f(); …x…` — the symbol has no type and the half that stopped is
    /// a call initialiser.
    InitCall,
    /// `var x = new C(); …x…`.
    InitNew,
    /// The line **is** a call: `f().foo` puts `f()` here.
    ExprCall,
    /// The line **is** a `new`.
    ExprNew,
}

impl Row {
    const ALL: [Self; 4] = [Self::InitCall, Self::InitNew, Self::ExprCall, Self::ExprNew];

    const fn label(self) -> &'static str {
        match self {
            Self::InitCall => "… / initialiser CallExpression      (assigned)",
            Self::InitNew => "… / initialiser NewExpression       (assigned)",
            Self::ExprCall => "expression answered error: CallExpression  (assigned)",
            Self::ExprNew => "expression answered error: NewExpression   (companion)",
        }
    }

    /// Whether the row is one of the three the assignment names.
    const fn assigned(self) -> bool {
        !matches!(self, Self::ExprNew)
    }

    /// The node kind the blocking expression must have, for control C1.
    const fn blocking_kind(self) -> SyntaxKind {
        match self {
            Self::InitCall | Self::ExprCall => SyntaxKind::CallExpression,
            Self::InitNew | Self::ExprNew => SyntaxKind::NewExpression,
        }
    }

    /// Whether the blocking node is reached through a declaration's initialiser
    /// rather than being the line's own node.
    const fn via_initialiser(self) -> bool {
        matches!(self, Self::InitCall | Self::InitNew)
    }
}

/// Which row a `gap_reason` belongs to. Every arm is a positive substring test
/// and the four are mutually exclusive (control C5).
fn row_of(reason: &str) -> Option<Row> {
    if reason.contains("/ initialiser CallExpression") {
        Some(Row::InitCall)
    } else if reason.contains("/ initialiser NewExpression") {
        Some(Row::InitNew)
    } else if reason == "expression answered error: CallExpression" {
        Some(Row::ExprCall)
    } else if reason == "expression answered error: NewExpression" {
        Some(Row::ExprNew)
    } else {
        None
    }
}

/// Why the blocking call could not be resolved. Every arm has a positive test;
/// the semantically loaded one — [`Blocked::CalleeTyped`], the bucket the
/// decision rule is written on — is the **first** and requires the callee's type
/// to be something other than `error`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Blocked {
    /// The callee has a real type and the call still answered `error`. The
    /// prerequisite for signature resolution is **met**; `calls.rs` owns this.
    CalleeTyped,
    /// The callee is a name that resolves and whose symbol has no type.
    /// `get_type_of_symbol`'s row, not the call path's.
    CalleeIdentifierUntyped,
    /// The callee is a name that does not resolve at all.
    CalleeIdentifierUnresolved,
    /// The callee is `a.b` and `a` itself gaps.
    CalleeAccessReceiverGaps,
    /// The callee is `a.b`, `a` has a type, and `b` does not.
    CalleeAccessMemberGaps,
    /// The callee gapped and is neither an identifier nor a property access —
    /// a call, a parenthesis, `this`, an element access.
    CalleeOtherForm,
    /// The call node carries no callee expression at all.
    NoCallee,
    /// The blocking node exists and is not a call or a `new`. A positive failure
    /// of the navigation, not a residue.
    BlockingNodeNotACall,
    /// No blocking node could be reached: the symbol, its value declaration or
    /// its initialiser was absent. Also a positive failure.
    NoBlockingNode,
}

impl Blocked {
    const fn label(self) -> &'static str {
        match self {
            Self::CalleeTyped => "callee HAS a type          <- calls.rs owns it (R1's bucket)",
            Self::CalleeIdentifierUntyped => "callee name resolves, symbol has no type",
            Self::CalleeIdentifierUnresolved => "callee name does not resolve",
            Self::CalleeAccessReceiverGaps => "callee is a.b, receiver gaps",
            Self::CalleeAccessMemberGaps => "callee is a.b, member gaps",
            Self::CalleeOtherForm => "callee is another expression form, gapped",
            Self::NoCallee => "call node has no callee",
            Self::BlockingNodeNotACall => "BLOCKING NODE IS NOT A CALL (walk failed)",
            Self::NoBlockingNode => "NO BLOCKING NODE (walk failed)",
        }
    }
}

/// Inside [`Blocked::CalleeTyped`], what kind of type the callee has.
///
/// This is **not** where the pre-registered rule lives — R1 is written on
/// `CalleeTyped` itself and stays there — but it is what says whether the
/// admitted lines are `calls.rs`'s work or somebody else's one level further in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum TypedShape {
    /// The callee is `any`. Upstream's call on an `any` callee answers `any`
    /// (`resolveUntypedCall`), so the line is *answerable* — and answering it
    /// means printing `any`, which every ranking on this project treats as the
    /// opposite of credit.
    Any,
    /// An anonymous object type: the only shape `resolve_call_signature` looks
    /// inside today. If the call still failed, `calls.rs`/`signatures.rs` owns it.
    Anonymous,
    /// A named type — a lib interface such as `SymbolConstructor`, or a class
    /// instance type. `resolve_call_signature` (`calls.rs:568`) destructures
    /// `TypeData::Anonymous` and returns `None` for everything else, so a
    /// `Named` callee never reaches signature lookup at all. Upstream reaches it
    /// through `getSignaturesOfType` (`checker.go:18959`) on the resolved
    /// members. `signature_parts_of` (`signatures.rs:834`) *does* already have
    /// `CallSignatureDeclaration` and `ConstructSignatureDeclaration` arms, so
    /// the missing link is the route from a `Named` type to its members, not the
    /// declaration reader — `bd tsr-qk9` is no longer the right citation here.
    Named,
    /// A union, an intersection, a literal, or a non-`any` intrinsic. Upstream
    /// mostly errors on calling these too.
    Other,
}

impl TypedShape {
    const fn label(self) -> &'static str {
        match self {
            Self::Any => "callee is `any`            -> upstream answers `any`; not credit",
            Self::Anonymous => "callee is an object type   -> calls.rs / signatures.rs own it",
            Self::Named => "callee is a Named type     -> never reaches resolve_call_signature",
            Self::Other => "callee is a union/literal/intrinsic",
        }
    }
}

/// How spellable the baseline's own right-hand side is, for R2.
///
/// The test is on the **baseline text**, never on anything this port answers, so
/// it cannot be moved by a change to the checker.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Spell {
    /// A bare name or keyword: `number`, `C`, `Foo`. This port can name these.
    Plain,
    /// Literally `any`. Upstream's own non-answer; converting to it is banned by
    /// the `errorType, never anyType` rule, so it is not credit.
    Any,
    /// Generic, structural, function-typed or unioned: `Promise<number>`,
    /// `{ a: string }`, `() => void`, `string[]`, `A | B`.
    Structural,
}

fn spell_of(rhs: &str) -> Spell {
    if rhs == "any" {
        return Spell::Any;
    }
    if rhs.contains(['<', '{', '[', '|', '&', '(']) || rhs.contains("=>") {
        return Spell::Structural;
    }
    Spell::Plain
}

/// A bucket's lines and the cases they came from, for concentration.
#[derive(Default)]
struct Tally {
    lines: usize,
    per_case: BTreeMap<String, usize>,
}

impl Tally {
    fn add(&mut self, case: &str, n: usize) {
        self.lines += n;
        *self.per_case.entry(case.to_string()).or_default() += n;
    }

    fn merge(&mut self, other: &Self) {
        self.lines += other.lines;
        for (case, n) in &other.per_case {
            *self.per_case.entry(case.clone()).or_default() += n;
        }
    }

    fn top_cases(&self, n: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> =
            self.per_case.iter().map(|(case, n)| (case.clone(), *n)).collect();
        entries.sort_by_key(|(case, n)| (std::cmp::Reverse(*n), case.clone()));
        entries.truncate(n);
        entries
    }

    /// `(cases, top-1, top-10)` as shares of `lines`.
    #[allow(clippy::cast_precision_loss)]
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.per_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let total = self.lines.max(1) as f64;
        let top1 = counts.first().copied().unwrap_or(0) as f64 / total * 100.0;
        let top10 = counts.iter().take(10).sum::<usize>() as f64 / total * 100.0;
        (counts.len(), top1, top10)
    }
}

#[derive(Default)]
struct Report {
    right: usize,
    gap: usize,
    wrong: usize,
    /// The four rows, whole-gradient.
    rows: BTreeMap<Row, Tally>,
    /// The verbatim reason inside each row, so the assignment's three exact
    /// strings can be reconciled against their families.
    verbatim: BTreeMap<(Row, String), Tally>,
    /// The partition: why the blocking call could not be resolved.
    blocked: BTreeMap<(Row, Blocked), Tally>,
    /// For `CalleeTyped`, what this port says the callee's type is.
    callee_types: BTreeMap<String, usize>,
    /// For `CalleeTyped`, the callee type's *shape*, which says who owns it.
    typed_shape: BTreeMap<(Row, TypedShape), Tally>,
    /// R2 taken per callee shape, so the gate can be read for each half
    /// separately rather than only over their mixture.
    shape_spelling: BTreeMap<(TypedShape, Spell), usize>,
    shape_rhs: BTreeMap<(TypedShape, String), usize>,
    /// The next link in the chain: the callee's own `gap_reason`, verbatim.
    callee_reasons: BTreeMap<String, Tally>,
    /// R2: the baseline's RHS for the blocked line, by shape and by row.
    spelling: BTreeMap<(Row, Blocked, Spell), usize>,
    /// The baseline's RHS verbatim, for the lines R1 admits.
    admitted_rhs: BTreeMap<String, usize>,
    /// The same, **keyed by row**, so R2′ can be read for the call half and the
    /// `new` half separately (`bd tsr-4tw`).
    ///
    /// The aggregate above is deliberately left untouched rather than derived
    /// from this: the published 69.4% has to stay reproducible from the same
    /// expression it was produced by, or the split becomes an unfalsifiable
    /// re-derivation of a number nobody can check.
    admitted_rhs_by_row: BTreeMap<(Row, String), usize>,
    /// `Row::ExprNew` is excluded from the admitted population by
    /// `row.assigned()`, so it has never been scored at all — and it is the
    /// 1,052-line own-root row `examples/armsplit.rs` measured. Tallied here on
    /// its own and reported as an **addition** to the population, never netted
    /// into the aggregate.
    expr_new_rhs: BTreeMap<String, usize>,
    /// Within the `new` rows, the baseline's RHS split by whether the callee's
    /// own type is a `*Constructor` interface. That is a property of the
    /// **callee**, which an implementation can see — unlike the baseline's
    /// right-hand side, which `docs/conventions.md` forbids slicing a population
    /// by.
    new_by_callee: BTreeMap<(bool, String), usize>,
    /// The same split, per **case**, because a rate is not a finding until its
    /// concentration is beside it (`docs/conventions.md`).
    new_by_callee_cases: BTreeMap<bool, BTreeMap<String, usize>>,
    /// The baseline's RHS verbatim for every line of the three assigned rows.
    row_rhs: BTreeMap<(Row, String), usize>,
    /// R2′: for each admitted line, whether the baseline's exact right-hand side
    /// is a string this port renders **somewhere in the same case**. Keyed by the
    /// shape bucket too, so the shape test's error can be read in both
    /// directions rather than asserted.
    vocabulary: BTreeMap<(Spell, bool), usize>,
    /// The stricter variant: rendered on a line we get **right**, so the string is
    /// not merely producible but produced correctly at least once.
    vocabulary_strict: BTreeMap<(Spell, bool), usize>,
    /// Admitted right-hand sides the port never renders in the case, verbatim.
    unrenderable: BTreeMap<String, usize>,
    /// Every type string this port renders **anywhere in the corpus**.
    ///
    /// The per-case vocabulary is too tight to be a clean upper bound: a case
    /// with no line answering `string` does not put `string` in its vocabulary,
    /// though the port obviously builds it. **Registered before it was computed:
    /// if this looser variant reads ≥70%, R2′ was measured on too tight an
    /// instrument and the refusal is NOT confirmed.** It is the weaker claim —
    /// the string is producible somewhere, not necessarily in this program — and
    /// so the honest upper bound.
    corpus_vocabulary: BTreeSet<String>,
    /// Cascade: gap lines whose receiver chain bottoms out at a symbol one of
    /// the `initialiser` rows blocks.
    downstream: BTreeMap<Row, Tally>,
    /// The baseline's RHS for those downstream lines — the collateral's
    /// spellability, which is where the last two measured items went negative.
    downstream_spelling: BTreeMap<Spell, usize>,
    c1_kind_mismatch: usize,
    c1_kind_match: usize,
    c2_parent_not_variable_declaration: usize,
    c2_parent_is_variable_declaration: usize,
    c3_expression_row_moved: usize,
    c4_downstream_in_a_row: usize,
    c5_multi_row: usize,
}

impl Report {
    #[allow(clippy::too_many_lines)]
    fn merge(&mut self, other: &Self) {
        self.right += other.right;
        self.gap += other.gap;
        self.wrong += other.wrong;
        for (key, tally) in &other.rows {
            self.rows.entry(*key).or_default().merge(tally);
        }
        for (key, tally) in &other.verbatim {
            self.verbatim.entry(key.clone()).or_default().merge(tally);
        }
        for (key, tally) in &other.blocked {
            self.blocked.entry(*key).or_default().merge(tally);
        }
        for (key, n) in &other.callee_types {
            *self.callee_types.entry(key.clone()).or_default() += n;
        }
        for (key, tally) in &other.typed_shape {
            self.typed_shape.entry(*key).or_default().merge(tally);
        }
        for (key, n) in &other.shape_spelling {
            *self.shape_spelling.entry(*key).or_default() += n;
        }
        for (key, n) in &other.shape_rhs {
            *self.shape_rhs.entry(key.clone()).or_default() += n;
        }
        for (key, tally) in &other.callee_reasons {
            self.callee_reasons.entry(key.clone()).or_default().merge(tally);
        }
        for (key, n) in &other.spelling {
            *self.spelling.entry(*key).or_default() += n;
        }
        for (key, n) in &other.admitted_rhs {
            *self.admitted_rhs.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.row_rhs {
            *self.row_rhs.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.vocabulary {
            *self.vocabulary.entry(*key).or_default() += n;
        }
        for (key, n) in &other.vocabulary_strict {
            *self.vocabulary_strict.entry(*key).or_default() += n;
        }
        for (key, n) in &other.unrenderable {
            *self.unrenderable.entry(key.clone()).or_default() += n;
        }
        self.corpus_vocabulary.extend(other.corpus_vocabulary.iter().cloned());
        for (key, n) in &other.admitted_rhs_by_row {
            *self.admitted_rhs_by_row.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.expr_new_rhs {
            *self.expr_new_rhs.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.new_by_callee {
            *self.new_by_callee.entry(key.clone()).or_default() += n;
        }
        for (key, cases) in &other.new_by_callee_cases {
            let mine = self.new_by_callee_cases.entry(*key).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        for (key, tally) in &other.downstream {
            self.downstream.entry(*key).or_default().merge(tally);
        }
        for (key, n) in &other.downstream_spelling {
            *self.downstream_spelling.entry(*key).or_default() += n;
        }
        self.c1_kind_mismatch += other.c1_kind_mismatch;
        self.c1_kind_match += other.c1_kind_match;
        self.c2_parent_not_variable_declaration += other.c2_parent_not_variable_declaration;
        self.c2_parent_is_variable_declaration += other.c2_parent_is_variable_declaration;
        self.c3_expression_row_moved += other.c3_expression_row_moved;
        self.c4_downstream_in_a_row += other.c4_downstream_in_a_row;
        self.c5_multi_row += other.c5_multi_row;
    }
}

/// Unit tests of the classifiers, run before the corpus so a drift in either is
/// found in a second rather than in a corpus pass.
fn check_classifiers() {
    assert_eq!(
        row_of(
            "reference, symbol has no type: SymbolFlags(BLOCK_SCOPED_VARIABLE) / VariableDeclaration / initialiser CallExpression"
        ),
        Some(Row::InitCall)
    );
    assert_eq!(
        row_of(
            "declaration name, symbol has no type: SymbolFlags(FUNCTION_SCOPED_VARIABLE) / VariableDeclaration / initialiser NewExpression"
        ),
        Some(Row::InitNew)
    );
    assert_eq!(row_of("expression answered error: CallExpression"), Some(Row::ExprCall));
    assert_eq!(row_of("expression answered error: NewExpression"), Some(Row::ExprNew));
    // A call *inside* a bigger expression is not this row: the reason names the
    // outer node, so it belongs to whoever owns that node.
    assert_eq!(row_of("expression answered error: BinaryExpression PlusToken"), None);
    assert_eq!(row_of("property access, the receiver is a gap: CallExpression"), None);
    assert_eq!(row_of("reference, the name does not resolve"), None);

    assert_eq!(spell_of("number"), Spell::Plain);
    assert_eq!(spell_of("C"), Spell::Plain);
    assert_eq!(spell_of("any"), Spell::Any);
    assert_eq!(spell_of("Promise<number>"), Spell::Structural);
    assert_eq!(spell_of("() => void"), Spell::Structural);
    assert_eq!(spell_of("string[]"), Spell::Structural);
    assert_eq!(spell_of("{ a: string; }"), Spell::Structural);
    assert_eq!(spell_of("A | B"), Spell::Structural);
}

fn main() {
    check_classifiers();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let reports: Vec<Report> = cases.par_iter().filter_map(measure).collect();
    let mut total = Report::default();
    for report in reports {
        total.merge(&report);
    }
    print(&total);
}

/// One gap line, kept from the first pass so the cascade pass does not have to
/// recompute `gap_reason`.
struct GapLine {
    position: usize,
    id: NodeId,
    reason: String,
}

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<Report> {
    // The suite's own skips, skip for skip.
    if case.has_varied_types() || case.has_known_divergence() {
        return None;
    }
    let text = case.expected_types()?;
    let expected = types_baseline::parse(&text);
    if types_baseline::assertion_count(&expected) == 0 {
        return None;
    }
    let parsed = case.load().ok()?;

    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
    let error = checker.intrinsics().error;

    let mut report = Report::default();
    let name = &case.name;

    // R2′'s vocabulary: every type string this port renders anywhere in the case,
    // and the subset it renders on a line it gets right. Built before the walk so
    // a line's own answer cannot be excluded — a type that only ever appears at
    // the line under test is still a type this port can build.
    let mut vocabulary: BTreeSet<String> = BTreeSet::new();
    let mut vocabulary_strict: BTreeSet<String> = BTreeSet::new();
    for (index, expected_file) in expected.iter().enumerate() {
        let Some(our_file) = ours.get(index) else { continue };
        for (position, assertion) in our_file.iter().enumerate() {
            if assertion.type_string == "error" {
                continue;
            }
            if expected_file.assertions.get(position).is_some_and(|b| b.text == assertion.line()) {
                vocabulary_strict.insert(assertion.type_string.clone());
            }
            vocabulary.insert(assertion.type_string.clone());
        }
    }
    report.corpus_vocabulary.clone_from(&vocabulary);

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }

        // The baseline's own right-hand side at a position, when our expression
        // text agrees with upstream's. `None` when the walkers disagree there,
        // which is not this probe's question.
        let rhs_at = |position: usize| -> Option<String> {
            let line = expected_file.assertions.get(position)?;
            let ours_text = our_file.get(position).map(|a| a.text.clone())?;
            line.text.strip_prefix(&format!("{ours_text} : ")).map(ToString::to_string)
        };

        // ---- pass 1: the rows themselves ------------------------------------
        let mut gap_lines: Vec<GapLine> = Vec::new();
        let mut blocked_symbols: BTreeSet<SymbolId> = BTreeSet::new();

        for (position, assertion) in our_file.iter().enumerate() {
            // **The baseline is asked first, and the order is the whole point.**
            // A line where we answer `error` *and upstream's baseline also says
            // `error`* is a **right** answer — a program may declare a type named
            // `error`, and some in the corpus do. `receiver_gap.rs` asked
            // `type_string == "error"` first and filed 389 such lines as gaps;
            // this probe was written from its shape and inherited the defect.
            // Corrected in `9b10272` upstream of here and in this commit for
            // this file; `crates/tsr-conformance/examples/reconcile.rs` is the
            // instrument that found it. `rank_board`, `wrong_attribution` and
            // `types_shapes` all test the baseline first and always did.
            let baseline = expected_file.assertions.get(position);
            if baseline.is_some_and(|b| b.text == assertion.line()) {
                report.right += 1;
                continue;
            } else if assertion.type_string == "error" {
                report.gap += 1;
            } else {
                report.wrong += 1;
                continue;
            }

            let id = line_ids[position];
            let reason = types_producer::gap_reason(&mut checker, bound, nodes, map, id);
            gap_lines.push(GapLine { position, id, reason: reason.clone() });

            // C5: the four row tests are mutually exclusive.
            let matches = usize::from(reason.contains("/ initialiser CallExpression"))
                + usize::from(reason.contains("/ initialiser NewExpression"))
                + usize::from(reason == "expression answered error: CallExpression")
                + usize::from(reason == "expression answered error: NewExpression");
            if matches > 1 {
                report.c5_multi_row += 1;
            }

            let Some(row) = row_of(&reason) else { continue };
            report.rows.entry(row).or_default().add(name, 1);
            report.verbatim.entry((row, reason.clone())).or_default().add(name, 1);
            if let Some(rhs) = rhs_at(position) {
                *report.row_rhs.entry((row, rhs)).or_default() += 1;
            }

            // The blocking node: the line's own node for the expression rows,
            // the declaration's initialiser for the `initialiser` rows.
            let (blocking, symbol) = if row.via_initialiser() {
                let symbol = symbol_of_line(bound, nodes, map, id);
                let initialiser = symbol
                    .and_then(|s| bound.symbols().get(s).value_declaration)
                    .and_then(|d| map.get(d))
                    .and_then(|d| d.initializer_id());
                (initialiser, symbol)
            } else {
                // C3: the reason is `expression answered error: {kind}` printed
                // from `nodes.kind(id)`, so the line's own node must carry the
                // kind the row is named after. It fires if `row_of` and the node
                // table ever disagree about what this line is.
                if nodes.kind(id) != row.blocking_kind() {
                    report.c3_expression_row_moved += 1;
                }
                (Some(id), None)
            };

            let (bucket, shape, callee_type) = match blocking {
                None => (Blocked::NoBlockingNode, None, None),
                Some(node) => {
                    if row.via_initialiser() {
                        if nodes.kind(node) == row.blocking_kind() {
                            report.c1_kind_match += 1;
                        } else {
                            report.c1_kind_mismatch += 1;
                        }
                        // C2 is conditioned on the reason *naming* a
                        // `VariableDeclaration`: these rows also carry
                        // `PropertyAssignment`, `PropertyDeclaration` and
                        // `Parameter` initialisers, and an unconditional test
                        // counted those as failures on the first run. Fixed by
                        // reading the reason rather than by loosening the test.
                        if reason.contains("/ VariableDeclaration /") {
                            if nodes
                                .parent(node)
                                .is_some_and(|p| nodes.kind(p) == SyntaxKind::VariableDeclaration)
                            {
                                report.c2_parent_is_variable_declaration += 1;
                            } else {
                                report.c2_parent_not_variable_declaration += 1;
                            }
                        }
                    }
                    classify(&mut checker, bound, nodes, map, node, error, row, &mut report, name)
                }
            };
            report.blocked.entry((row, bucket)).or_default().add(name, 1);

            if let Some(rhs) = rhs_at(position) {
                *report.spelling.entry((row, bucket, spell_of(&rhs))).or_default() += 1;
                if bucket == Blocked::CalleeTyped {
                    // `Row::ExprNew` on its own, because `row.assigned()` below
                    // excludes it from the aggregate and it has therefore never
                    // been scored. Reported as an addition, never netted in.
                    if row == Row::ExprNew {
                        *report.expr_new_rhs.entry(rhs.clone()).or_default() += 1;
                    }
                    if matches!(row, Row::InitNew | Row::ExprNew)
                        && let Some(callee) = callee_type.as_deref()
                    {
                        // A property of the **callee's type**, which an
                        // implementation can test. Not a slice by the baseline's
                        // expected answer, which `docs/conventions.md` forbids.
                        let is_ctor = callee.ends_with("Constructor");
                        *report.new_by_callee.entry((is_ctor, rhs.clone())).or_default() += 1;
                        *report
                            .new_by_callee_cases
                            .entry(is_ctor)
                            .or_default()
                            .entry(name.to_owned())
                            .or_default() += 1;
                    }
                }
                if bucket == Blocked::CalleeTyped && row.assigned() {
                    *report.admitted_rhs.entry(rhs.clone()).or_default() += 1;
                    *report.admitted_rhs_by_row.entry((row, rhs.clone())).or_default() += 1;
                    // R2′. Exact string equality against what this port renders,
                    // with no shape heuristic anywhere in the test.
                    let shape = spell_of(&rhs);
                    let renders = vocabulary.contains(&rhs);
                    *report.vocabulary.entry((shape, renders)).or_default() += 1;
                    *report
                        .vocabulary_strict
                        .entry((shape, vocabulary_strict.contains(&rhs)))
                        .or_default() += 1;
                    if !renders {
                        *report.unrenderable.entry(rhs.clone()).or_default() += 1;
                    }
                }
                if let Some(shape) = shape.filter(|_| row.assigned()) {
                    *report.shape_spelling.entry((shape, spell_of(&rhs))).or_default() += 1;
                    *report.shape_rhs.entry((shape, rhs)).or_default() += 1;
                }
            }
            if let Some(symbol) = symbol {
                blocked_symbols.insert(symbol);
            }
        }

        // ---- pass 2: the cascade --------------------------------------------
        // A gap line whose receiver chain bottoms out at a symbol one of the
        // `initialiser` rows blocks is a line those rows are holding down, and
        // it is in *another* row entirely (C4).
        if blocked_symbols.is_empty() {
            continue;
        }
        for line in &gap_lines {
            if !line.reason.contains(RECEIVER_GAP) {
                continue;
            }
            let Some(terminal) = terminal_receiver(&mut checker, bound, nodes, map, line.id) else {
                continue;
            };
            let Some(Node::Identifier(identifier)) = map.get(terminal) else { continue };
            let symbol = identifier.node_id.and_then(|n| {
                bound.resolve_name(nodes, map, n, identifier.text, SymbolFlags::VALUE)
            });
            let Some(symbol) = symbol.filter(|s| blocked_symbols.contains(s)) else { continue };
            if row_of(&line.reason).is_some() {
                report.c4_downstream_in_a_row += 1;
            }
            // Which of the two rows holds it — read off the symbol's own
            // initialiser kind rather than re-deriving the reason.
            let kind = bound
                .symbols()
                .get(symbol)
                .value_declaration
                .and_then(|d| map.get(d))
                .and_then(|d| d.initializer_id())
                .map(|i| nodes.kind(i));
            let row = match kind {
                Some(SyntaxKind::CallExpression) => Row::InitCall,
                Some(SyntaxKind::NewExpression) => Row::InitNew,
                _ => continue,
            };
            report.downstream.entry(row).or_default().add(name, 1);
            if let Some(rhs) = rhs_at(line.position) {
                *report.downstream_spelling.entry(spell_of(&rhs)).or_default() += 1;
            }
        }
    }
    Some(report)
}

/// The symbol a gap line's node refers to, mirroring `gap_reason`'s own order:
/// the declaration-name branch first, the identifier-reference branch second.
/// Any other order would attribute a line to a symbol the reason was not about.
fn symbol_of_line(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
) -> Option<SymbolId> {
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = bound.symbol_of(parent)
    {
        return Some(symbol);
    }
    let Some(Node::Identifier(identifier)) = map.get(id) else { return None };
    identifier
        .node_id
        .and_then(|n| bound.resolve_name(nodes, map, n, identifier.text, SymbolFlags::VALUE))
}

/// Descend a `a.b.c` receiver chain while the measured reason keeps saying the
/// receiver is the gap, and return the node the chain bottoms out at. The same
/// walk `receiver_gap.rs` performs; driven by the measured reason at each step.
fn terminal_receiver(
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
) -> Option<NodeId> {
    let mut access = if nodes.kind(id) == SyntaxKind::PropertyAccessExpression {
        id
    } else {
        nodes.parent(id).filter(|&p| nodes.kind(p) == SyntaxKind::PropertyAccessExpression)?
    };
    loop {
        let Some(Node::PropertyAccessExpression(node)) = map.get(access) else { return None };
        let receiver = node.expression.as_ref().and_then(tsr_ast::Expression::node_id)?;
        let reason = types_producer::gap_reason(checker, bound, nodes, map, receiver);
        if reason.contains(RECEIVER_GAP)
            && nodes.kind(receiver) == SyntaxKind::PropertyAccessExpression
        {
            access = receiver;
            continue;
        }
        return Some(receiver);
    }
}

/// Why the blocking call could not be resolved. One hop down from the row.
#[allow(clippy::too_many_arguments)]
fn classify(
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    blocking: NodeId,
    error: tsr_checker::TypeId,
    row: Row,
    report: &mut Report,
    case: &str,
) -> (Blocked, Option<TypedShape>, Option<String>) {
    let callee = match map.get(blocking) {
        Some(Node::CallExpression(node)) => node.expression,
        Some(Node::NewExpression(node)) => node.expression,
        Some(_) => return (Blocked::BlockingNodeNotACall, None, None),
        None => return (Blocked::NoBlockingNode, None, None),
    };
    let Some(callee) = callee else { return (Blocked::NoCallee, None, None) };
    let callee_type = checker.check_expression(callee);
    if callee_type != error {
        let printed = checker.type_to_string(callee_type);
        *report.callee_types.entry(printed.clone()).or_default() += 1;
        // `== intrinsics.any` by **identity**, never `TypeFlags::ANY`, because
        // `errorType` also carries `ANY` (`docs/architecture/checker-notes-arrays.md`,
        // and the guard `members.rs` holds for the same reason). `error` is
        // already excluded above, but the discipline is what keeps it excluded
        // if this branch ever moves.
        let shape = if callee_type == checker.intrinsics().any {
            TypedShape::Any
        } else {
            match checker.type_of(callee_type).data {
                tsr_checker::types::TypeData::Anonymous { .. } => TypedShape::Anonymous,
                tsr_checker::types::TypeData::Named { .. } => TypedShape::Named,
                _ => TypedShape::Other,
            }
        };
        report.typed_shape.entry((row, shape)).or_default().add(case, 1);
        return (Blocked::CalleeTyped, Some(shape), Some(printed));
    }
    let Some(callee_id) = callee.node_id() else { return (Blocked::NoCallee, None, None) };
    let reason = types_producer::gap_reason(checker, bound, nodes, map, callee_id);
    report.callee_reasons.entry(reason.clone()).or_default().add(case, 1);
    let bucket = match nodes.kind(callee_id) {
        SyntaxKind::Identifier => {
            if reason.contains("the name does not resolve") {
                Blocked::CalleeIdentifierUnresolved
            } else {
                Blocked::CalleeIdentifierUntyped
            }
        }
        SyntaxKind::PropertyAccessExpression => {
            if reason.contains(RECEIVER_GAP) {
                Blocked::CalleeAccessReceiverGaps
            } else {
                Blocked::CalleeAccessMemberGaps
            }
        }
        _ => Blocked::CalleeOtherForm,
    };
    (bucket, None, None)
}

#[allow(clippy::too_many_lines, clippy::cast_precision_loss, clippy::cast_possible_wrap)]
fn print(report: &Report) {
    let total = report.right + report.gap + report.wrong;
    println!("# callres — what the call-shaped terminal rows are waiting on\n");
    println!("The unit is an ASSERTION LINE everywhere on this page.\n");
    println!(
        "gradient: {} lines = right {} ({:.2}%) + gap {} ({:.2}%) + wrong {} ({:.2}%)\n",
        total,
        report.right,
        report.right as f64 / total.max(1) as f64 * 100.0,
        report.gap,
        report.gap as f64 / total.max(1) as f64 * 100.0,
        report.wrong,
        report.wrong as f64 / total.max(1) as f64 * 100.0,
    );

    println!("## 1. The rows, whole-gradient, with concentration\n");
    println!("{:<44} {:>7} {:>6} {:>8} {:>8}", "row (lines)", "lines", "cases", "top-1", "top-10");
    let mut assigned_total = 0usize;
    for row in Row::ALL {
        let Some(tally) = report.rows.get(&row) else { continue };
        let (cases, top1, top10) = tally.concentration();
        println!(
            "{:<44} {:>7} {:>6} {:>7.1}% {:>7.1}%",
            row.label(),
            tally.lines,
            cases,
            top1,
            top10
        );
        let named: Vec<String> =
            tally.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
        println!("      top: {}", named.join(" | "));
        if row.assigned() {
            assigned_total += tally.lines;
        }
    }
    println!("\nASSIGNED THREE = {assigned_total} lines\n");

    println!("## 2. The exact reason strings inside each row\n");
    let mut verbatim: Vec<_> = report.verbatim.iter().collect();
    verbatim.sort_by_key(|(_, tally)| std::cmp::Reverse(tally.lines));
    for ((_, reason), tally) in verbatim.iter().take(14) {
        let (cases, _, top10) = tally.concentration();
        println!("{:>6} {:>5}c {:>6.1}%t10  {}", tally.lines, cases, top10, reason);
    }
    println!();

    println!("## 3. R1 — why the blocking call could not be resolved\n");
    println!(
        "{:<58} {:>7} {:>6} {:>8}",
        "bucket (assigned three only)", "lines", "cases", "top-10"
    );
    let mut by_bucket: BTreeMap<Blocked, Tally> = BTreeMap::new();
    for ((row, bucket), tally) in &report.blocked {
        if row.assigned() {
            by_bucket.entry(*bucket).or_default().merge(tally);
        }
    }
    let mut partition_total = 0usize;
    for (bucket, tally) in &by_bucket {
        partition_total += tally.lines;
        let (cases, _, top10) = tally.concentration();
        println!("{:<58} {:>7} {:>6} {:>7.1}%", bucket.label(), tally.lines, cases, top10);
    }
    println!("{:<58} {:>7}", "PARTITION TOTAL", partition_total);
    let admitted = by_bucket.get(&Blocked::CalleeTyped).map_or(0, |t| t.lines);
    let r1 = admitted as f64 / partition_total.max(1) as f64 * 100.0;
    println!(
        "\nR1: callee HAS a type = {admitted} of {partition_total} = {r1:.1}%  (rule: build only if >= 25.0%) -> {}\n",
        if r1 >= 25.0 { "FIRES" } else { "does NOT fire" }
    );

    println!("### the same split, per row\n");
    for row in Row::ALL {
        let row_total: usize = report
            .blocked
            .iter()
            .filter(|((r, _), _)| *r == row)
            .map(|(_, tally)| tally.lines)
            .sum();
        if row_total == 0 {
            continue;
        }
        println!("  {} — {} lines", row.label(), row_total);
        for ((r, bucket), tally) in &report.blocked {
            if *r != row {
                continue;
            }
            println!(
                "      {:>6}  {:>5.1}%  {}",
                tally.lines,
                tally.lines as f64 / row_total as f64 * 100.0,
                bucket.label()
            );
        }
    }
    println!();

    println!("## 3b. Inside the admitted bucket: what shape the callee's type has\n");
    println!(
        "The pre-registered rule R1 is written on the bucket above and stays there.\n\
         This split says who owns the admitted lines one level further in.\n"
    );
    let mut by_shape: BTreeMap<TypedShape, Tally> = BTreeMap::new();
    for ((row, shape), tally) in &report.typed_shape {
        if row.assigned() {
            by_shape.entry(*shape).or_default().merge(tally);
        }
    }
    let shape_total: usize = by_shape.values().map(|t| t.lines).sum();
    for (shape, tally) in &by_shape {
        let (cases, _, top10) = tally.concentration();
        println!(
            "{:<58} {:>7} {:>5.1}% {:>5}c {:>6.1}%t10",
            shape.label(),
            tally.lines,
            tally.lines as f64 / shape_total.max(1) as f64 * 100.0,
            cases,
            top10
        );
        let named: Vec<String> =
            tally.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
        println!("      top: {}", named.join(" | "));
    }
    println!("{:<58} {:>7}\n", "ADMITTED TOTAL", shape_total);

    println!("## 4. What the callee's type is, where it has one\n");
    let mut callee_types: Vec<_> = report.callee_types.iter().collect();
    callee_types.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (text, n) in callee_types.iter().take(15) {
        println!("{n:>6}  {text}");
    }
    println!();

    println!("## 5. The next link in the chain — the callee's own gap_reason\n");
    let mut callee_reasons: Vec<_> = report.callee_reasons.iter().collect();
    callee_reasons.sort_by_key(|(_, tally)| std::cmp::Reverse(tally.lines));
    for (reason, tally) in callee_reasons.iter().take(15) {
        let (cases, _, top10) = tally.concentration();
        println!("{:>6} {:>5}c {:>6.1}%t10  {}", tally.lines, cases, top10, reason);
    }
    println!();

    println!("## 6. R2 — can this port spell the answer? (the BASELINE's own RHS)\n");
    let mut shapes: BTreeMap<Spell, usize> = BTreeMap::new();
    for ((row, bucket, spell), n) in &report.spelling {
        if row.assigned() && *bucket == Blocked::CalleeTyped {
            *shapes.entry(*spell).or_default() += n;
        }
    }
    let with_rhs: usize = shapes.values().sum();
    for (spell, n) in &shapes {
        println!(
            "  {:<12} {:>6}  {:>5.1}%",
            format!("{spell:?}"),
            n,
            *n as f64 / with_rhs.max(1) as f64 * 100.0
        );
    }
    let plain = shapes.get(&Spell::Plain).copied().unwrap_or(0);
    let r2 = plain as f64 / with_rhs.max(1) as f64 * 100.0;
    println!(
        "\nR2: plain-shaped = {plain} of {with_rhs} lines with a comparable baseline RHS = {r2:.1}%  (rule: >= 70.0%) -> {}\n",
        if r2 >= 70.0 { "FIRES" } else { "does NOT fire" }
    );
    println!("  the admitted lines' baseline RHS, verbatim:");
    let mut admitted_rhs: Vec<_> = report.admitted_rhs.iter().collect();
    admitted_rhs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (rhs, n) in admitted_rhs.iter().take(15) {
        println!("      {n:>5}  {rhs}");
    }
    println!();
    println!("  every assigned row's baseline RHS, verbatim (the ceiling, not the admitted half):");
    let mut row_rhs: BTreeMap<&String, usize> = BTreeMap::new();
    for ((row, rhs), n) in &report.row_rhs {
        if row.assigned() {
            *row_rhs.entry(rhs).or_default() += n;
        }
    }
    let mut row_rhs: Vec<_> = row_rhs.into_iter().collect();
    row_rhs.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (rhs, n) in row_rhs.iter().take(15) {
        println!("      {n:>5}  {rhs}");
    }
    println!();

    println!("### R2 per callee shape — the gate read for each half separately\n");
    for shape in [TypedShape::Any, TypedShape::Anonymous, TypedShape::Named, TypedShape::Other] {
        let total: usize =
            report.shape_spelling.iter().filter(|((s, _), _)| *s == shape).map(|(_, n)| *n).sum();
        if total == 0 {
            continue;
        }
        let plain = report.shape_spelling.get(&(shape, Spell::Plain)).copied().unwrap_or(0);
        println!(
            "  {:<58} plain {:>5} of {:>5} = {:>5.1}%",
            shape.label(),
            plain,
            total,
            plain as f64 / total as f64 * 100.0
        );
        let mut rhs: Vec<_> = report.shape_rhs.iter().filter(|((s, _), _)| *s == shape).collect();
        rhs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for ((_, text), n) in rhs.iter().take(8) {
            println!("        {n:>5}  {text}");
        }
    }
    println!();

    println!("## 6e. R2′ — the vocabulary test, by exact string equality\n");
    let renders: usize = report.vocabulary.iter().filter(|((_, r), _)| *r).map(|(_, n)| *n).sum();
    let total: usize = report.vocabulary.values().sum();
    let strict: usize =
        report.vocabulary_strict.iter().filter(|((_, r), _)| *r).map(|(_, n)| *n).sum();
    #[allow(clippy::cast_precision_loss)]
    let rate = renders as f64 / total.max(1) as f64 * 100.0;
    println!(
        "  the port renders this exact string somewhere in the case: {renders} of {total} = {rate:.1}%"
    );
    println!(
        "  the stricter variant (rendered on a line we get right):     {strict} of {total} = {:.1}%",
        strict as f64 / total.max(1) as f64 * 100.0
    );
    println!(
        "\n  R2′ (rule: refusal confirmed at < 70.0%) -> {}\n",
        if rate >= 70.0 {
            "the refusal does NOT stand on its stated grounds"
        } else {
            "REFUSAL CONFIRMED"
        }
    );
    println!("  cross-tab against the shape test, which is what says whether it misled:\n");
    println!("    {:<14} {:>10} {:>12} {:>8}", "shape", "renderable", "unrenderable", "total");
    for shape in [Spell::Plain, Spell::Any, Spell::Structural] {
        let yes = report.vocabulary.get(&(shape, true)).copied().unwrap_or(0);
        let no = report.vocabulary.get(&(shape, false)).copied().unwrap_or(0);
        if yes + no == 0 {
            continue;
        }
        println!("    {:<14} {:>10} {:>12} {:>8}", format!("{shape:?}"), yes, no, yes + no);
    }
    println!(
        "\n    the shape test called {} lines unspellable that this port renders,",
        report.vocabulary.get(&(Spell::Structural, true)).copied().unwrap_or(0)
    );
    println!(
        "    and {} lines spellable that it does not.",
        report.vocabulary.get(&(Spell::Plain, false)).copied().unwrap_or(0)
    );
    let corpus_hits: usize = report
        .admitted_rhs
        .iter()
        .filter(|(text, _)| report.corpus_vocabulary.contains(*text))
        .map(|(_, n)| *n)
        .sum();
    let admitted_total: usize = report.admitted_rhs.values().sum();
    #[allow(clippy::cast_precision_loss)]
    let corpus_rate = corpus_hits as f64 / admitted_total.max(1) as f64 * 100.0;
    println!(
        "  the looser variant — rendered ANYWHERE in the corpus: {corpus_hits} of {admitted_total} = {corpus_rate:.1}%"
    );
    println!(
        "    (registered in advance: >= 70.0% here means R2′ was measured on too tight an\n     instrument and the refusal is NOT confirmed) -> {}",
        if corpus_rate >= 70.0 { "NOT CONFIRMED" } else { "still confirmed" }
    );
    // ---- 6f. R2′ split, `bd tsr-4tw` -------------------------------------
    //
    // The aggregate above is untouched and stays reproducible from the same
    // expression that produced the published figure. Everything below is added
    // beside it.
    println!("\n## 6f. R2′ split — the call half and the `new` half, scored separately\n");
    println!("  Registered before this ran (`bd tsr-4tw`). The bar is the same 70%.");
    println!("  A rate on fewer than 300 lines licenses nothing and is marked.\n");

    let rate_over = |rows: &[Row]| -> (usize, usize) {
        let mut hits = 0usize;
        let mut total = 0usize;
        for ((row, text), n) in &report.admitted_rhs_by_row {
            if !rows.contains(row) {
                continue;
            }
            total += n;
            if report.corpus_vocabulary.contains(text) {
                hits += n;
            }
        }
        (hits, total)
    };

    println!("    half                              in corpus vocabulary          verdict");
    let mut split_rows: Vec<(&str, Vec<Row>)> = vec![
        ("CALL  (InitCall + ExprCall)", vec![Row::InitCall, Row::ExprCall]),
        ("NEW   (InitNew)", vec![Row::InitNew]),
        ("  of which InitCall", vec![Row::InitCall]),
        ("  of which ExprCall", vec![Row::ExprCall]),
    ];
    split_rows.push(("the aggregate, for comparison", Row::ALL.to_vec()));
    for (label, rows) in &split_rows {
        let (hits, total) = rate_over(rows);
        let rate = hits as f64 / total.max(1) as f64 * 100.0;
        let verdict = if total < 300 {
            "small denominator — licenses nothing"
        } else if rate >= 70.0 {
            ">= 70%  refusal NOT confirmed for this half"
        } else {
            "<  70%  refusal confirmed for this half"
        };
        println!("    {label:<33} {hits:>5} of {total:<5} = {rate:>5.1}%   {verdict}");
    }

    // `ExprNew` is outside the admitted population by construction, so it is
    // printed on its own and never folded into any figure above.
    let expr_new_hits: usize = report
        .expr_new_rhs
        .iter()
        .filter(|(text, _)| report.corpus_vocabulary.contains(*text))
        .map(|(_, n)| *n)
        .sum();
    let expr_new_total: usize = report.expr_new_rhs.values().sum();
    println!(
        "\n    ExprNew is EXCLUDED from every figure above by `row.assigned()` and has never\n    been scored. On its own: {expr_new_hits} of {expr_new_total} = {:.1}%",
        expr_new_hits as f64 / expr_new_total.max(1) as f64 * 100.0
    );

    println!("\n  Within the `new` rows, split by the CALLEE'S OWN TYPE — a property the");
    println!("  implementation can test, unlike the baseline's right-hand side:\n");
    for (is_ctor, label) in [(true, "callee is a `*Constructor`"), (false, "every other callee")] {
        let mut hits = 0usize;
        let mut total = 0usize;
        for ((ctor, text), n) in &report.new_by_callee {
            if *ctor != is_ctor {
                continue;
            }
            total += n;
            if report.corpus_vocabulary.contains(text) {
                hits += n;
            }
        }
        let rate = hits as f64 / total.max(1) as f64 * 100.0;
        let empty = BTreeMap::new();
        let cases = report.new_by_callee_cases.get(&is_ctor).unwrap_or(&empty);
        let (top_case, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or(("", &0), |(c, n)| (c.as_str(), n));
        let top1 = *top_n as f64 / total.max(1) as f64 * 100.0;
        println!(
            "    {label:<30} {hits:>5} of {total:<5} = {rate:>5.1}%   {:>4} cases, top-1 {top1:>5.1}%  {top_case}",
            cases.len()
        );
    }
    let mut ctor_rhs: Vec<_> = report.new_by_callee.iter().filter(|((c, _), _)| *c).collect();
    ctor_rhs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n    what the `*Constructor` lines want:");
    for ((_, text), n) in ctor_rhs.iter().take(10) {
        let renders =
            if report.corpus_vocabulary.contains(text) { "" } else { "<-- never rendered" };
        println!("      {n:>5}  {text:<34} {renders}");
    }
    println!();

    println!("\n  admitted right-hand sides the port never renders in the case:");
    let mut unrenderable: Vec<_> = report.unrenderable.iter().collect();
    unrenderable.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (text, n) in unrenderable.iter().take(12) {
        println!("      {n:>5}  {text}");
    }
    println!();

    println!("## 7. The cascade — lines the two `initialiser` rows hold down\n");
    let mut cascade_total = 0usize;
    for row in Row::ALL {
        let Some(tally) = report.downstream.get(&row) else { continue };
        let (cases, _, top10) = tally.concentration();
        let row_lines = report.rows.get(&row).map_or(0, |t| t.lines);
        cascade_total += tally.lines;
        println!(
            "  {:<44} downstream {:>6} over {:>4} cases (top-10 {:>5.1}%), row {:>5}, multiplier {:.2}x",
            row.label(),
            tally.lines,
            cases,
            top10,
            row_lines,
            (row_lines + tally.lines) as f64 / row_lines.max(1) as f64
        );
    }
    println!("  downstream total {cascade_total}");
    let downstream_with_rhs: usize = report.downstream_spelling.values().sum();
    for (spell, n) in &report.downstream_spelling {
        println!(
            "      {:<12} {:>6}  {:>5.1}%",
            format!("{spell:?}"),
            n,
            *n as f64 / downstream_with_rhs.max(1) as f64 * 100.0
        );
    }
    println!();

    println!("## 8. Controls\n");
    println!(
        "  C1  blocking node's kind != the kind the reason names = {} (must be 0)",
        report.c1_kind_mismatch
    );
    println!(
        "  C1' the mirror, kinds agreeing                        = {} (pins the arm)",
        report.c1_kind_match
    );
    println!(
        "  C2  blocking node's parent is not a VariableDeclaration = {} (must be 0, by the subject)",
        report.c2_parent_not_variable_declaration
    );
    println!(
        "  C2' the mirror                                        = {} (pins the arm)",
        report.c2_parent_is_variable_declaration
    );
    println!(
        "  C3  an `expression answered error:` row whose blocking node moved = {} (must be 0)",
        report.c3_expression_row_moved
    );
    println!(
        "  C4  a downstream line that is itself in one of these rows = {} (must be 0, prefixes disjoint)",
        report.c4_downstream_in_a_row
    );
    println!(
        "  C5  a reason matching more than one row                = {} (must be 0, tests disjoint)",
        report.c5_multi_row
    );
    let rows_total: usize =
        report.rows.iter().filter(|(row, _)| row.assigned()).map(|(_, t)| t.lines).sum();
    println!(
        "  A1  assigned rows {} == partition {} (difference {})",
        rows_total,
        partition_total,
        rows_total as i64 - partition_total as i64
    );
}
