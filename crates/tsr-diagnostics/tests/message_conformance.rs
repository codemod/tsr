//! Diagnostic conformance against typescript-go.
//!
//! As with the AST (see `docs/adr/0006-conformance-oracle.md`), the oracle is the
//! **generated Go**, not the JSON we generate from. Testing against our own input
//! would only prove the generator is self-consistent.
//!
//! Our catalogue is a deliberate strict **superset** of upstream's: upstream keys
//! its intermediate map by diagnostic code, which silently drops one message from
//! each of the 8 code-sharing pairs — and, because Go map iteration is randomised,
//! *which* one it drops varies between regenerations. We keep both. This test
//! asserts the superset relation exactly, so the divergence stays quantified
//! rather than assumed.

use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

use tsr_diagnostics::{Category, messages};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsr-diagnostics is two levels below the repo root")
        .to_path_buf()
}

static GO_SOURCE: LazyLock<Option<String>> = LazyLock::new(|| {
    let path =
        repo_root().join("vendor/typescript-go/internal/diagnostics/diagnostics_generated.go");
    std::fs::read_to_string(path).ok()
});

/// One message as declared in upstream's generated Go.
#[derive(Debug, PartialEq, Eq)]
struct GoMessage {
    code: u32,
    category: String,
    key: String,
    text: String,
}

/// Parse `var Name = &Message{code: 1002, category: CategoryError, key: "...", text: "..."}`.
fn parse_go_messages(source: &str) -> Vec<GoMessage> {
    let mut out = Vec::new();
    for line in source.lines() {
        let Some(rest) = line.strip_prefix("var ") else { continue };
        let Some((_, body)) = rest.split_once("&Message{") else { continue };

        let Some(code) = field(body, "code: ").and_then(|v| v.trim_end_matches(',').parse().ok())
        else {
            continue;
        };
        let Some(category) =
            field(body, "category: Category").map(|v| v.trim_end_matches(',').to_string())
        else {
            continue;
        };
        let Some(key) = go_string(body, "key: ") else { continue };
        let Some(text) = go_string(body, "text: ") else { continue };

        out.push(GoMessage { code, category, key, text });
    }
    out
}

/// Read an unquoted field value up to the next comma.
fn field<'a>(body: &'a str, prefix: &str) -> Option<&'a str> {
    let start = body.find(prefix)? + prefix.len();
    let rest = &body[start..];
    let end = rest.find(',').unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Read a Go double-quoted string field, honouring backslash escapes.
fn go_string(body: &str, prefix: &str) -> Option<String> {
    let start = body.find(prefix)? + prefix.len();
    let rest = &body[start..];
    let rest = rest.strip_prefix('"')?;

    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                // Anything else (\u, \x) is not present in this file; fail loudly
                // rather than silently mangling text we then compare against.
                other => return Some(format!("<unsupported escape \\{other}>")),
            },
            c => out.push(c),
        }
    }
    None
}

#[test]
fn every_upstream_message_exists_here_with_matching_fields() {
    let Some(source) = GO_SOURCE.as_ref() else {
        eprintln!("skipping: vendor/typescript-go not checked out");
        return;
    };
    let go = parse_go_messages(source);
    assert!(
        go.len() > 2_000,
        "parsed only {} messages from diagnostics_generated.go; the parser is probably broken",
        go.len()
    );

    let ours: HashMap<&str, &tsr_diagnostics::Message> =
        messages::ALL.iter().map(|m| (m.key(), *m)).collect();

    let mut problems = Vec::new();
    for expected in &go {
        let Some(actual) = ours.get(expected.key.as_str()) else {
            problems.push(format!("{}: missing (code {})", expected.key, expected.code));
            continue;
        };
        if actual.code() != expected.code {
            problems.push(format!(
                "{}: code {} here, {} upstream",
                expected.key,
                actual.code(),
                expected.code
            ));
        }
        if actual.text() != expected.text {
            problems.push(format!(
                "{}: text {:?} here, {:?} upstream",
                expected.key,
                actual.text(),
                expected.text
            ));
        }
        let expected_category = match expected.category.as_str() {
            "Error" => Category::Error,
            "Warning" => Category::Warning,
            "Suggestion" => Category::Suggestion,
            "Message" => Category::Message,
            other => {
                problems.push(format!("{}: unknown upstream category {other}", expected.key));
                continue;
            }
        };
        if actual.category() != expected_category {
            problems.push(format!(
                "{}: category {:?} here, {:?} upstream",
                expected.key,
                actual.category(),
                expected_category
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "{} message(s) disagree with typescript-go:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

#[test]
fn we_are_a_superset_and_the_extra_messages_are_the_expected_ones() {
    let Some(source) = GO_SOURCE.as_ref() else {
        eprintln!("skipping: vendor/typescript-go not checked out");
        return;
    };
    let go = parse_go_messages(source);
    let go_keys: std::collections::HashSet<&str> = go.iter().map(|m| m.key.as_str()).collect();

    let extra: Vec<&str> =
        messages::ALL.iter().map(|m| m.key()).filter(|k| !go_keys.contains(k)).collect();

    // Exactly the 8 messages upstream drops by keying its map on code. Any other
    // extra means our merge of diagnosticMessages.json + extraDiagnosticMessages
    // has drifted.
    assert_eq!(
        extra.len(),
        8,
        "expected exactly 8 extra messages (upstream's code-collision casualties), got {}: {extra:?}",
        extra.len()
    );

    // Each extra must share its code with a message upstream *did* keep.
    for key in &extra {
        let ours = tsr_diagnostics::by_key(key).expect("extra key resolves");
        let shares = go.iter().any(|m| m.code == ours.code());
        assert!(shares, "extra message {key} does not share a code with any upstream message");
    }
}

#[test]
fn placeholder_arity_is_consistent_with_the_text() {
    // A message whose text references {2} but never {0} or {1} would be a data
    // problem worth surfacing; check the placeholder set is contiguous from zero.
    let mut problems = Vec::new();
    for m in messages::ALL {
        let mut indices: Vec<usize> = Vec::new();
        let mut rest = m.text();
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            if let Some(close) = after.find('}') {
                if let Ok(i) = after[..close].parse::<usize>() {
                    indices.push(i);
                }
                rest = &after[close + 1..];
            } else {
                break;
            }
        }
        if indices.is_empty() {
            continue;
        }
        indices.sort_unstable();
        indices.dedup();
        let expected: Vec<usize> = (0..indices.len()).collect();
        if indices != expected {
            problems.push(format!("{}: placeholders {indices:?}", m.key()));
        }
    }
    assert!(
        problems.is_empty(),
        "{} message(s) have non-contiguous placeholders:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}
