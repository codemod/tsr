# The typed-receiver call: what the 557 answer once the blocker is gone

Status: demonstrated 2026-08-05 against `HEAD` at the time of writing, for
`bd tsr-gdy`. **No production code changed.** The deliverable for a kind-2 item
is a demonstration, not an estimate — `docs/conventions.md`, "For a
dependency-gated form, probe past the blocker, not at it".

The population and its concentration are measured in
[`checker-notes-calls.md`](checker-notes-calls.md) and are not re-derived here:
**557 call-expression nodes** (not assertion lines) where the receiver has a real
type, `get_property_of_type` finds the member, and the member types as `error`.
147 cases, top ten hold 49.9%.

## The question this page answers

`bd tsr-gdy` framed the probe as "give the member an anonymous object type with a
call signature and see whether the call line goes from `error` to a return type".
That framing is wrong, and the probe says so in its first line: **the member has
no type at all before any call is considered.** `p.get` is `error`; `p.get()`
being `error` is downstream of that and carries no information.

So the question was re-posed as the convention requires — *what does this form
answer once the blocker is removed?* — with the removal mocked by hand-writing
the instantiated declaration.

## The demonstration

Five fixtures, run through `Checker::check_expression` on the last expression
statement. No lib files are loaded, so **every fixture is written to touch no
lib global** — see the confound below, which cost two false findings before it
was caught.

```text
A   interface P<T> { get(): string; }  declare var p: P<number>;  p.get()   => error
A'  interface P    { get(): string; }  declare var p: P;          p.get()   => string
B   interface P<T> { get(): T; }       declare var p: P<number>;  p.get()   => error
B'  interface P    { get(): number; }  declare var p: P;          p.get()   => number
C   interface P<T> { get(): T; }       declare var p: P<number>;  p         => P<number>
```

Read the rows as three claims:

1. **C — the receiver types.** `P<number>` prints correctly. That is why these
   nodes land in the *typed receiver* row rather than the *receiver is error*
   row, and it is the fact the whole item rests on.
2. **A against A' — the blocker is the receiver's type arguments and nothing
   else.** The two differ in exactly one token. The member declaration, the
   member name, the call, and the return annotation are identical, and the
   return type contains no type parameter, so no substitution is required for
   `A` to succeed. It fails anyway.
3. **B against B' — removing the blocker is *sufficient*, not merely
   necessary.** `B'` is `B` with the instantiation performed by hand, `T := number`.
   It answers `number`, which is what `P<number>.get()` must answer. This is the
   step that a probe *at* the blocker cannot reach: it shows the call machinery
   downstream of a typed member — signature extraction, the single-candidate
   path, return-type printing — is live and already correct on this shape, so
   the next link in the chain is not a second blocker.

The complementary controls, same run, all answering correctly, which is what
makes the failure attributable to instantiation rather than to member calls in
general:

```text
interface C { from(o: number): number; }        c.from(1)  => number   method member
interface C { from: (o: number) => number; }    c.from(1)  => number   function-typed property
type    C = { from(o: number): number };        c.from(1)  => number   alias to a type literal
class   C { from(o: number): number {...} }     c.from(1)  => number   class instance member
interface C { from(o: number): Arr<number>; }   c.from(1)  => Arr<number>   generic *return*
interface C { from(o: number): Arr<object>; }   c.from(1)  => Arr<object>   constrained param
interface C { from(o: L<number>): string; }     c.from(..) => string   generic *parameter*
interface P { f<U>(x: U): string; }             p.f(1)     => string   generic *method*
interface C { from(o:number):string; from(o:string,f:number):string; }
                                                c.from(1)  => string   overloaded member
```

A generic return type, a generic parameter type, a generic method, and an
overloaded member all work. **The only shape that fails is a generic type
applied to the receiver.** That is a much narrower statement than "members of
interfaces are unported", and it is the one the fixtures support.

## What this does *not* show

`promiseType` + `promiseTypeStrictNull` are 97 of the 557 (17.4%) and are
`Promise<T>` members — the shape above, exactly. **`typedArrays` is the largest
single case at 54 (9.7%) and is not this shape.** Its calls are
`Int8Array.from(obj)` and `Int8Array.of(...obj)`, whose receiver is the
*non-generic* `Int8ArrayConstructor`. Those signatures reference `ArrayLike` and
`Array`, which are lib globals, so the no-lib probe harness cannot reproduce them
and this page makes no claim about them. Attributing the whole 557 to
instantiation on the strength of the Promise share would be the "a row named
after one case is about that case" error this project keeps writing down.

The honest split of the 557 by mechanism has **not** been measured. One more
counter at `crates/tsr-checker/src/calls.rs:416`, keyed on whether the receiver
type carries type arguments, would give it, and would say whether the
demonstrated fix is worth 17% of the row or most of it. Until it runs, the
demonstrated mechanism covers a measured 17.4% and an unmeasured remainder.

## The confound, recorded because it produced two false findings

Two earlier fixtures in this same probe read `error` and were briefly written
down as gaps:

```text
interface C { of(...i: number[]): string; }  c.of  => error   "rest parameters gap"
interface C { of(i: number[]): string; }     c.of  => error   "array parameters gap"
```

Both are artifacts. The probe harness builds a `Checker` over one parsed file and
loads no lib files, and `number[]` desugars to the lib global `Array<T>`. The
control that caught it is one line:

```text
var x: number[];  x  => error
```

An array type annotation fails *on its own* in this harness, so nothing
downstream of it can be attributed to parameters, rest tokens, or members. **In a
no-lib harness, any fixture mentioning a lib type is measuring the missing lib.**
The gradient does not have this problem — `types_producer::assertions_for_case`
loads the bundled libs (see the superseded header on
[`checker-notes-counters.md`](checker-notes-counters.md)) — which is precisely
why a hand-rolled probe and the gradient can disagree, and why every fixture in
the demonstration above is written to name no lib type.

## Consequence for the board

- `bd tsr-gdy`'s stated probe ("give the member a call signature") is **answered
  and rejected**: the member is `error` before the call is reached, so a call
  signature on it is not the missing piece.
- The blocker the demonstration names is **member lookup and member typing on a
  receiver whose named type carries type arguments** — instantiation, not
  callability. It is not `bd tsr-qk9` (`signature_parts_of` and
  `CallSignatureDeclaration`), which `checker-notes-calls.md` already ruled out
  on different grounds.
- The demonstration establishes sufficiency for its shape, which is the part a
  probe usually cannot: hand-instantiating the declaration makes the call answer
  correctly, including when the return type *is* the type parameter.
- **Ranking is unchanged by this page.** 557 nodes in 147 cases is a poor
  gradient target and a good case-gate target, as
  [`checker-notes-calls.md`](checker-notes-calls.md) already recorded. What
  changes is *which* work would move it.

### How you would know this page is wrong

Build instantiation for members of a generic named type and re-run the funnel. If
`of which: member types as error` falls by materially less than the ~97 the
Promise cases account for, the mechanism named here is not the one biting in the
corpus, even though fixtures A/A'/B/B' would still read the same. The fixtures
prove the mechanism exists and is sufficient *on that shape*; only the counter
proves it is the corpus's shape.
