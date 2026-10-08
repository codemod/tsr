# Lane notes: r4-heritage (tsr-2zk.939, tsr-2zk.3.2)

Judgment calls made by the `r4-heritage` parity box. Numbers are measured with
`verdictdump` / `diagverdictdump` against the baseline frozen at `5ad60b1`
(integration head at dispatch): diagnostics RIGHT 4312, EMPTY_RIGHT 4978,
WRONG 1190, EMPTY_WRONG 90; `checker_types` lines right 470138 of 477979.
Native behaviour is reproduced with a `tsgo` built from the pinned submodule
(`5b1047d`) by `scripts/offline-cargo/build-tsgo.sh`.

## 1. `links.interfaceChecked` is the first *checked* declaration

**Forcing constraint.** `checkInterfaceDeclaration` (`checker.go:4991`) runs
its once-per-symbol block — `checkInheritedPropertiesAreIdentical`, the
TS2430 loop over `getBaseTypes`, and `checkIndexConstraints(t, symbol,
false)` — from whichever declaration of the merged symbol is checked first,
guarded by `declaredTypeLinks.interfaceChecked`. The port ran it only from the
symbol's first interface declaration. A user `interface Object { data: A;
[x: string]: Object }` merges with lib.es5's `Object`, whose declaration is
first in the merged symbol and is never checked (the bundled libraries are
never walked), so the block never ran and `data`'s TS2411 was lost
(`objectTypeHidingMembersOfExtendedObject` 10). The port now picks the first
interface declaration that is not in a bundled default library
(`Checker::in_default_library`). Native, reached from the user declaration,
also reports on the lib side (`lib.es5.d.ts(--,--)` lines in that baseline);
those have no position and are not scored, and this port reports them too
because the check runs over the merged declared type.

**Alternatives.** Keep a once-per-symbol flag set at check time, as native
does. That needs a new per-symbol side table for a boolean whose only effect
over "first non-library declaration" is a program in which a non-library
declaration is not checked. Every non-library file is checked here (the
corpus and the CLI walk every root file), so the two agree. Falsifier: a
corpus case whose first non-library interface declaration lives in a file the
port does not walk (e.g. under `skipLibCheck`).

**The parse-error gate is dropped for interfaces.** `check_heritage_conformance`
returned on `file_has_parse_errors`; `checkInterfaceDeclaration` has no such
gate. `interfaceExtendingClass2` (11) has a recovered parse error later in
the file and natively reports TS2411 for the interface inheriting `Foo`'s
string index. The class arm keeps the gate for now (§2 measures it).

**Index constraints are called on the type, not the node.** The interface
arm called `check_index_constraints(node)` in `index_constraint.rs`, which
repeats the first-declaration rule and the parse-error gate on its own. This
lane now calls `check_index_constraints_of_type(declared, symbol, false)`
directly — the native call — which needed that function's visibility raised
to `pub(crate)`; no behaviour of `index_constraint.rs` changed. Its
`InterfaceDeclaration` arm in `check_index_constraints` is now unreached
(the class and type-literal arms are unchanged); removing it is left to that
file's owner.

Port convention record: no cache, side table, mapper or traversal is added;
the existing declared-type, base-type and index-constraint queries run once
per symbol from the chosen declaration.

**Measured** against the frozen baseline: diagnostics RIGHT 4312 -> 4314
(`objectTypeHidingMembersOfExtendedObject`, `interfaceExtendingClass2`);
`checker_types` unchanged; both loss checks empty.
