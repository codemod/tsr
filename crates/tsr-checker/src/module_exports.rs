//! The text of a `ModuleExportName`, identifier or string literal.
//!
//! ES2022 lets an import or export specifier name the export with a string
//! literal — `export { x as "<X>" }`, `import { "<X>" as y }`. Upstream never
//! distinguishes the two spellings once it has the name: every reader goes
//! through `Node.Text()` (`internal/ast/ast.go`), and the binder keys the
//! module's `exports` table by the same text (`getDeclarationName`,
//! `internal/binder/binder.go`; here `export_name` in
//! `crates/tsr-binder/src/binder.rs`). So the checker's lookups must too, or a
//! string-literal export is bound under a key no lookup can reach. See
//! `docs/parity/notes/r5-modexports.md` §1.

/// `name.Text()` for a `ModuleExportName` (`internal/ast/ast.go`, `Node.Text`):
/// the identifier's text or the string literal's cooked value. The binder keys
/// `exports` by exactly this text.
pub(crate) fn module_export_name_text(name: tsr_ast::ModuleExportName<'_>) -> &str {
    match name {
        tsr_ast::ModuleExportName::Identifier(name) => name.text,
        tsr_ast::ModuleExportName::StringLiteral(name) => name.text,
    }
}

/// `ast.ModuleExportNameIsDefault` (`internal/ast/utilities.go:2539`):
/// `node.Text() == InternalSymbolNameDefault`, so `{ "default" as d }` is the
/// default import exactly as `{ default as d }` is.
pub(crate) fn module_export_name_is_default(name: tsr_ast::ModuleExportName<'_>) -> bool {
    module_export_name_text(name) == "default"
}
