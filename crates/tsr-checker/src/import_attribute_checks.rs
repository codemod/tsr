//! The two value checks on an import or export declaration's `with { … }`
//! clause that [`crate::import_attributes`] left unported:
//!
//! - **TS2858**, `Import attribute values must be string literal
//!   expressions.`: the attribute loop at the end of
//!   `checkExternalImportOrExportDeclaration` (`checker.go:5361-5371`);
//! - **TS2322** against `ImportAttributes`: the opening relation of
//!   `checkImportAttributes` (`checker.go:5413-5416`), over the synthetic
//!   object type `getTypeFromImportAttributes` (`checker.go:5444`) builds.
//!
//! Both are `c.error` reports, not grammar errors, so neither is withheld in a
//! file with parse diagnostics (the grammar arms in `import_attributes.rs`
//! are).
//!
//! # Checker port convention (`docs/conventions.md`)
//!
//! - **Native operation:** `getTypeFromImportAttributes` caches its object in
//!   `typeNodeLinks[node].resolvedType`, keyed by the `ImportAttributes`
//!   node. This port mints it once per declaration check and keeps no cache:
//!   the check walk visits each declaration once, and nothing else reads the
//!   type (no contextual type, inference or printer asks for it), so a side
//!   table would only hold dead entries.
//! - **Publication:** the minted object is registered in
//!   `anonymous_properties` / `object_literal_members` (so the relater reads
//!   its members) and in `non_inferrable_types` (native's
//!   `ObjectFlagsNonInferrableType`). Nothing is provisional.
//! - **Receiver/alias context:** none; the members are the attributes' own
//!   names with `getRegularTypeOfLiteralType(checkExpression(value))`.
//! - **Expensive work:** one `check_expression` per attribute value and one
//!   relation per declaration that carries attributes. Declarations without
//!   a `with` clause return before any work.
//!
//! Not ported here: when TS2858 fires, native's
//! `checkExternalImportOrExportDeclaration` answers false and
//! `checkImportDeclaration` / `checkExportDeclaration` skip the import and
//! export bindings (`checker.go:5281`, `:5515`). That skip belongs to the
//! callers in `check.rs` (MAIN). No corpus case reports a binding diagnostic
//! beside a TS2858 (`docs/parity/notes/r6-triage.md` §4).

#![expect(
    dead_code,
    reason = "the check.rs and import_attributes.rs hooks ship as r6-triage-import-attribute-values.diff"
)]

use tsr_ast::{ImportAttributeName, ImportAttributes, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::objects::{AnonymousProperty, PrintedSlot, PropertySlot};
use crate::relater::{Relation, Ternary};
use crate::types::TypeId;

impl Checker<'_, '_> {
    /// The attribute loop of `checkExternalImportOrExportDeclaration`
    /// (`checker.go:5361-5371`) for `declaration`, an import or export
    /// declaration: TS2858 at every attribute value that is not a string
    /// literal. Returns whether it reported (native's `hasError`, which makes
    /// the function answer false).
    ///
    /// The function's earlier arms return before the loop: a missing or
    /// non-string module name (TS1141, `grammar.rs`), a declaration outside a
    /// source file or an ambient external module's block (TS1147/TS1194,
    /// `check.rs`), and a relative name inside an ambient external module that
    /// is not a module augmentation (TS2439).
    pub(crate) fn check_import_attribute_values(&mut self, declaration: NodeId) -> bool {
        let (attributes, specifier) = match self.node_map.get(declaration) {
            Some(Node::ImportDeclaration(node)) => (node.attributes, node.module_specifier),
            Some(Node::ExportDeclaration(node)) => (node.attributes, node.module_specifier),
            _ => return false,
        };
        let Some(attributes) = attributes else { return false };
        let Some(specifier) = specifier.and_then(|specifier| specifier.node_id()) else {
            return false;
        };
        // `ast.NodeIsMissing(moduleName)` and `!ast.IsStringLiteral(moduleName)`.
        let span = self.nodes.span(specifier);
        if span.start == span.end || self.nodes.kind(specifier) != SyntaxKind::StringLiteral {
            return false;
        }
        if !self.external_import_is_positioned_for_resolution(declaration) {
            return false;
        }
        if let Some(parent) = self.nodes.parent(declaration)
            && self.nodes.kind(parent) == SyntaxKind::ModuleBlock
            && let Some(Node::StringLiteral(name)) = self.node_map.get(specifier)
            && tsr_path::is_external_module_name_relative(name.text)
            && !self.is_top_level_in_external_module_augmentation(declaration)
        {
            return false;
        }
        let mut has_error = false;
        for attribute in attributes.attributes {
            let Some(value) = attribute.value else { continue };
            let Some(at) = value.node_id() else { continue };
            if self.nodes.kind(at) == SyntaxKind::StringLiteral {
                continue;
            }
            has_error = true;
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::IMPORT_ATTRIBUTE_VALUES_MUST_BE_STRING_LITERAL_EXPRESSIONS,
                    span,
                ),
            );
        }
        has_error
    }

    /// The opening of `checkImportAttributes` (`checker.go:5413-5416`):
    ///
    /// ```go
    /// importAttributesType := c.getGlobalImportAttributesTypeChecked()
    /// if importAttributesType != c.emptyObjectType {
    ///     c.checkTypeAssignableTo(c.getTypeFromImportAttributes(node),
    ///         c.getNullableType(importAttributesType, TypeFlagsUndefined), node, nil)
    /// }
    /// ```
    ///
    /// `declaration` is the import or export declaration; the caller has
    /// already passed `checkGrammarModuleElementContext`. A missing global
    /// `ImportAttributes` is native's `emptyObjectType` answer, so nothing is
    /// related (its TS2318 is the global-type resolver's, not this check's).
    /// No expression is passed, so nothing is elaborated.
    pub(crate) fn check_import_attributes_assignable(&mut self, declaration: NodeId) {
        let attributes = match self.node_map.get(declaration) {
            Some(Node::ImportDeclaration(node)) => node.attributes,
            Some(Node::ExportDeclaration(node)) => node.attributes,
            _ => None,
        };
        let Some(attributes) = attributes else { return };
        let Some(at) = attributes.node_id else { return };
        let Some(symbol) = self.global_type_symbol_with_arity("ImportAttributes", 0) else {
            return;
        };
        let target = self.get_declared_type_of_symbol(symbol);
        if self.is_gap(target) || target == self.intrinsics.empty_object {
            return;
        }
        let Some(source) = self.type_from_import_attributes(attributes) else { return };
        let target = self.get_union_type(&[target, self.intrinsics.undefined]);
        if self.relate_ternary(source, target, Relation::Assignable) == Ternary::NotRelated {
            self.report_relation_failure(at, self.error_span(at), None, source, target, None);
        }
    }

    /// `getTypeFromImportAttributes` (`checker.go:5444`): an anonymous
    /// object-literal type with one property per attribute, typed
    /// `getRegularTypeOfLiteralType(checkExpression(value))`, flagged
    /// `ObjectLiteral | NonInferrableType`. A later attribute with the same
    /// name replaces the earlier one (`members[member.Name] = member`).
    /// `None` where a value's type is the port's gap, so no relation is
    /// decided from an uncomputed member.
    fn type_from_import_attributes(&mut self, attributes: &ImportAttributes<'_>) -> Option<TypeId> {
        let mut properties: Vec<AnonymousProperty> = Vec::new();
        for attribute in attributes.attributes {
            let name = match attribute.name? {
                ImportAttributeName::Identifier(name) => name.text,
                ImportAttributeName::StringLiteral(name) => name.text,
            };
            let value = attribute.value?;
            let checked = self.check_expression(value);
            if self.is_gap(checked) {
                return None;
            }
            let ty = self.get_regular_type_of_literal_type(checked);
            let property = AnonymousProperty {
                accessor_write: None,
                method: false,
                origin: None,
                checked_declaration: None,
                name: name.to_owned(),
                printed_name: if crate::objects::is_identifier_text(name) {
                    name.to_owned()
                } else {
                    crate::printing::quote_ascii(name)
                },
                printed_slot: PrintedSlot::printed(self.type_to_string(ty)),
                optional: false,
                readonly: false,
                slot: PropertySlot::resolved(ty),
            };
            match properties.iter().position(|existing| existing.name == property.name) {
                Some(index) => properties[index] = property,
                None => properties.push(property),
            }
        }
        let members = self.property_members(&properties);
        let text = crate::objects::render_object_type(&members);
        let ty = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.anonymous_properties.insert(ty, (properties, true));
        self.object_literal_members.insert(ty, members);
        // `ObjectFlagsObjectLiteral` (no spread) and the symbol's
        // `SymbolFlagsObjectLiteral`.
        self.object_literal_spread_flags.insert(ty, false);
        self.non_inferrable_types.insert(ty);
        Some(ty)
    }

    /// `isTopLevelInExternalModuleAugmentation` (`checker/utilities.go:246`):
    /// the declaration's parent is a module block whose module declaration is
    /// an external module augmentation (`ast.IsExternalModuleAugmentation`,
    /// `ast/utilities.go:3567`, with `IsModuleAugmentationExternal`, `:1694`).
    fn is_top_level_in_external_module_augmentation(&self, declaration: NodeId) -> bool {
        let Some(block) = self.nodes.parent(declaration) else { return false };
        if self.nodes.kind(block) != SyntaxKind::ModuleBlock {
            return false;
        }
        let Some(module) = self.nodes.parent(block) else { return false };
        if !self.is_ambient_module_declaration(module) {
            return false;
        }
        let Some(parent) = self.nodes.parent(module) else { return false };
        match self.node_map.get(parent) {
            Some(Node::SourceFile(source)) => tsr_binder::is_external_module(source),
            Some(Node::ModuleBlock(_)) => self.nodes.parent(parent).is_some_and(|outer| {
                self.is_ambient_module_declaration(outer)
                    && matches!(self.nodes.parent(outer).and_then(|file| self.node_map.get(file)),
                        Some(Node::SourceFile(source)) if !tsr_binder::is_external_module(source))
            }),
            _ => false,
        }
    }
}
