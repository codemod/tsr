# r7-parser — parser and scanner parity (`tsr-2zk.1270`)

Round-7 lane box for `crates/tsr-parser` and `crates/tsr-scanner`. Native is
`vendor/typescript-go` @ `5b1047d`. Base: `origin/main` @ `9020aa67`, frozen
with `scripts/parity_gate.sh freeze`:

- types: 556,357 aligned lines, 551,176 RIGHT / 4,508 WRONG / 673 GAP;
- diagnostics: 12,238 cases, 5,741 RIGHT / 5,610 EMPTY_RIGHT / 852 WRONG /
  35 EMPTY_WRONG.

Re-measured queue on that base (r6-triage's finished-alone lists): REGEXP-
SCANNER-VALIDATION is already converted on main (0/10 still wrong: the
validator and its checker hook landed in round 6); SCANNER-NUMERIC-AND-ESCAPE
has 3/15 left; every other cluster is unchanged.

## 1. `parseDecoratorExpression` and the unary type assertion

- `parseDecoratorExpression` (`parser.go:3906`): a decorator is a
  left-hand-side expression parsed in the decorator context, which keeps a
  following `[` for the decorated member's computed name
  (`parseMemberExpressionRest`, `:5357`). The context is cleared by
  `parseExpression`, argument lists, function expressions and function bodies,
  as native's. `@await` in an await context is TS1109 plus a missing
  identifier. The previous port parsed only `@(expr)` or a dotted name, so
  `@<…`, `@new …` and `@x?.y()` recovered differently.
- `parseDecoratedExpression` (`:5723`): decorators in expression position
  before anything but `class` are `Expression expected` at the next token's
  full start. Native returns `MissingDeclaration(modifiers)`; this AST's
  `Expression` union has no `MissingDeclaration` (it is generated from
  `ast.json`), so the expression is a missing identifier and the decorators are
  not in the tree. The checker does not visit them natively either
  (`checkExpression` has no arm for it), so diagnostics match; the types
  walker loses the decorator's own lines (`>dec : …` in
  `decoratorOnFunctionExpression.types`). Fixing that needs
  `Expression::MissingDeclaration` in the generated AST: routed.
- A type assertion is a unary expression (`parseSimpleUnaryExpression`'s `<`
  arm, `:5070`), not a primary one: `parsePrimaryExpression` has no `<` arm,
  so `@<T>x`, `new <T>x` and `++<T>x` are TS1109 at the `<`.

## 3. Measurements

See the commit messages and the box-protocol §6 reports for per-commit
numbers; §4 below keeps the running table.

## 4. Running table

| commit | port | diag cases | types lines | perf (CPU ratio, dm / gi) |
|---|---|---|---|---|
