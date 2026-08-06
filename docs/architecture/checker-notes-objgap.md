# The object-literal remainder, decomposed — §4.3's registered probe answered

Fifth session. `STATUS.md` §4.3 listed **2,223** object-literal lines as
unscorable with the probe *"split the catch-all by member kind"*, warning that
*"accessors (`bd tsr-32y`) and computed names both fall into the catch-all, in
unknown proportion"*. `examples/objgap.rs` runs that split. C1 = 0, C2 exact.

`depend.rs` at HEAD reads `ObjectLiteralExpression` at 2,479 lines; 1,309 have
the literal itself as the assertion node and are classified here.

| bucket | lines | owner |
|---|---:|---|
| **every member kind HAS an arm — a member *value* gaps** | **1,003 (77%)** | downstream; belongs to whatever types the value |
| computed name | 219 | late-bound names, `bd tsr-y4u.11`'s family |
| get + set accessor | 42 | `bd tsr-32y` |
| get accessor | 25 | `bd tsr-32y` |
| set accessor | 11 | `bd tsr-32y` |
| combinations of the two | 9 | both |

## The finding: the row is not an item, and the accessor item is 78 lines

**77% of it is downstream.** `objects.rs` gaps the *whole literal* when any
member's value gaps, so a literal full of handled member kinds still fails
because one property's initialiser is a call this port cannot resolve. Those
lines belong to the mechanism that types the value and would convert for free
when it lands — counting them as object-literal work would be
double-counting the whole gap.

**The accessor item (`bd tsr-32y`) is 78 lines, not 2,223.** §4.3's warning
about "unknown proportion" was exactly right and the proportion is 6%. This is
the third row this session to dissolve under its own registered probe rather
than convert (with `FunctionDeclaration` and the element-access remainder), and
the pattern is worth naming:

> **A catch-all's population is not its arm's population.** When a construct
> gaps *whole* on any unhandled part, its row counts every line the construct
> prints — including the lines that would convert anyway once something else
> lands. Split by *which part is unhandled* before costing the arm, and put the
> "every part is handled, something downstream gapped" bucket first so it
> cannot inflate the rest.

Computed names at 219 are the larger of the two real shards and they are the
same machinery `bd tsr-y4u.11` needs for late binding in the binder.
