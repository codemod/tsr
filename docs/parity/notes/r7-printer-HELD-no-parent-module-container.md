# HELD: the no-parent arm of `symbol_chain` asks the resolver (r7-printer)

Held on `box/r7-printer-held`, not landable yet.

`symbol_chain` (checker.rs) stops at a symbol with no parent. Native's
`getContainersOfSymbol` fallback (`symbolaccessibility.go:288`) offers the
module of a top-level declaration that the module exports through an alias
(`class C {}` then `export { C }`), so `exportsAndImports1{,-es6}`'s `t2.ts`
prints `typeof import("./t1").C`. This diff asks the resolver
(`module_rooted_chain_at`) there.

Measured against `fd274090`, both dumps unfiltered: types **+8 / −1**,
diagnostics unchanged. The loss is
`commonJSImportClassTypeReference:0:0`: in `main.js`,
`const { K } = require("./mod1")` is an **alias** in native's binder
(`bindVariableDeclarationOrBindingElement`'s require arm), so the accessible
chain is `[K]` and native prints `typeof K`. The port's binder binds that
binding element as a `BLOCK_SCOPED_VARIABLE`, the resolver's alias loop
cannot see it, and the arm spells `typeof import("./mod1").K`. Teaching the
resolver to treat the binding as an alias would re-derive the binder's
decision outside the binder (box-protocol §3a), so the arm waits for the
binder (r7-shared, `crates/tsr-binder`). Expected after it: +8 / −0.
