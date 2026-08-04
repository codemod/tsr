//! Modifier arithmetic.
//!
//! Ported from typescript-go's `ensureModifiers`/`ensureModifierFlags`
//! (`internal/transformers/declarations/transform.go:2313`, `:2333`),
//! `maskModifierFlags` (`internal/transformers/declarations/util.go`) and
//! `ast.CreateModifiersFromModifierFlags` (`internal/ast/utilities.go:3250`).
//!
//! This is where `declare` comes from, and where `public`, `async` and `override`
//! go. It is four small functions with one large consequence: get the mask wrong
//! and every statement in the file gains or loses a keyword.

use tsr_ast::{ModifierFlags, ModifierLike, SyntaxKind};
use tsr_core::Span;

use crate::factory::Factory;

/// Ported from `ast.ModifierFlagsAll` (`internal/ast/modifierflags.go:50`).
///
/// Upstream's set also carries `ModifierFlagsDeprecated`, a JSDoc-only flag that
/// this AST does not model, so it is absent here rather than renamed. Nothing in
/// the declaration transform tests for it — it is in `All` upstream only so that
/// masking `All` clears it.
pub(crate) const ALL: ModifierFlags = ModifierFlags::EXPORT
    .union(ModifierFlags::AMBIENT)
    .union(ModifierFlags::PUBLIC)
    .union(ModifierFlags::PRIVATE)
    .union(ModifierFlags::PROTECTED)
    .union(ModifierFlags::STATIC)
    .union(ModifierFlags::READONLY)
    .union(ModifierFlags::ABSTRACT)
    .union(ModifierFlags::ACCESSOR)
    .union(ModifierFlags::ASYNC)
    .union(ModifierFlags::DEFAULT)
    .union(ModifierFlags::CONST)
    .union(ModifierFlags::OVERRIDE)
    .union(ModifierFlags::IN)
    .union(ModifierFlags::OUT)
    .union(ModifierFlags::DECORATOR);

/// The flag a modifier keyword stands for.
///
/// Ported from `ast.ModifierToFlag` (`internal/ast/utilities.go`).
pub(crate) fn modifier_to_flag(kind: SyntaxKind) -> ModifierFlags {
    match kind {
        SyntaxKind::StaticKeyword => ModifierFlags::STATIC,
        SyntaxKind::PublicKeyword => ModifierFlags::PUBLIC,
        SyntaxKind::ProtectedKeyword => ModifierFlags::PROTECTED,
        SyntaxKind::PrivateKeyword => ModifierFlags::PRIVATE,
        SyntaxKind::AbstractKeyword => ModifierFlags::ABSTRACT,
        SyntaxKind::AccessorKeyword => ModifierFlags::ACCESSOR,
        SyntaxKind::ExportKeyword => ModifierFlags::EXPORT,
        SyntaxKind::DeclareKeyword => ModifierFlags::AMBIENT,
        SyntaxKind::ConstKeyword => ModifierFlags::CONST,
        SyntaxKind::DefaultKeyword => ModifierFlags::DEFAULT,
        SyntaxKind::AsyncKeyword => ModifierFlags::ASYNC,
        SyntaxKind::ReadonlyKeyword => ModifierFlags::READONLY,
        SyntaxKind::OverrideKeyword => ModifierFlags::OVERRIDE,
        SyntaxKind::InKeyword => ModifierFlags::IN,
        SyntaxKind::OutKeyword => ModifierFlags::OUT,
        _ => ModifierFlags::empty(),
    }
}

/// Ported from `ast.GetCombinedModifierFlags`, for a node's own modifier list.
///
/// "Combined" upstream means walking up through a `VariableDeclaration` to its
/// `VariableStatement`, because the modifiers live on the statement. This port
/// never asks a declaration for flags its statement holds — the call sites pass
/// the statement — so the walk has no work to do and is not reproduced.
pub(crate) fn modifier_flags(modifiers: &[ModifierLike<'_>]) -> ModifierFlags {
    let mut flags = ModifierFlags::empty();
    for modifier in modifiers {
        match modifier {
            ModifierLike::Token(token) => flags |= modifier_to_flag(token.kind),
            ModifierLike::Decorator(_) => flags |= ModifierFlags::DECORATOR,
        }
    }
    flags
}

/// Ported from `maskModifierFlags` (`internal/transformers/declarations/util.go`).
///
/// The two corrections at the end are not simplifications and both are load
/// bearing: a `default` with no `export` is not syntactically valid, and
/// `declare` alongside `default` is an error rather than a redundancy.
pub(crate) fn mask_modifier_flags(
    flags: ModifierFlags,
    mask: ModifierFlags,
    additions: ModifierFlags,
) -> ModifierFlags {
    let mut flags = (flags & mask) | additions;
    if flags.contains(ModifierFlags::DEFAULT) && !flags.contains(ModifierFlags::EXPORT) {
        flags ^= ModifierFlags::EXPORT;
    }
    if flags.contains(ModifierFlags::DEFAULT) && flags.contains(ModifierFlags::AMBIENT) {
        flags ^= ModifierFlags::AMBIENT;
    }
    flags
}

/// Ported from `DeclarationTransformer.ensureModifierFlags`
/// (`internal/transformers/declarations/transform.go:2333`).
///
/// `is_always_type` ports `isAlwaysType`: an `interface` never needs `declare`,
/// because it has no runtime existence to declare.
pub(crate) fn ensure_modifier_flags(
    modifiers: &[ModifierLike<'_>],
    needs_declare: bool,
    parent_is_file: bool,
    is_always_type: bool,
) -> ModifierFlags {
    // No `async` and no `override` in declaration files.
    let mut mask = ALL ^ (ModifierFlags::PUBLIC | ModifierFlags::ASYNC | ModifierFlags::OVERRIDE);
    let mut additions = if needs_declare && !is_always_type {
        ModifierFlags::AMBIENT
    } else {
        ModifierFlags::empty()
    };
    if !parent_is_file {
        mask ^= ModifierFlags::AMBIENT;
        additions = ModifierFlags::empty();
    }
    mask_modifier_flags(modifier_flags(modifiers), mask, additions)
}

/// Ported from `ast.CreateModifiersFromModifierFlags`
/// (`internal/ast/utilities.go:3250`).
///
/// The order is upstream's, and it is the emitted order: `export declare default
/// const public private protected abstract static override readonly accessor
/// async in out`. Sorting it any other way would produce text that differs from
/// every baseline while parsing identically, which is the worst kind of
/// divergence — invisible to a round trip, fatal to a byte comparison.
pub(crate) fn create_modifiers_from_flags<'a>(
    factory: &mut Factory<'a, '_>,
    flags: ModifierFlags,
    span: Span,
) -> Vec<ModifierLike<'a>> {
    const ORDER: &[(ModifierFlags, SyntaxKind)] = &[
        (ModifierFlags::EXPORT, SyntaxKind::ExportKeyword),
        (ModifierFlags::AMBIENT, SyntaxKind::DeclareKeyword),
        (ModifierFlags::DEFAULT, SyntaxKind::DefaultKeyword),
        (ModifierFlags::CONST, SyntaxKind::ConstKeyword),
        (ModifierFlags::PUBLIC, SyntaxKind::PublicKeyword),
        (ModifierFlags::PRIVATE, SyntaxKind::PrivateKeyword),
        (ModifierFlags::PROTECTED, SyntaxKind::ProtectedKeyword),
        (ModifierFlags::ABSTRACT, SyntaxKind::AbstractKeyword),
        (ModifierFlags::STATIC, SyntaxKind::StaticKeyword),
        (ModifierFlags::OVERRIDE, SyntaxKind::OverrideKeyword),
        (ModifierFlags::READONLY, SyntaxKind::ReadonlyKeyword),
        (ModifierFlags::ACCESSOR, SyntaxKind::AccessorKeyword),
        (ModifierFlags::ASYNC, SyntaxKind::AsyncKeyword),
        (ModifierFlags::IN, SyntaxKind::InKeyword),
        (ModifierFlags::OUT, SyntaxKind::OutKeyword),
    ];

    ORDER
        .iter()
        .filter(|(flag, _)| flags.contains(*flag))
        .map(|(_, kind)| factory.modifier(*kind, span))
        .collect()
}

/// Ported from `DeclarationTransformer.ensureModifiers`
/// (`internal/transformers/declarations/transform.go:2313`).
///
/// Upstream reuses the original modifier nodes when the flags are unchanged,
/// filtering out decorators. The reuse is not an optimisation: reusing the nodes
/// keeps their comment ranges, which upstream's printer emits. This printer emits
/// no comments (`docs/architecture/printer.md`), so the reuse buys nothing here —
/// but it is kept, because it also keeps the *spans*, and a diagnostic anchored to
/// a synthesized modifier would point at nothing.
pub(crate) fn ensure_modifiers<'a>(
    factory: &mut Factory<'a, '_>,
    modifiers: &'a [ModifierLike<'a>],
    span: Span,
    needs_declare: bool,
    parent_is_file: bool,
    is_always_type: bool,
) -> &'a [ModifierLike<'a>] {
    let current = modifier_flags(modifiers) & ALL;
    let new_flags = ensure_modifier_flags(modifiers, needs_declare, parent_is_file, is_always_type);
    if current == new_flags {
        if modifiers.is_empty() {
            return modifiers;
        }
        let kept: Vec<ModifierLike<'a>> = modifiers
            .iter()
            .copied()
            .filter(|modifier| matches!(modifier, ModifierLike::Token(_)))
            .collect();
        return factory.slice(&kept);
    }
    let created = create_modifiers_from_flags(factory, new_flags, span);
    factory.slice(&created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_exported_default_keeps_its_export() {
        // `maskModifierFlags`'s first correction. `export default class {}` has its
        // export stripped by the mask, and putting it back is what keeps the output
        // parseable at all.
        let flags = mask_modifier_flags(
            ModifierFlags::DEFAULT | ModifierFlags::EXPORT,
            ALL ^ ModifierFlags::EXPORT,
            ModifierFlags::empty(),
        );
        assert!(flags.contains(ModifierFlags::EXPORT));
        assert!(flags.contains(ModifierFlags::DEFAULT));
    }

    #[test]
    fn declare_is_dropped_beside_default() {
        // The second correction: `declare` alongside `default` is an error, not a
        // redundancy, so a top-level `export default` never gains `declare`.
        let flags = mask_modifier_flags(
            ModifierFlags::DEFAULT | ModifierFlags::EXPORT,
            ALL,
            ModifierFlags::AMBIENT,
        );
        assert!(!flags.contains(ModifierFlags::AMBIENT));
    }

    #[test]
    fn an_interface_never_gains_declare() {
        // `isAlwaysType`. An interface has no runtime existence to declare, and
        // upstream's baselines never write `declare interface` at top level.
        let flags = ensure_modifier_flags(&[], true, true, true);
        assert!(!flags.contains(ModifierFlags::AMBIENT));
        let flags = ensure_modifier_flags(&[], true, true, false);
        assert!(flags.contains(ModifierFlags::AMBIENT));
    }

    #[test]
    fn a_nested_declaration_never_gains_declare() {
        // Inside a namespace body, `parentIsFile` is false: the mask clears
        // `AMBIENT` and the addition is suppressed, because the enclosing
        // `declare namespace` already covers everything inside it.
        let flags = ensure_modifier_flags(&[], true, false, false);
        assert!(!flags.contains(ModifierFlags::AMBIENT));
    }
}
