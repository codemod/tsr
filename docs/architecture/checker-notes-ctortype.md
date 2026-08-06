# Constructor type nodes — `new (x: T) => U` in type position

`bd tsr-jril`. Companion to `crates/tsr-checker/src/function_types.rs`, which
ports the *function*-type half of the same upstream function and states, in a
committed comment, why the constructor half was left out.

This page is written in two halves and the first was committed **before any
checker code existed**: §1–§3 are the sizing and the pre-registered keep/revert
bar; §4 is the measurement scored against it.

---

## 1. What was refused, and what the refusal actually said

`signatures.rs`'s `signature_parts_of` has carried this since the function-type
arm landed:

> **`ConstructorTypeNode` is deliberately not folded in here**, even though
> upstream builds its signature through the same function and the parts would
> line up. The two diverge in the *printer*: a construct signature emits
> `ast.KindConstructorType` (`nodebuilderimpl.go:2712`), which prints
> `new (x: T) => U`, and `abstract new (x: T) => U` when the declaration carries
> the `abstract` modifier that `FunctionTypeNode` cannot have. `Signature` has no
> construct flag and `Checker::signature_to_string` always emits the call form,
> so adding the arm alone would print every one of the corpus's 523
> constructor-type lines without its `new` — a wrong answer on all of them
> rather than a gap. It needs a `construct` flag on `Signature`, the
> `new `/`abstract new ` prefix, and a test per spelling; that is a slice, not an
> arm.

That refusal is **correct and checkable against upstream**, and it was checked
rather than trusted — the failure this project records twice is a doc comment
asserting what upstream requires and never being tested. Grepped on the
declarations at the pinned commit:

| upstream | what it does |
|---|---|
| `checker.go:19902` | `getSignatureFromDeclaration` sets `SignatureFlagsConstruct` for `IsConstructorTypeNode` — the *same* function that builds a function type's signature |
| `checker.go:19905` | and `SignatureFlagsAbstract` when the constructor type node carries `ModifierFlagsAbstract` |
| `nodebuilderimpl.go:2712` | a resolved type with exactly one construct signature and no call signature is rendered with `kind == ast.KindConstructorType` |
| `nodebuilderimpl.go:1834` | that kind, plus `SignatureFlagsAbstract`, is what puts the `abstract` modifier on the emitted node |
| `nodebuilderimpl.go:1886` | and the node built is a `ConstructorTypeNode`, whose printer emits `new ` |

So the flag lives on the **signature**, set from the declaration's kind and its
modifier, and it is read by the **printer**. The port's shape mirrors that: two
booleans on `Signature`, written where the parts are read and consumed where the
string is built. Nothing else about the construct differs — parameters,
optionality, rest, `this`, type parameters and the return annotation all reach
the identical upstream code.

## 2. The counterfactual — `examples/ctorgen.rs`

`depend.rs` at `1e40af4` sizes the `ConstructorType` root at **1,195 gap lines,
want-any 18 (1.5%), 72 cases, top-1 12.9%**. That is a **ceiling on a row**, and
this file's standing rule is that a row is not a mechanism. `ctorgen.rs` does the
*match* test instead: it computes the string the arm would print, from public
checker API over the same AST parts `signature_parts_of` would hand
`signature_to_string`, and compares it to the baseline character for character.

Run at `1e40af4`:

```
classified (gap line whose dependency root is a ConstructorType node): 1084

     821  CONVERTS — forecast matches the baseline exactly
     124  still a gap: a parameter annotation itself gaps
      75  MISS — forecast differs
      35  downstream — a lossy step lies between the line and the node
      17  still a gap: a parameter is a binding pattern
       7  still a gap: the return annotation itself gaps
       4  still a gap: a type parameter constraint itself gaps
       1  still a gap: a type parameter carries a modifier (const/in/out)

  C1 classified-but-not-gap: 0  (expect 0)
  C2 buckets sum 1084 vs classified 1084
  C3 root already types: 0  (expect 0)

spellings:  1083 `new`   |   1 `abstract new`
top cases:  154 subtypingWithConstructSignatures2 (14.2%), 100
            assignmentCompatWithConstructSignatures3, 90/86
            subtypingWithConstructSignatures3/4, …
```

**The classified figure is 1,084 and not 1,195** because the probe walks
`depend.rs`'s dependency edges itself and keeps only the lines whose root is a
`ConstructorTypeNode`; the ~111 difference is walk-order noise between two
instruments run at different points and is not claimed either way.

### The three controls, and why the third is the one that matters

- **C1** — every classified line answers `errorType` today. Arithmetic-free: it
  re-asks the checker rather than trusting the filter that selected the line.
  **0 violations.**
- **C2** — buckets sum to classified. **1,084 = 1,084.** This control is the weak
  one and is recorded as such: *an arithmetic control over a partition cannot see
  that the partition is wrong.*
- **C3** — `get_type_from_type_node` on every classified **root** must answer
  `error` today. **0 violations.** This is the control pinned to the thing being
  claimed rather than to a summary of it: the whole item rests on the assertion
  that constructor type nodes are unanswered, and if any of them already typed,
  the refusal in §1 would be about a population that does not exist.

### The population the forecast is allowed to claim

A root population includes lines reached by steps that do **not** preserve the
printed type — `type C = new () => X; var v: C` records `>v : C`, not the body.
The probe therefore classifies each step and forecasts only for chains made
entirely of type-preserving steps (a declaration's name to its annotation, an
identifier reference to its declaration's name). The 35 lines behind a lossy step
are claimed as **nothing**; if the build converts them, that is upside, not
forecast.

## 3. THE BAR — registered before any checker code exists

Measured by `examples/casedelta.rs` and `examples/wrongdelta.rs` at both ends of
a `git stash`, per §6 of `STATUS.md`.

> **KEEP only if all four legs pass:**
>
> 1. **Net floor — `net ≥ +700` lines.** Derived from the counterfactual's **821
>    exact string matches**, *not* from `depend.rs`'s 1,195 row, discounted 15%.
> 2. **Lost rule — `lost == 0`.**
> 3. **Case regression — `regressed < finished`.**
> 4. **Gap→wrong — `gained ≥ 8 × new wrong lines`** (`wrongdelta`, `comm -13`).
>
> **FALSIFIER: if more than 50% of the gain comes from a single case, the
> population was mis-sized** and the counterfactual's case spread — top-1 at
> 14.2% of 1,084 — was measuring something other than the mechanism.

### What input would make each leg non-zero

This is the question `docs/conventions.md` requires of every leg, because the
`&&` build shipped with a strongest-looking leg that had no denominator.

1. **Net floor.** It can fall short, and the concrete way is that the probe
   **reimplements** `signature_to_string` rather than calling it — it is
   `pub(crate)`. Optionality (`minArgumentCount`), the `this`-parameter split and
   the `written_text` node-reuse rule are all re-derived in the probe. A ≥15%
   shortfall is evidence that the reimplementation was not faithful, which is
   exactly what this leg is for.
2. **Lost.** An absolute zero rather than a ratio, on the rule that *when a
   change can only add, a loss is proof of a wrong rule and not of a bad trade*.
   The arm replaces `errorType` with an answer and never replaces an answer with
   a different one. It can still fire, and the route is named in advance: a
   dependency that stops being `error` changes what downstream code sees —
   `get_type_of_function_expression`'s `has_no_contextual_type` gate, union
   construction, and `instantiate_type` all behave differently once a constituent
   types. Any loss is to be read as a defect in the build, not priced against the
   gain.
3. **Case regression.** Cases are whole-baseline and positional; a case that
   finishes needs every line right. It fires if the arm converts lines in a case
   while breaking others in it.
4. **Gap→wrong.** The counterfactual reads **821 : 75 = 10.9 : 1**. The bar is
   set at 8:1, leaving headroom for the 35 unclaimed downstream lines and for the
   probe artifacts named below, and firing if the miss population turns out
   materially larger in the cascade than at the root.

### The wrong lines this build is expected to mint, named in advance

Reading the 75 misses **before** the build, so they are not discovered
post-hoc — the residual is the part of a build this project has twice found real
defects in:

| n | family | owner |
|---:|---|---|
| 59 | `want new (x: Array<Base>, …) => Array<Derived>`, forecast `new (x: Base[], …) => Derived[]` | `bd tsr-5o2` |
| 2 | `want new <S extends Schema>() => …`, forecast `new <S extends Record<string, unknown>>() => …` | `bd tsr-5o2` |
| 7 | the node denotes a **type alias** and the baseline prints the alias's name (`Constructor`, `Constructable`, `Foo`, `G1`, `T4`) | `getAliasForTypeNode`, shared with the function-type arm |
| 6 | probe artifacts — the walk stepped from a *function* declaration's name to its **return** annotation, so the line is a whole function signature and will convert, not miss | none; understatement |
| 1 | `want new () => boolean & null`, forecast `new () => (boolean) & null` | pre-existing intersection parenthesisation |

**The 61-line `tsr-5o2` family is not grounds to refuse the construct**, and the
reasoning is worth stating because this project's rule is *a gap beats a wrong
answer*. That rule is about a **sub-form of the construct** needing unported
machinery. This is not one: it is a defect of the *shared renderer* that the
`FunctionTypeNode` arm already ships today, on the identical inputs. The
baselines record the same shape without a `new` anywhere in it —

```
>arryFn2 : (x: Array<number>) => void          (5 instances)
>x352 : (n: Array<Base>) => void               (2)
>takeArray : (arr: Array<unknown>) => void     (2)
```

— and this port prints `(x: number[]) => void` for all of them today. Refusing
constructor types on this ground would imply reverting the function-type arm,
which nobody is proposing. `bd tsr-5o2` owns the family for both spellings, and
its size is now measured on a second population.

The 7 alias-named lines are the same argument in miniature: `getAliasForTypeNode`
is applied by `get_type_from_type_literal` and by the union arm, and **not** by
`get_type_from_function_type_node`. Extending it is a separate change, testable
on the function-type arm alone, and folding it into this slice would make the
measurement unattributable.

---

## 4. The measurement, scored against the bar

*(§4 is filled in after the build, at the same commit as the code.)*
