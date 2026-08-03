//! The derived keyword table matches upstream's hand-written one.
//!
//! `keyword_kind` derives the table from `SyntaxKind` rather than duplicating
//! upstream's map: every keyword kind is named `<Word>Keyword` and its source text
//! is the lowercase form. That is a *claim*, and it is exactly the sort of claim
//! that holds for 80 entries and fails for one — so it is checked against
//! `internal/scanner/scanner.go` rather than assumed.

use std::{collections::BTreeMap, path::PathBuf};

use tsr_ast::SyntaxKind;
use tsr_scanner::keyword_kind;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsr-scanner is two levels below the repo root")
        .to_path_buf()
}

/// Parse `"abstract":    ast.KindAbstractKeyword,` out of the `textToKeyword` map.
fn upstream_keywords(source: &str) -> BTreeMap<String, String> {
    let Some(body) = source.split_once("var textToKeyword = map[string]ast.Kind{") else {
        return BTreeMap::new();
    };
    let body = body.1.split_once("\n}").map_or(body.1, |(head, _)| head);

    let mut out = BTreeMap::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('"') else { continue };
        let Some((word, rest)) = rest.split_once('"') else { continue };
        let Some((_, kind)) = rest.split_once("ast.Kind") else { continue };
        let kind = kind.trim().trim_end_matches(',').trim();
        out.insert(word.to_string(), kind.to_string());
    }
    out
}

#[test]
fn derived_keyword_table_matches_upstream_exactly() {
    let path = repo_root().join("vendor/typescript-go/internal/scanner/scanner.go");
    let Ok(source) = std::fs::read_to_string(&path) else {
        eprintln!("skipping: vendor/typescript-go not checked out");
        return;
    };

    let upstream = upstream_keywords(&source);
    assert!(
        upstream.len() > 60,
        "parsed only {} keywords from scanner.go; the parser is probably broken",
        upstream.len()
    );

    let mut problems = Vec::new();

    // Every upstream keyword must resolve here, to the same kind.
    for (word, expected_kind) in &upstream {
        match keyword_kind(word) {
            Some(kind) if kind.name() == expected_kind => {}
            Some(kind) => problems.push(format!(
                "{word:?}: resolves to {} here, {expected_kind} upstream",
                kind.name()
            )),
            None => problems.push(format!("{word:?}: not recognised here ({expected_kind})")),
        }
    }

    // And nothing extra: every keyword kind in the enum's keyword range must
    // appear in upstream's table, or our derivation is inventing keywords.
    let first = SyntaxKind::FIRST_KEYWORD as u16;
    let last = SyntaxKind::LAST_KEYWORD as u16;
    let upstream_kinds: std::collections::BTreeSet<&str> =
        upstream.values().map(String::as_str).collect();
    for kind in (first..=last).filter_map(SyntaxKind::from_u16) {
        if !upstream_kinds.contains(kind.name()) {
            problems
                .push(format!("{}: in our keyword range but not upstream's table", kind.name()));
        }
    }

    assert!(
        problems.is_empty(),
        "{} keyword disagreement(s) with typescript-go:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

#[test]
fn non_keywords_are_rejected() {
    assert_eq!(keyword_kind("notakeyword"), None);
    assert_eq!(keyword_kind(""), None);
    assert_eq!(keyword_kind("If"), None, "keywords are case-sensitive");
    assert_eq!(keyword_kind("IF"), None);
    assert_eq!(keyword_kind("$"), None);
}

#[test]
fn representative_keywords_resolve() {
    assert_eq!(keyword_kind("if"), Some(SyntaxKind::IfKeyword));
    assert_eq!(keyword_kind("instanceof"), Some(SyntaxKind::InstanceOfKeyword));
    assert_eq!(keyword_kind("keyof"), Some(SyntaxKind::KeyOfKeyword));
    assert_eq!(keyword_kind("typeof"), Some(SyntaxKind::TypeOfKeyword));
    assert_eq!(keyword_kind("bigint"), Some(SyntaxKind::BigIntKeyword));
}
