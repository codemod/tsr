//! Which rule printed `any` — the audit `bd tsr-7xs` asks for.
//!
//! `cargo run -p tsr-conformance --example any_audit --release`
//!
//! # The question, and the question this does *not* answer
//!
//! `examples/rank_board.rs` measured, at `33e3bd5`, that 21,685 aligned lines
//! are banked because we print `any` and upstream printed `any` too, and that
//! 7,288 are lost because we print `any` and upstream printed something else.
//! `docs/adr/0038` and `docs/adr/0039` both turn on refusing to answer `any`
//! where nothing was computed, precisely because a blanket substitution scores
//! as *right* on every line where upstream genuinely computed `any`. If a
//! material share of the 21,685 is a **defaulted** `any` rather than a
//! **computed** one, the gradient is overstated by that share.
//!
//! This attributes every such line to **the rule that minted the printed
//! `any`** — the last step, the one that put the string there. That is a
//! different question from "what would upstream have said here", which no probe
//! on this side of the port can answer, and from "where did the `any` that
//! flowed into this rule come from", which the propagating rows below name but
//! do not resolve. Stating it matters: summing the rows answers *"which rule
//! prints `any` in this corpus"*, and nothing else.
//!
//! # Two populations that look identical in the baseline and are not
//!
//! `any` reaches a baseline line by two entirely separate routes in this port,
//! and lumping them is how the audit would go wrong:
//!
//! - **CHECKER** — [`tsr_checker`] answered `intrinsics.any` for the node. There
//!   are twelve sites in the checker that can do this (`grep -n 'intrinsics\.any'
//!   crates/tsr-checker/src`), and the rows below name which one.
//! - **WRITER** — the checker answered `error`, and
//!   `types_producer::type_at_location` printed `any` because **upstream's
//!   baseline writer does**, at a position where upstream's own checker also
//!   holds `errorType` (`type_symbol_baseline.go:380`). The qualified-name left,
//!   the property name of a binding element, a label name and an intrinsic JSX
//!   tag are the four ported so far. Nothing was computed by *either* compiler
//!   on these lines; they are a rendering port, not a type answer, and they must
//!   not be counted as evidence either way about the checker's discipline.
//!
//! # The control bucket
//!
//! `UNCLASSIFIED` is printed unconditionally and must read zero. This classifier
//! mirrors `type_at_location`'s branch order — the same discipline
//! `types_producer::gap_reason` documents, and for the same reason: an earlier
//! version of *that* function did not follow the code it explained and invented
//! a 22,768-line finding. A non-zero `UNCLASSIFIED` means this file and the
//! producer have drifted and no row below can be trusted.
//!
//! A second control, `DISAGREEMENT`, counts lines where this classifier reached
//! a branch whose type is not `any` although the producer printed `any`. It is
//! the same invariant from the other side and must also read zero.

use std::collections::HashMap;

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Where the printed `any` came from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bucket {
    /// The checker answered `intrinsics.any`.
    Checker,
    /// The checker answered `error`; upstream's baseline writer prints `any`
    /// here and this port reproduces that.
    Writer,
    /// Not the `any` type at all: a *declaration named* `any`, whose printed
    /// name is the same string. `class any {}` in
    /// `compiler/primitiveTypeAsClassName` is the shape. Neither computed nor
    /// defaulted — the string collides.
    Named,
    /// Neither. The control.
    Unclassified,
}

impl Bucket {
    fn label(self) -> &'static str {
        match self {
            Self::Checker => "CHECKER",
            Self::Writer => "WRITER",
            Self::Named => "NAMED-any",
            Self::Unclassified => "UNCLASSIFIED",
        }
    }
}

/// One row: the lines it holds and the cases they came from.
#[derive(Default)]
struct Row {
    lines: usize,
    by_case: HashMap<String, usize>,
}

impl Row {
    fn add(&mut self, case: &str) {
        self.lines += 1;
        *self.by_case.entry(case.to_string()).or_default() += 1;
    }

    fn merge(&mut self, other: &Self) {
        self.lines += other.lines;
        for (case, count) in &other.by_case {
            *self.by_case.entry(case.clone()).or_default() += count;
        }
    }

    /// Case count, top-1 share, top-10 share — the concentration check
    /// `docs/conventions.md` requires beside every row size.
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.by_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let top1 = counts.first().copied().unwrap_or(0);
        let top10: usize = counts.iter().take(10).sum();
        (counts.len(), pct(top1, self.lines), pct(top10, self.lines))
    }
}

#[derive(Default)]
struct CaseReport {
    aligned: usize,
    matched: usize,
    expected: usize,
    /// Banked: we print `any`, upstream printed `any`.
    banked: HashMap<(&'static str, String), Row>,
    /// Lost: we print `any`, upstream printed something else.
    lost: HashMap<(&'static str, String), Row>,
    /// What upstream printed on the lost lines.
    lost_wants: HashMap<String, usize>,
    disagreement: usize,
    debug: Vec<String>,
    /// Per-line attribution for the LOST lines, emitted only under
    /// `TSR_ANY_DUMP=1`. The key is `case:index:position`, byte-for-byte the
    /// one `verdict.rs:70` writes, so this joins against
    /// `target/verdict_baseline.tsv` without a fuzzy match. Written because
    /// the ranked rows above are diffuse (top-1 under 4%): the forecastable
    /// quantity is not a row's line count but its intersection with the
    /// single-transition population, and only a join can compute that.
    dump: Vec<String>,
}

#[allow(clippy::cast_precision_loss)]
fn pct(part: usize, whole: usize) -> f64 {
    if whole == 0 { 0.0 } else { 100.0 * part as f64 / whole as f64 }
}

/// `is_type_declaration` (`types_producer.rs:978`), mirrored.
fn is_type_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::TypeParameter
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::EnumDeclaration
    )
}

/// `is_label_name` (`types_producer.rs:648`), mirrored.
fn is_label_name(id: NodeId, nodes: &NodeTable, map: &NodeMap<'_>) -> bool {
    if nodes.kind(id) != SyntaxKind::Identifier {
        return false;
    }
    let Some(parent) = nodes.parent(id) else { return false };
    let label = match map.get(parent) {
        Some(Node::LabeledStatement(statement)) => statement.label,
        Some(Node::BreakStatement(statement)) => statement.label,
        Some(Node::ContinueStatement(statement)) => statement.label,
        _ => return false,
    };
    label.and_then(|label| label.node_id) == Some(id)
}

/// `jsx_tag_name_of` (`types_producer.rs:672`), mirrored.
fn jsx_tag_name_of(element: NodeId, map: &NodeMap<'_>) -> Option<NodeId> {
    match map.get(element)? {
        Node::JsxOpeningElement(n) => n.tag_name.and_then(|tag| tag.node_id()),
        Node::JsxClosingElement(n) => n.tag_name.and_then(|tag| tag.node_id()),
        Node::JsxSelfClosingElement(n) => n.tag_name.and_then(|tag| tag.node_id()),
        _ => None,
    }
}

/// `is_intrinsic_jsx_name` (`types_producer.rs:690`), mirrored.
fn is_intrinsic_jsx_name(name: &str) -> bool {
    name.starts_with(|first: char| first.is_ascii_lowercase()) || name.contains('-')
}

struct Ctx<'a, 'b, 'c> {
    checker: &'a mut tsr_checker::Checker<'b, 'c>,
    binder: &'a tsr_binder::BindResult<'b>,
    nodes: &'a NodeTable,
    map: &'a NodeMap<'b>,
}

impl Ctx<'_, '_, '_> {
    fn is_any(&self, id: tsr_checker::TypeId) -> bool {
        id == self.checker.intrinsics().any
    }

    fn is_error(&self, id: tsr_checker::TypeId) -> bool {
        id == self.checker.intrinsics().error
    }

    /// Walk down a chain of accesses on an `any` receiver to the node that
    /// **minted** the `any`, and describe that node's own arm.
    ///
    /// `a.b.c` is three lines and two of them say "the receiver is `any`", which
    /// answers *which rule printed this line* and not *whose `any` it is*. The
    /// ownership claim in `crates/tsr-checker/src/members.rs` — that its wrong
    /// lines belong to contextual typing rather than to the arm itself — is a
    /// claim about the **origin**, so it needs this walk to be tested at all.
    fn receiver_origin(&mut self, mut id: NodeId) -> String {
        for _ in 0..64 {
            let Some(node) = self.map.get(id) else { break };
            let receiver = match node {
                Node::PropertyAccessExpression(access) => access.expression,
                Node::ElementAccessExpression(access) => access.expression,
                _ => None,
            };
            let Some(receiver) = receiver else { break };
            let ours = self.checker.check_expression(receiver);
            if !self.is_any(ours) {
                break;
            }
            let Some(next) = receiver.node_id() else { break };
            id = next;
        }
        let (_, reason) = self.classify(id);
        reason
    }

    /// Whether an unannotated parameter's own function is a form upstream can
    /// contextually type.
    ///
    /// The fence is `crate::expressions`'s, and it is upstream's: a function
    /// *declaration* and a class method cannot be contextually typed, while a
    /// function expression, an arrow and an object-literal method can
    /// (`checker.go:29496`, `isContextSensitiveFunctionOrObjectLiteralMethod`;
    /// `checker.go:10349`, `assignContextualParameterTypes`). Inside a
    /// contextualisable container, upstream may have inferred a real type where
    /// this port fell to the implicit `any`; outside one, upstream's answer is
    /// the implicit `any` too.
    ///
    /// This is upstream's predicate with its second conjunct dropped:
    /// `isContextSensitiveFunctionOrObjectLiteralMethod` also requires
    /// `isContextSensitiveFunctionLikeDeclaration` (`checker.go:30931`). Keeping
    /// only the container test makes this a **superset**, so the row it produces
    /// is an upper bound on the exposure and never an under-count.
    fn in_contextualisable_container(&self, parameter: NodeId) -> bool {
        let Some(container) = self.nodes.parent(parameter) else { return false };
        match self.nodes.kind(container) {
            SyntaxKind::FunctionExpression | SyntaxKind::ArrowFunction => true,
            SyntaxKind::MethodDeclaration => matches!(
                self.nodes.parent(container).map(|owner| self.nodes.kind(owner)),
                Some(SyntaxKind::ObjectLiteralExpression)
            ),
            _ => false,
        }
    }

    /// Which `get_type_of_symbol` arm can have produced `any` for this symbol.
    ///
    /// Mirrors the dispatch at `crates/tsr-checker/src/symbols.rs:29` in its
    /// order, because the order is what decides which worker ran.
    fn symbol_arm(&mut self, symbol: tsr_binder::SymbolId) -> String {
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::ACCESSOR) {
            return "accessor with no annotation and no getter body (symbols.rs:271, checker.go:18545)".to_string();
        }
        if flags.intersects(SymbolFlags::VARIABLE | SymbolFlags::PROPERTY) {
            let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
                return "variable worker, no value declaration (unreachable: answers error)"
                    .to_string();
            };
            let kind = self.nodes.kind(declaration);
            let node = self.map.get(declaration);
            let annotation = node.and_then(|node| node.type_id());
            let initializer = node.and_then(|node| node.initializer_id());
            return match kind {
                SyntaxKind::VariableDeclaration
                | SyntaxKind::Parameter
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature => match (annotation, initializer) {
                    (Some(_), _) => {
                        "annotation denotes `any` (declared.rs:27, checker.go:22811)".to_string()
                    }
                    (None, Some(initializer)) => {
                        let ours = match self
                            .map
                            .get(initializer)
                            .and_then(|node| tsr_ast::Expression::try_from(node).ok())
                        {
                            Some(expression) => self.checker.check_expression(expression),
                            None => self.checker.intrinsics().error,
                        };
                        if self.is_any(ours) {
                            format!("initialiser is `any` (propagated into {kind:?})")
                        } else {
                            "self-referential initialiser: reportCircularityError (symbols.rs:767, checker.go:18822)".to_string()
                        }
                    }
                    (None, None) if kind == SyntaxKind::Parameter => format!(
                        "implicit `any`: unannotated Parameter, container {} (symbols.rs:780, checker.go:18264)",
                        if self.in_contextualisable_container(declaration) {
                            "CONTEXTUALISABLE"
                        } else {
                            "cannot be contextually typed"
                        }
                    ),
                    (None, None) => format!(
                        "implicit `any`: {kind:?} with no annotation and no initialiser (symbols.rs:780, checker.go:18264)"
                    ),
                },
                SyntaxKind::PropertyAssignment | SyntaxKind::ShorthandPropertyAssignment => {
                    format!("object-literal member initialiser is `any` ({kind:?})")
                }
                _ => format!("variable worker on {kind:?} (unreachable: answers error)"),
            };
        }
        if flags.intersects(
            SymbolFlags::FUNCTION
                | SymbolFlags::METHOD
                | SymbolFlags::CLASS
                | SymbolFlags::ENUM
                | SymbolFlags::VALUE_MODULE,
        ) {
            return "shorthand ambient module (symbols.rs:563, utilities.go:198)".to_string();
        }
        if flags.intersects(SymbolFlags::ENUM_MEMBER) {
            return "enum member is `any`".to_string();
        }
        if flags.intersects(SymbolFlags::ALIAS) {
            return "alias target is `any`".to_string();
        }
        if flags.contains(SymbolFlags::EXPORT_VALUE) {
            return "export marker, shared declaration is `any`".to_string();
        }
        format!("symbol flags with no arm: {flags:?}")
    }

    /// Which checker rule minted the `any` for an expression node.
    fn expression_arm(&mut self, id: NodeId, expression: tsr_ast::Expression<'_>) -> String {
        let kind = self.nodes.kind(id);
        match expression {
            tsr_ast::Expression::Identifier(identifier) => {
                match identifier.node_id.and_then(|node| {
                    self.binder.resolve_name(
                        self.nodes,
                        self.map,
                        node,
                        identifier.text,
                        SymbolFlags::VALUE,
                    )
                }) {
                    Some(symbol) => format!("identifier -> {}", self.symbol_arm(symbol)),
                    None => "identifier does not resolve (unreachable: answers error)".to_string(),
                }
            }
            tsr_ast::Expression::PropertyAccessExpression(access) => {
                let receiver = match access.expression {
                    Some(expression) => self.checker.check_expression(expression),
                    None => self.checker.intrinsics().error,
                };
                if self.is_any(receiver) {
                    format!(
                        "property access on an `any` receiver (members.rs:99, checker.go:11314) <- {}",
                        self.receiver_origin(id)
                    )
                } else {
                    "property access: the property's own type is `any`".to_string()
                }
            }
            tsr_ast::Expression::ElementAccessExpression(access) => {
                let receiver = match access.expression {
                    Some(expression) => self.checker.check_expression(expression),
                    None => self.checker.intrinsics().error,
                };
                if self.is_any(receiver) {
                    format!(
                        "element access on an `any` receiver (indexed.rs:107, checker.go:11266) <- {}",
                        self.receiver_origin(id)
                    )
                } else {
                    "element access: the indexed type is `any`".to_string()
                }
            }
            tsr_ast::Expression::BinaryExpression(binary) => {
                let operator = binary.operator_token.and_then(|token| token.node_id).map_or_else(
                    || "?".to_string(),
                    |token| format!("{:?}", self.nodes.kind(token)),
                );
                format!("binary {operator} with an `any` operand (binary.rs:220)")
            }
            tsr_ast::Expression::YieldExpression(_) => {
                "yield in a non-contextualisable container (expressions.rs:676, checker.go:11005)"
                    .to_string()
            }
            _ => format!("expression answered `any`: {kind:?}"),
        }
    }

    /// The origin of a printed `any`, mirroring `type_at_location`'s branch
    /// order exactly (`types_producer.rs:276`). The **first** branch that
    /// returns is the one that printed.
    fn classify(&mut self, id: NodeId) -> (Bucket, String) {
        let Some(node) = self.map.get(id) else {
            return (Bucket::Unclassified, "no node".to_string());
        };

        // 1. A type declaration's own name.
        if let Some(parent) = self.nodes.parent(id)
            && self.nodes.kind(id) == SyntaxKind::Identifier
            && is_type_declaration(self.nodes.kind(parent))
            && self.map.get(parent).and_then(|node| node.name_id()) == Some(id)
            && let Some(symbol) = self.binder.symbol_of(parent)
        {
            let declared = self.checker.get_declared_type_of_symbol(symbol);
            return self.verdict(
                declared,
                "declared type of a type symbol is `any`".to_string(),
                id,
            );
        }

        // 2. The right side of a property access.
        if let Some(parent) = self.nodes.parent(id)
            && self.nodes.kind(parent) == SyntaxKind::PropertyAccessExpression
            && self.map.get(parent).and_then(|node| node.name_id()) == Some(id)
            && let Some(Node::PropertyAccessExpression(access)) = self.map.get(parent)
        {
            let ours = self.checker.check_property_access_expression(access);
            let receiver = match access.expression {
                Some(expression) => self.checker.check_expression(expression),
                None => self.checker.intrinsics().error,
            };
            // §780: the producer's SS183 fast path (`types_producer.rs:510`)
            // — a name whose parent IS a property access prints `any` when the
            // ACCESS ITSELF computes to `error`, not only when it computes to
            // `any`. This classifier tested `any` alone, so every such name
            // became a DISAGREEMENT: the producer printed `any` and the
            // classifier's own branch had answered `error`. Mirroring the
            // branch means mirroring BOTH of its exits.
            if self.is_error(ours) {
                return (
                    Bucket::Checker,
                    "member name of an access that ANSWERED ERROR: SS183 prints `any`                      (types_producer.rs:510)"
                        .to_string(),
                );
            }
            let reason = if self.is_any(receiver) {
                format!(
                    "member name of an access on an `any` receiver (members.rs:99, checker.go:11314) <- {}",
                    self.receiver_origin(parent)
                )
            } else {
                "member name: the property's own type is `any`".to_string()
            };
            return self.verdict(ours, reason, id);
        }

        // 3. A declaration name.
        if let Some(parent) = self.nodes.parent(id)
            && self.map.get(parent).and_then(|node| node.name_id()) == Some(id)
            && let Some(symbol) = self.binder.symbol_of(parent)
        {
            let ours = self.checker.get_type_of_symbol(symbol);
            let arm = self.symbol_arm(symbol);
            return self.verdict(ours, format!("declaration name -> {arm}"), id);
        }

        // 4. The base of an `extends` clause — falls through unless the base's
        //    declared type is available, exactly as the producer does.
        if let Some(parent) = self.nodes.parent(id)
            && self.nodes.kind(parent) == SyntaxKind::ExpressionWithTypeArguments
            && let Some(clause) = self.nodes.parent(parent)
            && let Some(Node::HeritageClause(heritage)) = self.map.get(clause)
            && heritage.token.kind == SyntaxKind::ExtendsKeyword
            && matches!(
                self.nodes.parent(clause).map(|owner| self.nodes.kind(owner)),
                Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression)
            )
            && self.nodes.kind(id) == SyntaxKind::Identifier
            && let Some(Node::Identifier(name)) = self.map.get(id)
            && let Some(symbol) =
                self.binder.resolve_name(self.nodes, self.map, id, name.text, SymbolFlags::TYPE)
        {
            let declared = self.checker.get_declared_type_of_symbol(symbol);
            if !self.is_error(declared) {
                return self.verdict(
                    declared,
                    "base class expression: declared type is `any`".to_string(),
                    id,
                );
            }
        }

        // 5. The left of a qualified name in type position.
        if let Some(parent) = self.nodes.parent(id)
            && self.nodes.kind(parent) == SyntaxKind::QualifiedName
            && let Some(Node::QualifiedName(qualified)) = self.map.get(parent)
            && qualified.right.and_then(|right| right.node_id) != Some(id)
        {
            let mut outermost = parent;
            while let Some(above) = self.nodes.parent(outermost) {
                if self.nodes.kind(above) != SyntaxKind::QualifiedName {
                    break;
                }
                outermost = above;
            }
            let enclosing = self.nodes.parent(outermost).map(|above| self.nodes.kind(above));

            if enclosing == Some(SyntaxKind::ImportEqualsDeclaration)
                && let Some(Node::Identifier(name)) = self.map.get(id)
                && let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.map,
                    id,
                    name.text,
                    SymbolFlags::NAMESPACE,
                )
            {
                let declared = self.checker.get_declared_type_of_symbol(symbol);
                if !self.is_error(declared) {
                    return self.verdict(
                        declared,
                        "import-equals entity name: declared type is `any`".to_string(),
                        id,
                    );
                }
                let value = self.checker.get_type_of_symbol(symbol);
                return self.verdict(
                    value,
                    "import-equals entity name: value type is `any`".to_string(),
                    id,
                );
            }

            if enclosing != Some(SyntaxKind::TypeQuery) {
                return (
                    Bucket::Writer,
                    "qualified-name left in type position (type_symbol_baseline.go:380)"
                        .to_string(),
                );
            }
        }

        // 6. The property name of a binding element.
        if let Some(parent) = self.nodes.parent(id)
            && let Some(Node::BindingElement(element)) = self.map.get(parent)
            && element.property_name.and_then(|name| name.node_id()) == Some(id)
        {
            let name = match tsr_ast::Expression::try_from(node) {
                Ok(expression) => self.checker.check_expression(expression),
                Err(_) => self.checker.intrinsics().error,
            };
            if self.is_error(name) {
                return (
                    Bucket::Writer,
                    "binding element: the property name (type_symbol_baseline.go:380)".to_string(),
                );
            }
        }

        // 7. A label name.
        if is_label_name(id, self.nodes, self.map) {
            let label = match tsr_ast::Expression::try_from(node) {
                Ok(expression) => self.checker.check_expression(expression),
                Err(_) => self.checker.intrinsics().error,
            };
            if self.is_error(label) {
                return (Bucket::Writer, "label name (type_symbol_baseline.go:380)".to_string());
            }
        }

        // 8. An intrinsic JSX tag name.
        if self.nodes.kind(id) == SyntaxKind::Identifier
            && let Some(parent) = self.nodes.parent(id)
            && jsx_tag_name_of(parent, self.map) == Some(id)
            && let Some(Node::Identifier(name)) = self.map.get(id)
            && is_intrinsic_jsx_name(name.text)
        {
            let tag = match tsr_ast::Expression::try_from(node) {
                Ok(expression) => self.checker.check_expression(expression),
                Err(_) => self.checker.intrinsics().error,
            };
            if self.is_error(tag) {
                return (
                    Bucket::Writer,
                    "intrinsic JSX tag name (type_symbol_baseline.go:481)".to_string(),
                );
            }
            if self.is_any(tag) {
                let arm =
                    self.expression_arm(id, tsr_ast::Expression::try_from(node).expect("checked"));
                return (Bucket::Checker, arm);
            }
        }

        // 9. The expression fall-through.
        if let Ok(expression) = tsr_ast::Expression::try_from(node) {
            let ours = self.checker.check_expression(expression);
            let arm = self.expression_arm(id, expression);
            return self.verdict(ours, arm, id);
        }

        (
            Bucket::Unclassified,
            format!("neither a declaration name nor an expression: {:?}", self.nodes.kind(id)),
        )
    }

    /// A branch this classifier took has a type; it must be `any`, or the
    /// classifier and the producer have diverged.
    fn verdict(
        &mut self,
        ours: tsr_checker::TypeId,
        reason: String,
        at: NodeId,
    ) -> (Bucket, String) {
        if self.is_any(ours) {
            return (Bucket::Checker, reason);
        }
        // §780: mirror the PRODUCER'S PRINTER, not a context-free one.
        // `type_at_location` renders through `type_to_string_at(id, reference)`
        // (`types_producer.rs:1355`), which is reference-aware: a type can
        // print `any` at one position and something else out of context. This
        // classifier used `type_to_string`, so every such line became a
        // DISAGREEMENT — the producer printed `any` and the classifier's own
        // branch, printed differently, disagreed with it.
        //
        // This is mirroring, not tautology: the branch's TYPE is still the
        // classifier's own, and a branch that reaches a genuinely different
        // type still disagrees. Only the rendering is made common.
        let printed = self
            .checker
            .type_to_string_at(ours, at)
            .unwrap_or_else(|| self.checker.type_to_string(ours));
        // §780: the producer converts `error` to `any` on several branches
        // (`types_producer.rs:434`, `:467`, `:617`, `:630`) — "upstream holds
        // `errorType` at" those positions and the baseline records `any`. A
        // classifier branch that answered `error` therefore EXPLAINS the
        // printed `any` rather than contradicting it, and calling that a
        // DISAGREEMENT buried a real origin under the control.
        //
        // Still not a tautology: a branch reaching a genuine non-any,
        // non-error type continues to disagree, which is the drift the control
        // exists to catch.
        if self.is_error(ours) {
            return (
                Bucket::Checker,
                format!("{reason} <- the branch answered ERROR; the producer prints `any` there"),
            );
        }
        if printed == "any" {
            // A type whose printed *name* is `any` — `class any {}` — is not
            // the `any` type and must not be counted as one in either
            // direction.
            return (Bucket::Named, "a declaration named `any` (`class any {}`)".to_string());
        }
        (Bucket::Unclassified, format!("DISAGREEMENT: {reason}"))
    }
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let dump = std::env::var("TSR_ANY_DUMP").is_ok_and(|v| !v.is_empty());

    let reports: Vec<CaseReport> = cases
        .par_iter()
        .filter_map(|case| {
            // The suite's own skips, so every share here is a share of the
            // gradient's denominator and not of some other set.
            if case.has_varied_types() || case.has_known_divergence() {
                return None;
            }
            let text = case.expected_types()?;
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return None;
            }
            let parsed = case.load().ok()?;
            let arena = tsr_core::Arena::new();
            let (program, ours, ids) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
            let nodes = program.nodes();
            let node_map = program.node_map();
            let bound = program.binder();
            let mut checker = tsr_checker::Checker::new(bound, nodes, node_map);
            let mut ctx = Ctx { checker: &mut checker, binder: bound, nodes, map: node_map };

            let mut report = CaseReport::default();
            for (index, expected_file) in expected.iter().enumerate() {
                let our_file = ours.get(index);
                let our_ids = ids.get(index);
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    report.expected += 1;
                    let Some(got) = our_file.and_then(|file| file.get(position)) else { continue };
                    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    report.aligned += 1;
                    if want_type == got.type_string {
                        report.matched += 1;
                    }
                    if got.type_string != "any" {
                        continue;
                    }
                    let Some(id) = our_ids.and_then(|line_ids| line_ids.get(position).copied())
                    else {
                        continue;
                    };
                    let (bucket, reason) = ctx.classify(id);
                    if reason.starts_with("DISAGREEMENT") {
                        report.disagreement += 1;
                        if std::env::var("TSR_ANY_DEBUG").is_ok_and(|v| !v.is_empty()) {
                            report.debug.push(format!(
                                "{}:{index}:{position}\t{:?}\tparent={:?}\twant={want_type}\t{reason}",
                                case.name,
                                ctx.nodes.kind(id),
                                ctx.nodes
                                    .parent(id)
                                    .map(|p| ctx.nodes.kind(p))
                            ));
                        }
                    }
                    let rows =
                        if want_type == "any" { &mut report.banked } else { &mut report.lost };
                    rows.entry((bucket.label(), reason)).or_default().add(&case.name);
                    if want_type != "any" {
                        *report.lost_wants.entry(want_type.to_string()).or_default() += 1;
                        if dump {
                            let (bucket, reason) = ctx.classify(id);
                            report.dump.push(format!(
                                "{}:{index}:{position}\t{}\t{reason}\t{want_type}",
                                case.name,
                                bucket.label(),
                            ));
                        }
                    }
                }
            }
            Some(report)
        })
        .collect();

    if dump {
        let path = repo_root().join("target/any_lost_lines.tsv");
        let mut out: Vec<&str> =
            reports.iter().flat_map(|r| r.dump.iter().map(String::as_str)).collect();
        out.sort_unstable();
        std::fs::write(&path, out.join("\n")).expect("write any dump");
        eprintln!("any_audit: wrote {} LOST lines to {}", out.len(), path.display());
    }

    let aligned: usize = reports.iter().map(|r| r.aligned).sum();
    let matched: usize = reports.iter().map(|r| r.matched).sum();
    let expected: usize = reports.iter().map(|r| r.expected).sum();
    let disagreement: usize = reports.iter().map(|r| r.disagreement).sum();

    let mut banked: HashMap<(&'static str, String), Row> = HashMap::new();
    let mut lost: HashMap<(&'static str, String), Row> = HashMap::new();
    let mut lost_wants: HashMap<String, usize> = HashMap::new();
    for report in &reports {
        for (key, row) in &report.banked {
            banked.entry(key.clone()).or_default().merge(row);
        }
        for (key, row) in &report.lost {
            lost.entry(key.clone()).or_default().merge(row);
        }
        for (want, count) in &report.lost_wants {
            *lost_wants.entry(want.clone()).or_default() += count;
        }
    }

    let banked_total: usize = banked.values().map(|row| row.lines).sum();
    let lost_total: usize = lost.values().map(|row| row.lines).sum();

    println!("cases: {}", reports.len());
    println!("upstream lines: {expected}");
    println!("aligned lines:  {aligned}");
    println!("exactly right:  {matched}  ({:.2}% of aligned)", pct(matched, aligned));
    println!(
        "\nRECONCILIATION against `cargo run -p tsr-conformance --bin coverage`: the gradient's\n\
         denominator is every upstream line ({expected} here); its numerator is every exactly\n\
         matched line ({matched} here). Both are re-derived above from the same producer entry\n\
         point the suite scores through, so a difference in either is a defect in this probe."
    );

    println!("\nBANKED — we print `any` and upstream printed `any`: {banked_total}");
    print_rows(&banked, banked_total);
    println!("\nLOST — we print `any` and upstream printed something else: {lost_total}");
    print_rows(&lost, lost_total);

    println!("\nwhat upstream printed on the lost lines, commonest first:");
    let mut wants: Vec<_> = lost_wants.into_iter().map(|(want, count)| (count, want)).collect();
    wants.sort_unstable_by(|a, b| b.cmp(a));
    for (count, want) in wants.iter().take(12) {
        println!("  {count:>7}  {}", truncate(want, 60));
    }

    let unclassified_banked: usize = banked
        .iter()
        .filter(|((bucket, _), _)| *bucket == Bucket::Unclassified.label())
        .map(|(_, row)| row.lines)
        .sum();
    let unclassified_lost: usize = lost
        .iter()
        .filter(|((bucket, _), _)| *bucket == Bucket::Unclassified.label())
        .map(|(_, row)| row.lines)
        .sum();
    for (label, rows) in [("banked", &banked), ("lost", &lost)] {
        for ((bucket, reason), row) in rows {
            if *bucket == Bucket::Unclassified.label() {
                let mut cases: Vec<_> = row.by_case.iter().collect();
                cases.sort_unstable_by_key(|(name, count)| {
                    (std::cmp::Reverse(**count), (*name).clone())
                });
                println!(
                    "\n  control detail ({label}): {reason} — {} lines, first cases {:?}",
                    row.lines,
                    cases.iter().take(3).collect::<Vec<_>>()
                );
            }
        }
    }
    // The exposure, stated as the question it answers: of the banked lines,
    // how many were minted by a rule whose *precondition* this port may have
    // reached for a reason upstream would not have. Only the contextualisable
    // parameter row is in that position — see the module doc.
    let exposure: usize = banked
        .iter()
        .filter(|((_, reason), _)| reason.contains("CONTEXTUALISABLE"))
        .map(|(_, row)| row.lines)
        .sum();
    let writer: usize = banked
        .iter()
        .filter(|((bucket, _), _)| *bucket == Bucket::Writer.label())
        .map(|(_, row)| row.lines)
        .sum();
    println!(
        "\nEXPOSURE — the share of the banked {banked_total} that is not a type this checker\n\
         computed from an upstream-anchored rule on inputs upstream would have had:\n\
         \x20 writer rules (neither compiler computed a type)          {writer:>6}  ({:.1}%)\n\
         \x20 unannotated parameter in a contextualisable container    {exposure:>6}  ({:.1}%)",
        pct(writer, banked_total),
        pct(exposure, banked_total)
    );

    if std::env::var("TSR_ANY_DEBUG").is_ok_and(|v| !v.is_empty()) {
        let mut kinds: HashMap<String, usize> = HashMap::new();
        for report in &reports {
            for line in &report.debug {
                let key = line.split('\t').skip(1).take(2).collect::<Vec<_>>().join(" ");
                *kinds.entry(key).or_default() += 1;
            }
        }
        let mut rows: Vec<(String, usize)> = kinds.into_iter().collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.1));
        println!("\nRAW disagreements (first 10):");
        for line in reports.iter().flat_map(|report| report.debug.iter()).take(10) {
            println!("  {line}");
        }
        println!("\nDISAGREEMENT by node kind x parent kind (TSR_ANY_DEBUG):");
        for (key, count) in rows.iter().take(15) {
            println!("  {count:7}  {key}");
        }
    }
    println!("\nCONTROLS (must read zero):");
    println!("  UNCLASSIFIED, banked      {unclassified_banked}");
    println!("  UNCLASSIFIED, lost        {unclassified_lost}");
    println!("  DISAGREEMENT (both)       {disagreement}");
    // Asserted, not merely printed: a classifier that has drifted from
    // `type_at_location` produces a tidy table either way, and the only thing
    // standing between that and a published finding is this line.
    assert_eq!(
        unclassified_banked + unclassified_lost + disagreement,
        0,
        "the classifier no longer mirrors `types_producer::type_at_location`; no row above is trustworthy"
    );
}

fn print_rows(rows: &HashMap<(&'static str, String), Row>, total: usize) {
    let mut by_bucket: HashMap<&'static str, usize> = HashMap::new();
    for ((bucket, _), row) in rows {
        *by_bucket.entry(bucket).or_default() += row.lines;
    }
    let mut buckets: Vec<_> = by_bucket.into_iter().collect();
    buckets.sort_unstable_by_key(|(_, lines)| std::cmp::Reverse(*lines));
    for (bucket, lines) in buckets {
        println!("  {bucket:<14} {lines:>7}  ({:.1}%)", pct(lines, total));
    }
    println!("  {:<150} {:>7}  {:>5} {:>7} {:>7}", "row", "lines", "cases", "top-1", "top-10");
    let mut ranked: Vec<_> = rows.iter().collect();
    ranked.sort_unstable_by_key(|(_, row)| std::cmp::Reverse(row.lines));
    for ((bucket, reason), row) in ranked.iter().take(28) {
        let (cases, top1, top10) = row.concentration();
        println!(
            "  {:<150} {:>7}  {cases:>5} {top1:>6.1}% {top10:>6.1}%",
            truncate(&format!("{bucket}/{reason}"), 148),
            row.lines
        );
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars().take(width - 1).collect::<String>() + "…"
}
