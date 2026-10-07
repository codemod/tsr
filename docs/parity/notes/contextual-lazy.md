# Lazy object-literal accessors: tsr-2zk.11.5

## Status and identities

Investigation at TSR `5dd3bad84d12991e1ba169d2d5687321e1989740`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. No checker implementation changed:
the faithful cutover crosses files outside this lane's ownership. No converted
cases, optimization claim, or release acceptance claim.

The fresh Box built TSR with the installed pinned Rust 1.96.0 and native with
Go 1.26.8; offline bootstrap was unnecessary. Frozen release binary SHA-256:
`866e1c6ebe89bf98af5cd20e04fb44b93bb4f73a4cd3d84566c6021c0893211b`.
Native binary SHA-256:
`7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`.
Temporary binary, verdict and probe artifacts are under `/tmp/box`; they are
not durable repository artifacts and will disappear with the Box.

Before edits, both unfiltered verdict dumps completed:

| Dump | Measured population | Verdicts |
| --- | --- | --- |
| Type assertions | 477,970 aligned rows | 469,765 RIGHT; 7,212 WRONG; 993 GAP |
| Diagnostic cases | 10,570 rows | 4,221 RIGHT; 4,968 EMPTY_RIGHT; 1,281 WRONG; 100 EMPTY_WRONG |

The type output file has 477,976 physical lines, including six non-verdict
lines. Grouping assertion-bearing cases gives 8,093 entirely RIGHT cases out
of 9,535; this is **not** the suite's case denominator: zero-assertion cases do
not appear in that grouping. Neither dump verifies complete diagnostic
messages, lengths, order, variants, or native complete-input checking.

## Direct reproduction

This source was run through pinned native `--declaration --emitDeclarationOnly
--strict --pretty false` and the existing TSR `probefile` example (real corpus
pipeline with bundled libraries):

```ts
function make() {
    return { get self() { return make(); } };
}
const result = make();
const distinct = {
    get value(): string { return "x"; },
    set value(v: number) {}
};
const computed = { get ["value"]() { return 1; } };
const recursive = { get self() { return recursive; } };
const illegal = { get self() { return illegal.self; } };
```

Native declaration output for `make` is:

```ts
declare function make(): {
    readonly self: {
        readonly self: /*elided*/ any;
    };
};
```

TSR's type walker reports `make : () => any`, its object literal as `error`,
and `result : any`. This directly supports the eager-publication hypothesis;
it is not inferred from historical target counts.

Native reports exactly one diagnostic in this control:

```text
../../tmp/box/accessor-controls.ts(8,23): error TS7023: 'self' implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.
```

The actual scratch source wrote the divergent pair on one line; the diagnostic
above preserves its observed path and location, rather than the expanded
source's location. TSR CLI reported no diagnostic. TSR's
walker reports the illegal member's type as `error`; native emits `any`.

Distinct controls: both preserve `get value(): string; set value(v: number);`
for the divergent pair and `readonly value: number` for the computed getter.
The legal variable-self reference must remain distinct from the illegal
accessor-self reference.

The current corpus names `compiler/noCircularitySelfReferentialGetter3` and
`compiler/noCircularitySelfReferentialGetter4` are verified. Both are already
fully RIGHT (9/9 and 23/23 type lines), and both are EMPTY_RIGHT diagnostics.
Pinned native checks with `--noEmit --strict --target es2015 --pretty false`
exit 0 with empty output. They are regression controls, not conversions.
The other 41 assigned names are present in the frozen type dump and have
non-RIGHT lines; no causal attribution of those lines to lazy accessors has
been established.

## Native algorithm and publication boundary

Ported counterpart to implement: native `checkObjectLiteral`
(`internal/checker/checker.go`) retains the accessor's member symbol and calls
`checkNodeDeferred`; it does **not** obtain its read type while collecting the
literal. Native `getWidenedProperty` returns a non-Property symbol unchanged,
so accessor return types are not forced or widened again by that operation.
`getTypeOfAccessors` performs annotation precedence (getter, setter,
auto-accessor), then getter-body inference, inside the symbol resolution
frame. `isPropertySymbolTypeRelated` (`internal/checker/relater.go`) reads
member types when the relation requires them. Node-builder
`addPropertyToElementList` (`internal/checker/nodebuilderimpl.go`) obtains the
read/write types and only then decides whether to serialize separate accessor
signatures.

The existing `PropertySlot::Accessor(SymbolId)` belongs to one private TSR
Checker's anonymous member image; its SymbolId refers to that Checker's
borrowed Program binder store, not a printed name or a cross-Program id.
`symbol_types` owns the resolved TypeId for the Checker lifetime. Repeated
property reads must reuse that completion rather than copy a second answer.
Relevant options include `noImplicitAny`/strict and contextual check mode;
setter read/write types and concrete receiver instantiation must remain
separate. A merged computed accessor needs native member-symbol identity,
not just an equal computed-name string.

Publication states:

- Absent: retain symbol without forcing a signature, body, or read/write type.
- Active: resolution-stack entry is provisional, not completed success.
- Completed success: `getTypeOfAccessors` publishes its inferred/annotated type.
- Completed circular failure: native publishes `any` and emits the applicable
  circularity diagnostic, not an absent entry or TSR's current `error` result.
- Unsupported: a missing canonical computed member cannot be certified as
  an ordinary bound accessor; do not silently substitute an eager read.

Preserve declaration origin, literal member order, getter/setter pairing,
setter parameter spelling, diagnostic file/span, and instantiated receiver
context. No new semantic cache or reuse surface is proposed.

The expensive workers are `getTypeOfAccessors` and getter-body/signature
inference, reached by relation, index construction, and serialization reads.
Actual executions, active repeats, completed hits and copy bytes were not
instrumented. Integrator Beads follow-up request: record this bounded work
attribution before extending accessor reuse; use tsr-1yb.11/tsr-1yb.11.1 and
tsr-2zk.11.5 as the owning context. No performance ratio was measured.

## Required serialized integration changes

These are prerequisites, not patches applied by this owner.

1. **`printing.rs`: `certified_object_literal_text_at`,
   `deferred_accessor_text_at`, and the structural member plan.** Current lazy
   branches only replace `Member::Property.printed`; they cannot expand one
   retained symbol to native divergent get/set declarations at serialization.
   Move the read/write comparison and signature rendering out of literal
   construction and into node-builder-equivalent member serialization. Compare
   semantic types as native does, not printed equality. Preserve native
   member order and recursion visitation. `distinct` above proves a necessary
   distinct control. No Diagnostic API cutover is needed for this step.
2. **`symbols.rs`: `get_type_of_accessors` and
   `report_accessor_circularity`.** Native failed pop publishes `any`; TSR
   currently inserts `intrinsics.error`. Port TS7023 for the unannotated getter
   under `noImplicitAny`, full native message and accessor span; retain the
   annotated TS2502 precedence. The current comment explicitly defers TS7023
   because eager object-literal resolution forms false cycles. Apply together
   with unconditional laziness, not ahead of it. `illegal` is the positive
   diagnostic control; `recursive` and the two named corpus cases must remain
   diagnostic-free.
3. **`symbols.rs`: `accessor_write_parameter` and computed accessor resolution;
   canonical member access owner as required.**
   `capture_checked_object_member` currently calls `accessor_write_parameter`,
   which calls `get_signature_from_declaration(setter)` during construction.
   Retain declaration/symbol identity and defer that signature extraction too.
   The current computed accessor path uses declaration-local binder symbols
   and sibling reconstruction; provide a canonical merged symbol access path
   corresponding to native late-bound member preparation before admitting
   computed accessor slots. Do not add string-keyed speculative completion.
4. **Owned `objects.rs`, after those prerequisites:** remove resolution-stack
   and annotation conditions from accessor deferral; publish accessor symbols
   for all supported accessor names, without eager `get_type_of_symbol`,
   `get_signature_from_declaration`, `member_text_at`, or setter-signature
   extraction. The getter/setter constructor branches currently perform
   eager comparison for their baked display. Remove that obsolete producer
   work after the serializer handles it. `object_literal_indexes` may force
   a contributing index member where native does; laziness does not authorize
   skipping semantically required consumers.
5. **Owned `widening.rs`:** retain native non-Property accessor identity without
   forcing it; its existing `reads_on_demand` skip already supports a lazy
   slot. Recheck union sibling/context reads rather than asserting that all
   widening consumers are lazy.

Full coverage was not run: `coverage` unconditionally writes all snapshots,
which this owner's contract forbids touching. No workspace tests, lint/fmt,
after-verdict comparison, or performance acceptance run was performed for
this documentation-only investigation. The integrator must run the strict
full-population oracle and release gates after the atomic cutover. No
unverified acceptance flags or partial checker implementation were committed.
