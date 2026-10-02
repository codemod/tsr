# Object-literal targets and optional spread merges

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 4aad09e1: 454,963/478,855 correct assertions (95.01%).
The active 99% target requires 474,067 matches.

## Object-literal target requirements

The semantic index relation port preserves incompatible indexed literal unions,
but [{["a" as string]:1},{b:2}] still collapses before widening. Native
propertiesRelatedTo (relater.go:4240) separately requires every source name to
exist as an actual property on an ObjectLiteral target. Its index signature does
not substitute for that property. A native control with an explicit
{[x:string]:number} annotation does reduce; a computed indexed literal does not.
The difference follows semantic ObjectLiteral metadata, retained by regularization
and removed by widening, not the displayed index spelling.

The port adds the native target-side named-member check, using complete property
name enumeration. Unresolved names remain Unknown. This is tracked in tsr-6.40;
full corpus measurement and native controls follow.

## Optional spread merge

Native getSpreadType (checker.go:13463) keeps the left property's optionality
when an optional right property collides. It unions the left type with the
right type after removeMissingOrUndefinedType (checker.go:29099), using subtype
reduction. Equal present types reuse the left type. The Rust path previously
replaced both the semantic record and its displayed member unconditionally.
Both now use the merged TypeId and presence flag. Required right properties
still overwrite, and an ordinary assignment after a spread still overwrites.

The source extractor reads optionality through property_is_optional: the binder
does not set SymbolFlags::OPTIONAL. Nullable member values are now supported by
the declaration widening port, so the old nullable refusal is removed. Exact
optional display removes missingType while retaining explicitly written undefined.

tryMergeUnionOfObjectTypeAndEmptyObject (checker.go:13530) turns one nonempty
object plus empty/falsy alternatives into optional properties. That conversion
must follow isValidSpreadType (checker.go:13504): truthy primitive alternatives
and a wholly null/undefined union are invalid. The first candidate omitted this
validation and lost ten RIGHT assertions in spreadInvalidArgumentType and
spreadUnion3. The native validation restores all ten without losing the gains.
This is a semantic operand check, not a fixture or text gate.

The current extractor still limits this port: multiple nonempty alternatives
need distribution; generic references, methods/accessors, class visibility,
computed names and index infos need complete semantic extraction. The empty
object predicate recognizes captured anonymous objects and empty-spreading
primitive flags, not every resolved structural empty type. Optional merge is
currently supported by the existing complete property-only capture. No claim
is made that the entire spread subsystem is ported.

## Controls and measured candidate

Four tests in literal_target_spreads.rs cover compatible indexed literals versus
explicit index annotations, optional/required overlaps, exact explicit undefined,
partial object unions, false alternatives, member reads and invalid operands.
Native probes use pinned tsgo with explicit --strict true, and the exact case also
sets --exactOptionalPropertyTypes true. The overwrite-of-undefined control emits
TS2783 but still produces the native declaration used to check the result. Invalid
spreads emit TS2698 and declaration type any; the test producer exposes the same
recovery errorType as error. For unchanged optional symbols, declaration emission
can omit implicit undefined, so the same-type optimization is checked by a member
read as well as the exact-mode object output.

Native files are /tmp/tsr-99-index-compatible-native,
/tmp/tsr-99-optional-spread-native, /tmp/tsr-99-optional-spread-native-exact and
/tmp/tsr-99-invalid-spread-native. Every expected semantic outcome is derived
from these native probes; existing corpus fixtures supply the full regression check.

The frozen candidate /tmp/tsr-99-spread-candidate2.tsv has 455,064 RIGHT,
2,912 GAP and 16,267 WRONG among 474,243 aligned rows. It gains 101 matches:
93 WRONG-to-RIGHT and eight GAP-to-RIGHT, with zero RIGHT losses. Two GAP-to-WRONG
and 25 changed-wrong rows remain; these are not counted as successes. Sixteen
conversions in each spreadDuplicate variant close the central optional merge
cases. The relation rule alone gained 42 matches. On the full 478,855 denominator,
455,064 is 95.03%; 19,003 further matches are required for 99%.

Residual changed answers include optional-symbol display, spread ordering,
computed enum keys, union distribution and union member ordering. These need
semantic follow-up rather than rendered-text replacement. The first partial
candidate's ten regressions were refused and repaired before delivery.

Simplification reuses existing TypeId union reduction, fact filtering, optionality
and semantic property capture. Review runs sequentially in the primary thread
under the repository tool map; no independent or cross-model review is claimed.
Focused tests pass. Workspace gates and an isolated committed reproduction follow.

Follow-up scope is tracked in tsr-8 under the active tsr-6 goal.
Checker sources plus trace_case SHA-256: b06f70bd600be42bca214b7a31030ddbb5e3021d290f9de7bd53d3371d65066e

The release workspace passes 201 result blocks. Clippy, formatting, whitespace
and all 3,342 upstream references pass. A RIGHT loss, removal of explicit undefined
in exact mode, loss of required left presence, acceptance of a truthy primitive
spread, or reduction of the computed indexed-literal control falsifies the
corresponding claim. These checks precede the code commit; its isolated measurement
and deliberate mutations are recorded below once completed.
