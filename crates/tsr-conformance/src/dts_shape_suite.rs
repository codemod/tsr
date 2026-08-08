//! `dts_shape`: which declarations survive into the `.d.ts`, and in what order.
//!
//! # Why a weaker gate is worth having beside a stronger one
//!
//! [`crate::dts_emit_suite`] compares bytes, which means it can only judge a case
//! where **nothing needs inference** — 488 of them. The 650 it skips still have a
//! `.d.ts` baseline, and what those need a checker for is the *types*. Which
//! declarations survive, and in what order, is decidable from syntax:
//!
//! ```text
//! export declare class Foo {          ← dts_shape compares this line's shape
//!     bar: SomeInferredType;          ← and deliberately not this one
//! }
//! ```
//!
//! That matters because visibility, elision and ordering are exactly where this
//! port approximates. `EmitResolver.IsDeclarationVisible` is checker-backed
//! upstream and is reachability-from-the-exports here
//! (`docs/architecture/isolated-declarations.md`), and every bug found in it so
//! far — the elided namespace bodies, the dropped side-effect imports, the missing
//! scope markers — was a *structural* bug that a byte comparison happened to catch
//! only on the cases it could reach.
//!
//! # The denominator does not move when the analysis does
//!
//! This is the property that justifies a fourth number rather than a wider third
//! one. `dts_emit`'s denominator is the reachable set, so it moves whenever
//! `tsr_dts` changes — a coupling that document argues for and accepts. This
//! suite's denominator is "every case with emitted declaration output", which
//! depends on nothing but the corpus. A regression here is always a regression in
//! the transform.
//!
//! # What a pass means, exactly
//!
//! The **kind and name of every declaration**, in source order, recursing into
//! namespace bodies and stopping at class and interface bodies. Namespaces recurse
//! because an emptied namespace body was a real defect that printed
//! `declare namespace M {}` for every namespace in the corpus. Class bodies do not,
//! because that is where the types live and where inference bites — a suite that
//! descended into them would be `dts_emit` with extra steps, and would stop being
//! judgeable on the cases it exists to judge.
//!
//! Formatting, types, modifiers and comments are all invisible here. That is the
//! point: this suite says *the right things are present*, and `dts_emit` says
//! *they are spelled right*.

use tsr_core::Arena;
use tsr_parser::ScriptKind;

use crate::{
    CaseEntry,
    js_baseline::JsBaseline,
    suite::{Outcome, Suite},
};

/// The `dts_shape` suite.
pub struct DtsShape;

impl Suite for DtsShape {
    fn name(&self) -> &'static str {
        "dts_shape"
    }

    fn describes(&self) -> &'static str {
        "the declarations this port emits into a .d.ts — their kind, name and order, \
         recursing into namespaces — match upstream's, ignoring types and formatting; \
         judged on every case with declaration output, including the ones dts_emit \
         cannot reach because they need inference"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if !case.has_any_baseline() {
            return Outcome::Skipped { reason: "upstream recorded no output for this case".into() };
        }
        let Ok(text) = std::fs::read_to_string(case.baseline_path("js")) else {
            return Outcome::Skipped { reason: "upstream recorded no .js emit baseline".into() };
        };
        let baseline = JsBaseline::parse(&text);
        let Ok(parsed_case) = case.load() else {
            return Outcome::Failed { reason: "case did not load".into() };
        };

        // The denominator is decided here, from the baseline alone, before
        // anything is emitted — see [`crate::dts_emit_suite::output_units`] for the
        // mutation that showed why that matters.
        let units = crate::dts_emit_suite::output_units(&baseline, &parsed_case);
        if units.is_empty() {
            return Outcome::Skipped {
                reason: "the emit baseline has no emitted .d.ts section".into(),
            };
        }

        // Every skip is decided before any comparison — see
        // [`crate::dts_emit_suite`]'s note on why short-circuiting made the
        // denominator depend on the emitter's own output.
        let mut pairs = Vec::with_capacity(units.len());
        for (unit, expected) in &units {
            // Upstream's own baseline must parse for its shape to be readable. A
            // baseline this port cannot reparse is a fact about the parser, not
            // about the emitter, so it is a skip rather than a failure.
            let Some(want) = shape_of(&expected.content) else {
                return Outcome::Skipped {
                    reason: "upstream's .d.ts baseline does not reparse".into(),
                };
            };
            let kind = ScriptKind::from_file_name(&unit.name);
            let arena = Arena::new();
            let parsed = tsr_parser::parse_with_script_kind(&arena, &unit.content, kind);
            if !parsed.diagnostics.is_empty() {
                return Outcome::Skipped {
                    reason: "a unit of this case does not parse cleanly".into(),
                };
            }
            let references = crate::dts_emit_suite::declaration_references(&parsed.file_references);
            let mut nodes = parsed.nodes;
            crate::dts_emit_suite::stamp_javascript_root(
                &unit.name,
                parsed.source_file,
                &mut nodes,
            );
            let result = tsr_declarations::emit_with_references_and_options(
                &arena,
                &mut nodes,
                parsed.source_file,
                &references,
                crate::dts_emit_suite::declaration_emit_options(
                    &parsed_case,
                    &unit.name,
                    &unit.content,
                ),
            );
            if let Some(kind) = result.unsupported.first() {
                return Outcome::Unsupported { reason: format!("printer: {kind}") };
            }
            // Deliberately *not* skipped for `result.diagnostics` — the cases that
            // need inference are the ones this suite exists to judge.
            let Some(got) = shape_of(&result.text) else {
                return Outcome::Failed { reason: "the emitted .d.ts does not reparse".into() };
            };
            pairs.push((want, got));
        }

        for (want, got) in &pairs {
            if want != got {
                return Outcome::Failed { reason: first_difference(want, got) };
            }
        }

        // Over-emission: a unit upstream produced no declaration file for, that
        // this port emits into anyway. Checked after the comparisons so a genuine
        // text difference is reported in preference to it.
        for unit in crate::dts_emit_suite::unemitted_units(&baseline, &parsed_case) {
            if crate::dts_emit_suite::emits_anything(unit) {
                return Outcome::Failed {
                    reason: format!(
                        "emitted {}, upstream emitted no declaration file",
                        crate::dts_emit_suite::declaration_name(&unit.name)
                    ),
                };
            }
        }
        Outcome::Passed
    }
}

/// The declaration shape of a `.d.ts`, as a flat sequence.
///
/// `None` if the text does not parse, which for upstream's baseline is a fact
/// about this port's parser rather than about the emitter.
fn shape_of(text: &str) -> Option<Vec<String>> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, text);
    if !parsed.diagnostics.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    push_statements(parsed.source_file.statements, 0, &mut out);
    Some(out)
}

fn push_statements(statements: &[tsr_ast::Statement<'_>], depth: usize, out: &mut Vec<String>) {
    for statement in statements {
        out.push(format!("{}{}", "  ".repeat(depth), describe(statement)));
        // Namespaces recurse; class and interface bodies do not. See the module
        // docs for why the line is drawn there.
        if let tsr_ast::Statement::ModuleDeclaration(module) = statement {
            match &module.body {
                Some(tsr_ast::ModuleBody::ModuleBlock(block)) => {
                    push_statements(block.statements, depth + 1, out);
                }
                Some(tsr_ast::ModuleBody::ModuleDeclaration(inner)) => {
                    let inner = tsr_ast::Statement::ModuleDeclaration(inner);
                    push_statements(std::slice::from_ref(&inner), depth + 1, out);
                }
                None => {}
            }
        }
    }
}

/// One declaration's kind and name.
///
/// A variable statement lists every name it declares, because
/// `declare var x, y` and `declare var x` differ in what they export even though
/// they are one statement.
fn describe(statement: &tsr_ast::Statement<'_>) -> String {
    use tsr_ast::Statement as S;
    let named = |kind: &str, name: Option<&tsr_ast::Identifier<'_>>| {
        format!("{kind} {}", name.map_or("<anonymous>", |name| name.text))
    };
    match statement {
        S::ClassDeclaration(node) => named("class", node.name),
        S::FunctionDeclaration(node) => named("function", node.name),
        S::InterfaceDeclaration(node) => named("interface", node.name),
        S::TypeAliasDeclaration(node) => named("type", node.name),
        S::EnumDeclaration(node) => named("enum", node.name),
        S::ImportEqualsDeclaration(node) => named("import=", node.name),
        S::ModuleDeclaration(node) => match &node.name {
            Some(tsr_ast::ModuleName::Identifier(name)) => format!("namespace {}", name.text),
            Some(tsr_ast::ModuleName::StringLiteral(name)) => format!("module \"{}\"", name.text),
            None => "namespace <anonymous>".to_string(),
        },
        S::VariableStatement(node) => {
            let declared: Vec<&str> = node
                .declaration_list
                .map(|list| {
                    list.declarations
                        .iter()
                        .map(|declaration| match &declaration.name {
                            Some(tsr_ast::BindingName::Identifier(bound)) => bound.text,
                            _ => "<pattern>",
                        })
                        .collect()
                })
                .unwrap_or_default();
            format!("var {}", declared.join(", "))
        }
        S::ImportDeclaration(_) => "import".to_string(),
        S::ExportDeclaration(_) => "export{}".to_string(),
        S::ExportAssignment(_) => "export=".to_string(),
        // Anything else in a `.d.ts` is a defect in itself — a runtime statement
        // that should have been elided — so it is named rather than ignored.
        other => format!("<unexpected {:?}>", std::mem::discriminant(other)),
    }
}

/// The first position where the two sequences diverge.
fn first_difference(want: &[String], got: &[String]) -> String {
    for (index, expected) in want.iter().enumerate() {
        match got.get(index) {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return format!("#{index}: want `{expected}`, got `{actual}`");
            }
            None => return format!("#{index}: missing `{expected}`"),
        }
    }
    let extra = got.get(want.len()).map_or("", String::as_str);
    format!("#{}: extra `{extra}`", want.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_is_kinds_and_names_with_namespaces_nested() {
        let shape = shape_of(
            "declare class A {\n    x: number;\n}\ndeclare namespace N {\n    interface I {\n    }\n}\nexport {};\n",
        )
        .expect("parses");
        assert_eq!(
            shape,
            vec![
                "class A".to_string(),
                "namespace N".to_string(),
                "  interface I".to_string(),
                "export{}".to_string(),
            ],
            "a class body is opaque; a namespace body is not"
        );
    }

    #[test]
    fn types_and_formatting_do_not_change_the_shape() {
        // The whole point: two declaration files that differ in every type still
        // have the same shape, which is what lets this suite judge a case whose
        // types needed a checker.
        let a = shape_of("declare const x: SomeInferredType;\n").expect("parses");
        let b = shape_of("declare  const   x :   number ;\n").expect("parses");
        assert_eq!(a, b);
    }

    #[test]
    fn a_dropped_or_reordered_declaration_is_a_difference() {
        let want = shape_of("declare class A {\n}\ndeclare class B {\n}\n").expect("parses");
        let dropped = shape_of("declare class A {\n}\n").expect("parses");
        let reordered = shape_of("declare class B {\n}\ndeclare class A {\n}\n").expect("parses");
        assert_eq!(first_difference(&want, &dropped), "#1: missing `class B`");
        assert_eq!(first_difference(&want, &reordered), "#0: want `class A`, got `class B`");
    }

    #[test]
    fn every_name_of_a_multi_declarator_statement_is_recorded() {
        // `declare var x, y` and `declare var x` are one statement each and export
        // different things.
        let two = shape_of("declare var x: number, y: string;\n").expect("parses");
        let one = shape_of("declare var x: number;\n").expect("parses");
        assert_eq!(two, vec!["var x, y".to_string()]);
        assert_ne!(two, one);
    }
}
