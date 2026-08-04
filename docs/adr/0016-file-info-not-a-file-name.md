# ADR-0016: the binder takes a `FileInfo`, not a file name

- **Status:** accepted
- **Date:** 2026-08-04
- **Supersedes:** [ADR-0015](0015-file-name-is-a-bind-input.md) in part — the file
  name is still an input to binding, but it is no longer the only one and no
  longer a bare parameter.

## The forcing constraint

ADR-0015 named its own falsifier:

> **A second per-file fact is needed** — `IsDeclarationFile` from configuration
> rather than the suffix, a package.json `type` field, a module kind. Two
> parameters is one too many; that is the signal to introduce the `FileInfo`
> struct named above.

**It fired**, and from a direction the ADR did not list: the **source text**.

A JSX namespaced attribute, `<X ns:href={…} />`, declares one symbol whose name
is `ns:href`. Upstream builds it — `JsxNamespacedName.Text()` joins the two
halves with a colon — because Go allocates strings freely. Here every
`Symbol::name` is a `&'a str` borrowed from the source or from the file name
([ADR-0013](0013-checker-memoisation.md) put symbols in a contiguous store; an
owned name per symbol would cost 24 bytes against 16 across 41,623 symbols on the
benchmark fixtures). The binder was leaving the attribute undeclared rather than
misnamed, and `lib.rs` said so.

The name is *already contiguous in the source*: `ns:href` is exactly the range
the two identifier nodes span. Nothing needed to be built — only read. But the
binder held the tree and the node table, and neither carries the text.

## The decision

```rust
pub struct FileInfo<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

pub fn bind<'a>(file: &'a SourceFile<'a>, nodes: &NodeTable, info: FileInfo<'a>)
    -> BindResult<'a>
```

`FileInfo` is what upstream reads off `b.file` — the source file *and* everything
the compiler knows about it. Our `SourceFile` node is generated from `ast.json`
and carries only what that schema names
([ADR-0005](0005-codegen-from-ast-json.md)), so the rest arrives beside it.

The text is read in exactly one place today, and that is the point: it is a
capability, not a habit. Anything that reaches for it should ask first whether
the tree already holds the answer.

## Alternatives

**Keep two parameters.** Rejected by ADR-0015 in advance, and the reason holds:
the third caller-visible argument is where a signature stops documenting itself.
`bind(file, nodes, name, text)` says nothing about which of the last two matters
to which decision; `FileInfo` has a field comment apiece.

**Give the parser a `text` field on `JsxNamespacedName`.** This is closest to
upstream, and it is closed for the same reason ADR-0015 could not add fields to
`SourceFile`: the node is generated and diffed in CI
([ADR-0007](0007-generated-code-policy.md)). *This wins if* the generator ever
grows a sanctioned way to carry derived text on a node, which several other
gaps would also use.

**Make `Symbol::name` a `Cow<'a, str>`.** Would solve this and the numeric-name
normalisation below it (`{ 0b11: x, 3: y }` are one member upstream, two here,
because the name has to be *computed* rather than sliced). Rejected for now on
size: 8 bytes per symbol against a gap currently measured at ~5 conformance
cases. *This wins if* the checker needs synthesised names anyway — late-bound
property names resolve to strings nothing in the source spells, so it probably
will, and then this ADR is superseded rather than extended.

## Consequences accepted

- **Every caller passes a struct**, including benchmarks that only care about the
  name. They write `FileInfo { name, text }`.
- **The binder can now read the source**, which is a wider capability than the one
  case that motivated it. Nothing enforces the discipline above but review.
- **Names that must be *built* are still unavailable.** A JSX namespaced name is
  a slice; a normalised numeric name is not. That gap is unchanged and is why the
  `Cow` alternative is written down rather than dismissed.

## How we would know this was wrong

- **A third field arrives that is not a property of the file** — a compiler
  option, say. `FileInfo` would then be a bag rather than a description, and the
  right shape is a `Program`-level context the binder borrows from.
- **The text gets read for things the tree could answer.** That is the failure
  mode of handing out a capability, and it shows up as span arithmetic in the
  binder where a node field would have done.
- **Synthesised names become common**, at which point the borrow-only constraint
  on `Symbol::name` is the thing to revisit, not this signature.
