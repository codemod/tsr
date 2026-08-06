# `bd tsr-6v7` — widening `SELECTABLE` is blocked on assignability. REFUSED.

> **Correction, sixth session.** The refusal stands and its numbers stand, but
> the reason quoted below — *"`relater.rs` says in its own module doc that two
> distinct object types answer `false` because structural comparison is
> narrow"* — cited a doc that was already stale. Structural comparison of object
> types **is** ported (`e24b7ca`, 387 commits before this file was written). The
> correct reason is that the relation has no way to say *"I could not tell"*, so
> a `false` between object types is still untrustworthy — for six enumerated
> reasons, none of which is "structural comparison is missing". See
> `checker-notes-assign.md` §1 and §2. What would overturn the refusal is
> therefore `bd tsr-kmzf` (a third answer), not structural comparison.

Fifth session, `examples/selectable.rs`. C1 = 0, C2 exact.

`examples/callgate.rs` measured overload selection's own population at 1,095
lines, of which **473 stop at `a parameter type outside SELECTABLE`** and 28 at
the argument twin. `bd tsr-6v7` proposes widening that set. The open question
was whether the excluded space is *cheap* (enums, unique symbols — a few flags)
or *expensive* (object types, which need structural assignability).

Resolving the real candidate parameter types — a match test, not a read of the
annotations' syntax — over 492 classified lines:

| what the first non-selectable parameter is | lines |
|---|---:|
| **an OBJECT parameter — needs structural assignability** | **303 (62%)** |
| a TYPE PARAMETER — generic, needs inference | 45 |
| a UNION parameter | 44 |
| a parameter annotation that itself gaps | 36 |
| an `any` parameter | 35 |
| every parameter IS selectable — the gate is elsewhere | 24 |
| a parameter with no annotation | 5 |

**REFUSED.** `SELECTABLE` is not a conservative flag list that could be
loosened; it is a faithful statement of where this port's relater can be
trusted to say **no**. 62% of what it excludes is object types, and
`relater.rs` says in its own module doc that two distinct object types answer
`false` because structural comparison is narrow — *not* because they are
unrelated. Widening to admit them would make a **false negative silently
promote the next overload**, which is the plausible-wrong-answer failure the
whole `errorType`-not-`anyType` discipline exists to prevent.

> The item is not "widen a constant". It is **structural assignability**, and
> until that lands the constant is correct as written. A further 45 lines are
> inference (`bd tsr-g30h`) and 44 are unions, which need the same relation one
> level up.

**What would overturn it:** structural assignability landing, at which point
this probe should be re-run — the 303 become reachable in one step, and the 24
"the gate is elsewhere" lines are the only part addressable before then, which
is not worth a build.
