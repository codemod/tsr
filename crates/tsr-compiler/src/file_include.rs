//! Why each file is in the program, and the diagnostics that explain it.
//!
//! Ported from `internal/compiler/fileInclude.go`, the include half of
//! `internal/compiler/includeprocessor.go` and
//! `processingDiagnostic.createDiagnosticExplainingFile`
//! (`internal/compiler/processingDiagnostic.go:69`) at `5b1047d`.
//!
//! The loader records a [`FileIncludeReason`] on every task it creates
//! (`parseTask.includeReason`), and its replay walk collects them per file in
//! the order upstream's `filesParser.collectFiles` does
//! ([`crate::loader::LoadedFiles::include_reasons`]). The first consumer is
//! `Program.verifyCompilerOptions`' composite file-list check (TS6307,
//! [`crate::program_diagnostics::composite_file_list_diagnostics`]); a
//! diagnostic explaining a file is positioned at the first written reference
//! to it and carries the other reasons as a message chain and related
//! information, exactly as upstream builds it.
//!
//! Not ported, each unreachable from TS6307 today and named rather than
//! approximated: the root-file reason's `Part of 'files' list` / `Matched by
//! include pattern` explanations (`GetMatchedFileSpec`/`GetMatchedIncludeSpec`
//! have no counterpart in `tsr-tsoptions`), and the related information of
//! the root-file, lib-file and automatic-type-directive reasons, which points
//! into the config file's syntax tree that the program does not hold. A root
//! file is in `rootPaths` by definition, and lib files and type-library entry
//! points are declaration files, which are never emitted, so neither can be the
//! subject of TS6307. They can still appear as *other* reasons of a subject
//! file; then this port explains them with upstream's no-config text
//! (`Root file specified for compilation`) and omits their related location.
//! `--explainFiles` itself is not ported.

use std::sync::Arc;

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_core::{ResolutionMode, Span};
use tsr_diagnostics::{Diagnostic, DiagnosticFile, Message, messages};
use tsr_module::types::PackageId;
use tsr_parser::SpecifierContext;

use crate::Program;

/// A reference written in a program file (`referencedFileData`): the file it
/// is written in and its index among that file's references of its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencedFileData {
    /// The containing file's index in [`Program::source_files`]. Upstream
    /// keeps its path; an index is what every reader here wants, and what the
    /// loader can record without allocating (it records its own task index
    /// and maps it once files are numbered,
    /// [`FileIncludeReason::map_containing_file`]).
    pub file: usize,
    /// The reference's index among the containing file's references of the
    /// same kind (`ReferencedFiles`, `TypeReferenceDirectives`,
    /// `LibReferenceDirectives`).
    pub index: usize,
}

/// Which import of a file an import reason names.
///
/// Upstream stores the index into `file.Imports()`, or the synthesized
/// specifier node for an `importHelpers` / JSX-runtime import. This port's
/// loader collects specifiers without their nodes
/// ([`tsr_parser::ModuleSpecifier`]), so the specifier keeps what locates its
/// literal again ([`reference_location`]) and, for a synthetic one, which of
/// the two synthesized imports it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportReference {
    /// One of the file's own imports.
    Specifier {
        /// Its index among the file's imports (`importIndex`).
        index: usize,
        /// [`tsr_parser::ModuleSpecifier::pos`].
        pos: u32,
        /// What syntax wrote it.
        context: SpecifierContext,
    },
    /// The loader's synthesized `importHelpers` or JSX-runtime import.
    Synthetic {
        /// The specifier text.
        text: String,
        /// Whether it is the `importHelpers` import
        /// (`program.importHelpersImportSpecifiers[file] == node`).
        import_helpers: bool,
    },
}

/// Why a file is in the program (`compiler.FileIncludeReason`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileIncludeReason {
    /// `fileIncludeKindImport`.
    Import {
        /// The importing file's index, as [`ReferencedFileData::file`].
        file: usize,
        /// Which import.
        specifier: ImportReference,
        /// The resolution's package identity, read upstream from
        /// `GetResolvedModuleFromModuleSpecifier` when the location is built.
        package_id: PackageId,
    },
    /// `fileIncludeKindReferenceFile`: `/// <reference path="…" />`.
    ReferenceFile(ReferencedFileData),
    /// `fileIncludeKindTypeReferenceDirective`: `/// <reference types="…" />`.
    TypeReferenceDirective {
        /// Where it is written.
        reference: ReferencedFileData,
        /// The resolution's package identity.
        package_id: PackageId,
    },
    /// `fileIncludeKindLibReferenceDirective`: `/// <reference lib="…" />`.
    LibReferenceDirective(ReferencedFileData),
    /// `fileIncludeKindRootFile`, with its index among the root file names.
    RootFile {
        /// The index into the program's root file names.
        index: usize,
    },
    /// `fileIncludeKindLibFile`: the `lib` entry that named it, or `None` for
    /// the default library.
    LibFile {
        /// The index into `options.lib`.
        index: Option<usize>,
    },
    /// `fileIncludeKindAutomaticTypeDirectiveFile`.
    AutomaticTypeDirectiveFile {
        /// The `types` entry or `@types` package name.
        type_reference: String,
        /// The resolution's package identity.
        package_id: PackageId,
    },
}

impl FileIncludeReason {
    /// Rewrite the containing file of a referenced-file reason, from the
    /// loader's task numbering to the program's file numbering.
    pub(crate) fn map_containing_file(&mut self, map: impl Fn(usize) -> usize) {
        match self {
            Self::Import { file, .. }
            | Self::ReferenceFile(ReferencedFileData { file, .. })
            | Self::TypeReferenceDirective { reference: ReferencedFileData { file, .. }, .. }
            | Self::LibReferenceDirective(ReferencedFileData { file, .. }) => *file = map(*file),
            _ => {}
        }
    }

    /// `FileIncludeReason.isReferencedFile`: one of the four kinds written in a
    /// program file.
    #[must_use]
    pub fn is_referenced_file(&self) -> bool {
        matches!(
            self,
            Self::Import { .. }
                | Self::ReferenceFile(_)
                | Self::TypeReferenceDirective { .. }
                | Self::LibReferenceDirective(_)
        )
    }
}

/// Where a referenced-file reason is written (`referenceFileLocation`).
#[derive(Debug, Clone)]
pub struct ReferenceLocation {
    /// The containing file's index in [`Program::source_files`].
    pub file_index: usize,
    /// The specifier literal's span, or the quoted reference value's.
    /// `None` for a synthetic import, which has no position.
    pub span: Option<Span>,
    /// `referenceFileLocation.text()`: the written literal with its quotes,
    /// or `"<text>"` for a synthetic import.
    pub text: String,
    /// The resolution's package identity.
    pub package_id: PackageId,
    /// Whether the reference is the loader's synthesized import.
    pub is_synthetic: bool,
}

impl ReferenceLocation {
    /// `referenceFileLocation.diagnosticAt`.
    fn diagnostic_at(
        &self,
        program: &Program<'_>,
        message: &'static Message,
        args: Vec<String>,
    ) -> Diagnostic {
        let file = &program.source_files()[self.file_index];
        let mut diagnostic = Diagnostic::with_args(message, self.span.unwrap_or_default(), args);
        diagnostic.set_file(Arc::new(DiagnosticFile::new(file.file_name(), file.text())));
        diagnostic
    }
}

/// `FileIncludeReason.getReferencedLocation` (`fileInclude.go:105`).
///
/// `None` for a reason that is not a referenced-file reason, or whose
/// containing file is not in the program.
#[must_use]
pub fn reference_location(
    program: &Program<'_>,
    reason: &FileIncludeReason,
) -> Option<ReferenceLocation> {
    let (file_index, package_id) = match reason {
        FileIncludeReason::Import { file, package_id, .. }
        | FileIncludeReason::TypeReferenceDirective {
            reference: ReferencedFileData { file, .. },
            package_id,
        } => (*file, package_id.clone()),
        FileIncludeReason::ReferenceFile(reference)
        | FileIncludeReason::LibReferenceDirective(reference) => {
            (reference.file, PackageId::default())
        }
        _ => return None,
    };
    let source = program.source_files().get(file_index)?;
    let directive = |list: &[tsr_parser::pragma::FileReference], index: usize| {
        let reference = list.get(index)?;
        Some(ReferenceLocation {
            file_index,
            span: Some(reference.span),
            text: source.text()[reference.span.start as usize..reference.span.end as usize]
                .to_string(),
            package_id: package_id.clone(),
            is_synthetic: false,
        })
    };
    match reason {
        FileIncludeReason::Import { specifier, .. } => match specifier {
            ImportReference::Synthetic { text, .. } => Some(ReferenceLocation {
                file_index,
                span: None,
                text: format!("\"{text}\""),
                package_id,
                is_synthetic: true,
            }),
            ImportReference::Specifier { pos, context, .. } => {
                let literal = specifier_literal(program, file_index, *pos, *context)?;
                let span = program.nodes().span(literal);
                Some(ReferenceLocation {
                    file_index,
                    span: Some(span),
                    text: source.text()[span.start as usize..span.end as usize].to_string(),
                    package_id,
                    is_synthetic: false,
                })
            }
        },
        FileIncludeReason::ReferenceFile(reference) => {
            directive(&source.file_references().referenced_files, reference.index)
        }
        FileIncludeReason::TypeReferenceDirective { reference, .. } => {
            directive(&source.file_references().type_reference_directives, reference.index)
        }
        FileIncludeReason::LibReferenceDirective(reference) => {
            directive(&source.file_references().lib_reference_directives, reference.index)
        }
        _ => None,
    }
}

/// The literal node of an import specifier, upstream's `file.Imports()[index]`.
///
/// The loader recorded where the specifier's *enclosing* syntax starts and
/// what syntax it is ([`tsr_parser::ModuleSpecifier`]); the literal is that
/// syntax's module-specifier slot, read from the typed node:
///
/// - an import/export declaration or `import x = require()` starts at `pos`;
///   its specifier is its `module_specifier` / external module reference;
/// - an `import("x")` type starts at `pos`; its specifier is its argument;
/// - a dynamic `import("x")` / `require("x")` argument, and a JSDoc `@import`
///   specifier, is the literal at `pos` itself.
///
/// One literal starts at a given position, and a declaration has one
/// specifier slot, so position and syntax identify it without its text.
fn specifier_literal(
    program: &Program<'_>,
    file_index: usize,
    pos: u32,
    context: SpecifierContext,
) -> Option<NodeId> {
    let file = &program.source_files()[file_index];
    let nodes = program.nodes();
    let node_map = program.node_map();
    let is_literal = |id: NodeId| {
        matches!(
            node_map.get(id),
            Some(Node::StringLiteral(_) | Node::NoSubstitutionTemplateLiteral(_))
        )
    };
    let slot = |id: NodeId| -> Option<NodeId> {
        let expression = match node_map.get(id)? {
            Node::ImportDeclaration(node) => node.module_specifier?,
            Node::ExportDeclaration(node) => node.module_specifier?,
            Node::ImportEqualsDeclaration(node) => match node.module_reference? {
                tsr_ast::ModuleReference::ExternalModuleReference(reference) => {
                    reference.expression?
                }
                _ => return None,
            },
            Node::ImportTypeNode(node) => match node.argument? {
                tsr_ast::TypeNode::LiteralTypeNode(literal) => {
                    return literal.literal?.node_id();
                }
                _ => return None,
            },
            _ => return None,
        };
        Node::from(expression).node_id()
    };
    let enclosing_kinds: &[SyntaxKind] = match context {
        SpecifierContext::ImportDeclaration => {
            &[SyntaxKind::ImportDeclaration, SyntaxKind::JSImportDeclaration]
        }
        SpecifierContext::ExportDeclaration => &[SyntaxKind::ExportDeclaration],
        SpecifierContext::ImportEquals => &[SyntaxKind::ImportEqualsDeclaration],
        SpecifierContext::ImportType => &[SyntaxKind::ImportType],
        _ => &[],
    };
    let mut literal_at_pos = None;
    for raw in file.node_range() {
        let id = NodeId::new(raw);
        if nodes.span(id).start != pos {
            continue;
        }
        if enclosing_kinds.contains(&nodes.kind(id))
            && let Some(literal) = slot(id)
            && is_literal(literal)
        {
            return Some(literal);
        }
        if literal_at_pos.is_none() && is_literal(id) {
            literal_at_pos = Some(id);
        }
    }
    literal_at_pos
}

/// `FileIncludeReason.toDiagnostic(program, false)` (`fileInclude.go:166`):
/// the chain entry explaining one reason, with file names as written.
#[must_use]
pub fn reason_diagnostic(program: &Program<'_>, reason: &FileIncludeReason) -> Diagnostic {
    let options = program.compiler_options();
    let compiler = |message: &'static Message, args: Vec<String>| {
        Diagnostic::with_args(message, Span::default(), args)
    };
    let with_package = |plain: &'static Message,
                        packaged: &'static Message,
                        mut args: Vec<String>,
                        id: &PackageId| {
        if id.name.is_empty() {
            compiler(plain, args)
        } else {
            args.push(id.to_string());
            compiler(packaged, args)
        }
    };
    if reason.is_referenced_file() {
        // `computeReferenceFileDiagnostic` (`fileInclude.go:241`).
        let Some(location) = reference_location(program, reason) else {
            return compiler(&messages::ROOT_FILE_SPECIFIED_FOR_COMPILATION, Vec::new());
        };
        let file_name = program.source_files()[location.file_index].file_name().to_string();
        let args = vec![location.text.clone(), file_name];
        return match reason {
            FileIncludeReason::Import { specifier, .. } => match specifier {
                ImportReference::Specifier { .. } => with_package(
                    &messages::IMPORTED_VIA_0_FROM_FILE_1,
                    &messages::IMPORTED_VIA_0_FROM_FILE_1_WITH_PACKAGEID_2,
                    args,
                    &location.package_id,
                ),
                ImportReference::Synthetic { import_helpers: true, .. } => with_package(
                    &messages::IMPORTED_VIA_0_FROM_FILE_1_TO_IMPORT_IMPORTHELPERS_AS_SPECIFIED_IN_COMPILEROPTIONS,
                    &messages::IMPORTED_VIA_0_FROM_FILE_1_WITH_PACKAGEID_2_TO_IMPORT_IMPORTHELPERS_AS_SPECIFIED_IN_COMPILEROPTIONS,
                    args,
                    &location.package_id,
                ),
                ImportReference::Synthetic { import_helpers: false, .. } => with_package(
                    &messages::IMPORTED_VIA_0_FROM_FILE_1_TO_IMPORT_JSX_AND_JSXS_FACTORY_FUNCTIONS,
                    &messages::IMPORTED_VIA_0_FROM_FILE_1_WITH_PACKAGEID_2_TO_IMPORT_JSX_AND_JSXS_FACTORY_FUNCTIONS,
                    args,
                    &location.package_id,
                ),
            },
            FileIncludeReason::ReferenceFile(_) => {
                compiler(&messages::REFERENCED_VIA_0_FROM_FILE_1, args)
            }
            FileIncludeReason::TypeReferenceDirective { .. } => with_package(
                &messages::TYPE_LIBRARY_REFERENCED_VIA_0_FROM_FILE_1,
                &messages::TYPE_LIBRARY_REFERENCED_VIA_0_FROM_FILE_1_WITH_PACKAGEID_2,
                args,
                &location.package_id,
            ),
            _ => compiler(&messages::LIBRARY_REFERENCED_VIA_0_FROM_FILE_1, args),
        };
    }
    match reason {
        // Upstream's no-config arm; the config arms are not ported (module
        // docs).
        FileIncludeReason::RootFile { .. } => {
            compiler(&messages::ROOT_FILE_SPECIFIED_FOR_COMPILATION, Vec::new())
        }
        FileIncludeReason::AutomaticTypeDirectiveFile { type_reference, package_id } => {
            let args = vec![type_reference.clone()];
            if options.uses_wildcard_types() {
                with_package(
                    &messages::ENTRY_POINT_FOR_IMPLICIT_TYPE_LIBRARY_0,
                    &messages::ENTRY_POINT_FOR_IMPLICIT_TYPE_LIBRARY_0_WITH_PACKAGEID_1,
                    args,
                    package_id,
                )
            } else {
                with_package(
                    &messages::ENTRY_POINT_OF_TYPE_LIBRARY_0_SPECIFIED_IN_COMPILEROPTIONS,
                    &messages::ENTRY_POINT_OF_TYPE_LIBRARY_0_SPECIFIED_IN_COMPILEROPTIONS_WITH_PACKAGEID_1,
                    args,
                    package_id,
                )
            }
        }
        FileIncludeReason::LibFile { index: Some(index) } => compiler(
            &messages::LIBRARY_0_SPECIFIED_IN_COMPILEROPTIONS,
            vec![options.lib.get(*index).cloned().unwrap_or_default()],
        ),
        _ => {
            let target = options.emit_script_target().to_string();
            if target.is_empty() {
                compiler(&messages::DEFAULT_LIBRARY, Vec::new())
            } else {
                compiler(&messages::DEFAULT_LIBRARY_FOR_TARGET_0, vec![target])
            }
        }
    }
}

/// `FileIncludeReason.toRelatedInfo` (`fileInclude.go:283`), for the
/// referenced-file reasons (`computeReferenceFileRelatedInfo`). The other
/// kinds point into the config file and are not ported (module docs).
#[must_use]
pub fn reason_related_info(
    program: &Program<'_>,
    reason: &FileIncludeReason,
) -> Option<Diagnostic> {
    let location = reference_location(program, reason)?;
    if location.is_synthetic {
        return None;
    }
    let message = match reason {
        FileIncludeReason::Import { .. } => &messages::FILE_IS_INCLUDED_VIA_IMPORT_HERE,
        FileIncludeReason::ReferenceFile(_) => &messages::FILE_IS_INCLUDED_VIA_REFERENCE_HERE,
        FileIncludeReason::TypeReferenceDirective { .. } => {
            &messages::FILE_IS_INCLUDED_VIA_TYPE_LIBRARY_REFERENCE_HERE
        }
        _ => &messages::FILE_IS_INCLUDED_VIA_LIBRARY_REFERENCE_HERE,
    };
    Some(location.diagnostic_at(program, message, Vec::new()))
}

/// `includeProcessor.explainRedirectAndImpliedFormat` (`includeprocessor.go:123`)
/// for a program file: why it has the module format it has.
///
/// Project-reference outputs and redirect files have no producer in this
/// program, so only the implied-format arm can answer.
#[must_use]
pub fn explain_implied_format(program: &Program<'_>, file_index: usize) -> Vec<Diagnostic> {
    let file = &program.source_files()[file_index];
    let Some(node) = file.source_file().node_id else { return Vec::new() };
    // `ast.IsExternalOrCommonJSModule`: the binder gives a module's
    // `SourceFile` a symbol and a script none (`checkSourceFile`'s test in
    // `tsr-checker`'s `unused.rs`).
    if program.binder().symbol_of(node).is_none() {
        return Vec::new();
    }
    let Some(metadata) = program.meta_data(file_index) else { return Vec::new() };
    let package_json = || format!("{}/package.json", metadata.package_json_directory);
    let compiler = |message: &'static Message, args: Vec<String>| {
        Diagnostic::with_args(message, Span::default(), args)
    };
    match program.implied_node_format_for_emit(node) {
        ResolutionMode::ESNext if metadata.package_json_type == "module" => vec![compiler(
            &messages::FILE_IS_ECMASCRIPT_MODULE_BECAUSE_0_HAS_FIELD_TYPE_WITH_VALUE_MODULE,
            vec![package_json()],
        )],
        ResolutionMode::CommonJS if !metadata.package_json_type.is_empty() => vec![compiler(
            &messages::FILE_IS_COMMONJS_MODULE_BECAUSE_0_HAS_FIELD_TYPE_WHOSE_VALUE_IS_NOT_MODULE,
            vec![package_json()],
        )],
        ResolutionMode::CommonJS if !metadata.package_json_directory.is_empty() => vec![compiler(
            &messages::FILE_IS_COMMONJS_MODULE_BECAUSE_0_DOES_NOT_HAVE_FIELD_TYPE,
            vec![package_json()],
        )],
        ResolutionMode::CommonJS => vec![compiler(
            &messages::FILE_IS_COMMONJS_MODULE_BECAUSE_PACKAGE_JSON_WAS_NOT_FOUND,
            Vec::new(),
        )],
        _ => Vec::new(),
    }
}

/// `processingDiagnostic.createDiagnosticExplainingFile`
/// (`processingDiagnostic.go:69`) for a diagnostic about program file
/// `file_index` with no reason of its own (`diagnosticReason == nil`).
///
/// Returns the file the diagnostic is positioned in — the first written,
/// non-synthetic reference to the subject — or `None` for a global
/// diagnostic, and the diagnostic itself.
#[must_use]
pub fn diagnostic_explaining_file(
    program: &Program<'_>,
    file_index: usize,
    message: &'static Message,
    args: Vec<String>,
) -> (Option<usize>, Diagnostic) {
    let reasons = program.file_include_reasons(file_index);
    let mut preferred: Option<(usize, ReferenceLocation)> = None;
    let mut include_details = Vec::with_capacity(reasons.len());
    let mut related = Vec::new();
    // `seenReasons` dedupes by reason identity; each recorded reason here is
    // its own entry, as each edge upstream allocates its own reason.
    for (position, reason) in reasons.iter().enumerate() {
        include_details.push(reason_diagnostic(program, reason));
        // `processRelatedInfo`.
        if preferred.is_none()
            && reason.is_referenced_file()
            && let Some(location) = reference_location(program, reason)
            && !location.is_synthetic
        {
            preferred = Some((position, location));
        } else if preferred.as_ref().is_none_or(|(chosen, _)| *chosen != position)
            && let Some(info) = reason_related_info(program, reason)
        {
            related.push(info);
        }
    }
    let redirect_info = explain_implied_format(program, file_index);

    let mut chain = Vec::new();
    // Upstream's `includeDetails` is a non-nil (possibly empty) slice whenever
    // the diagnostic names a file, so only the preferred-location test gates.
    if preferred.is_none() || reasons.len() != 1 {
        let mut file_reason =
            Diagnostic::new(&messages::THE_FILE_IS_IN_THE_PROGRAM_BECAUSE_COLON, Span::default());
        file_reason.set_message_chain(include_details);
        chain.push(file_reason);
    }
    chain.extend(redirect_info);

    let (located_in, mut result) = match &preferred {
        Some((_, location)) => (
            Some(location.file_index),
            Diagnostic::with_args(message, location.span.unwrap_or_default(), args),
        ),
        None => (None, Diagnostic::with_args(message, Span::default(), args)),
    };
    if !chain.is_empty() {
        result.set_message_chain(chain);
    }
    if !related.is_empty() {
        result.set_related_information(Arc::new(related));
    }
    (located_in, result)
}
