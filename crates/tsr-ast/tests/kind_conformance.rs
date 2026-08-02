//! AST conformance against typescript-go.
//!
//! The generated AST is derived from `_scripts/ast.json`, but the artifact the Go
//! compiler actually *uses* is `internal/ast/kind_generated.go`. Those two can
//! drift — while writing this port they did, by two kinds — so conformance is
//! asserted against the Go source itself rather than against the JSON we
//! generated from.
//!
//! These tests read the vendored submodule. When it is absent they skip rather
//! than fail, so a checkout without submodules can still run the unit suite.

use std::{path::PathBuf, sync::LazyLock};

use tsr_ast::SyntaxKind;

/// Repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsr-ast is two levels below the repo root")
        .to_path_buf()
}

/// The vendored `kind_generated.go`, or `None` if the submodule is not checked out.
static GO_KIND_SOURCE: LazyLock<Option<String>> = LazyLock::new(|| {
    let path = repo_root().join("vendor/typescript-go/internal/ast/kind_generated.go");
    std::fs::read_to_string(path).ok()
});

/// Parse the `Kind` enum out of `kind_generated.go`, in declaration order.
///
/// The enum body runs from `KindUnknown Kind = iota` until the first aliased
/// entry (`KindFirstAssignment = KindEqualsToken`) or `KindCount`.
fn go_kind_names(source: &str) -> Vec<String> {
    let body = source.split_once("const (").expect("kind_generated.go has a const block").1;

    let mut names = Vec::new();
    let mut started = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("Kind") else { continue };

        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            continue;
        }
        let tail = &rest[name.len()..];

        if !started {
            // The enum opens with `KindUnknown Kind = iota`.
            if name == "Unknown" && tail.contains("iota") {
                names.push(name);
                started = true;
            }
            continue;
        }

        // `KindCount` terminates the list; an `=` marks the start of the alias
        // block that follows it.
        if name == "Count" || tail.contains('=') {
            break;
        }
        names.push(name);
    }
    names
}

#[test]
fn every_go_kind_exists_here_in_the_same_order() {
    let Some(source) = GO_KIND_SOURCE.as_ref() else {
        eprintln!("skipping: vendor/typescript-go not checked out");
        return;
    };
    let go = go_kind_names(source);
    assert!(!go.is_empty(), "failed to parse any kinds out of kind_generated.go");

    let ours: Vec<&str> = SyntaxKind::ALL.iter().map(|k| k.name()).collect();

    // Report the full disagreement rather than just the first, so a regeneration
    // gap is visible in one run.
    let missing: Vec<&String> = go.iter().filter(|k| !ours.contains(&k.as_str())).collect();
    let extra: Vec<&&str> = ours.iter().filter(|k| !go.contains(&(*k).to_string())).collect();
    assert!(missing.is_empty(), "kinds in typescript-go but missing here: {missing:?}");
    assert!(extra.is_empty(), "kinds here but not in typescript-go: {extra:?}");

    assert_eq!(
        go.len(),
        ours.len(),
        "kind count differs: typescript-go has {}, we have {}",
        go.len(),
        ours.len()
    );

    // Order is semantic: several predicates are range checks over discriminants.
    // Compare the *discriminant value* against Go's ordinal position, not merely
    // the order of `ALL` — those two can drift apart independently, and only the
    // discriminant is what range predicates actually test.
    for (ordinal, expected) in go.iter().enumerate() {
        let kind = SyntaxKind::ALL
            .iter()
            .find(|k| k.name() == expected)
            .unwrap_or_else(|| panic!("kind {expected} exists in typescript-go but not here"));
        assert_eq!(
            *kind as usize, ordinal,
            "kind {expected} has discriminant {} here but ordinal {ordinal} in typescript-go",
            *kind as usize
        );
    }
}

#[test]
fn marker_constants_agree_with_go() {
    let Some(source) = GO_KIND_SOURCE.as_ref() else {
        eprintln!("skipping: vendor/typescript-go not checked out");
        return;
    };

    // Markers chain upstream (`LastToken = LastKeyword = DeferKeyword`), so
    // resolve through the alias block before comparing.
    let mut aliases: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some((lhs, rhs)) = trimmed.split_once('=') else { continue };
        let lhs = lhs.trim();
        let rhs = rhs.split("//").next().unwrap_or(rhs).trim();
        if let (Some(l), Some(r)) = (lhs.strip_prefix("Kind"), rhs.strip_prefix("Kind")) {
            if !l.is_empty() && !r.is_empty() && !r.contains(' ') {
                aliases.push((l.to_string(), r.to_string()));
            }
        }
    }
    assert!(!aliases.is_empty(), "failed to parse marker aliases");

    let resolve = |name: &str| -> String {
        let mut current = name.to_string();
        for _ in 0..=aliases.len() {
            match aliases.iter().find(|(l, _)| *l == current) {
                Some((_, r)) => current = r.clone(),
                None => break,
            }
        }
        current
    };

    // Spot-check the markers the checker actually relies on as range bounds.
    let checks: &[(&str, SyntaxKind)] = &[
        ("FirstAssignment", SyntaxKind::FIRST_ASSIGNMENT),
        ("LastAssignment", SyntaxKind::LAST_ASSIGNMENT),
        ("FirstKeyword", SyntaxKind::FIRST_KEYWORD),
        ("LastKeyword", SyntaxKind::LAST_KEYWORD),
        ("LastToken", SyntaxKind::LAST_TOKEN),
    ];
    for (go_name, ours) in checks {
        let expected = resolve(go_name);
        assert_eq!(
            expected,
            ours.name(),
            "marker {go_name}: typescript-go resolves to {expected}, we have {}",
            ours.name()
        );
    }
}

#[test]
fn generated_manifest_matches_the_compiled_enum() {
    // Guards against a stale checked-in `generated/` directory: the manifest is
    // written by the generator, the enum is what actually compiled.
    let manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/generated/manifest.json");
    let raw = std::fs::read_to_string(manifest_path).expect("generated manifest is checked in");
    let manifest: serde_json::Value = serde_json::from_str(&raw).expect("manifest is valid JSON");

    let count = manifest["kind_count"].as_u64().expect("kind_count is a number");
    assert_eq!(count, u64::from(SyntaxKind::COUNT), "manifest is stale; run `cargo xtask codegen`");

    let kinds = manifest["kinds"].as_array().expect("kinds is an array");
    for (i, expected) in kinds.iter().enumerate() {
        let expected = expected.as_str().expect("kind name is a string");
        assert_eq!(expected, SyntaxKind::ALL[i].name(), "manifest diverges at index {i}");
    }
}
