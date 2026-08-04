//! Asserts the generated lib tables against **upstream's generated output**.
//!
//! [ADR-0006](../../../docs/adr/0006-conformance-oracle.md) rule: never assert
//! against the input we share with upstream. `LIB_NAMES` is generated from the
//! `internal/bundled/libs/` directory, so checking it against that directory would
//! only prove the generator can read a directory twice. It is checked here against
//! `internal/bundled/libs_generated.go` — what upstream's *own* generator decided
//! that directory means — so a disagreement is a real signal.
//!
//! `LIB_MAP` has no generated counterpart upstream, so it is checked for the
//! properties that make it load-bearing instead: every file it names is shipped,
//! and its order is preserved.

use std::path::PathBuf;

use tsr_tsoptions::{LIB_MAP, LIB_NAMES};

fn upstream_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsr-tsoptions is two levels below the repo root")
        .join("vendor/typescript-go")
}

/// The string literals of upstream's `LibNames`, in file order.
fn upstream_lib_names() -> Option<Vec<String>> {
    let path = upstream_root().join("internal/bundled/libs_generated.go");
    let source = std::fs::read_to_string(path).ok()?;
    Some(
        source
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let inner = line.strip_prefix('"')?.strip_suffix("\",")?;
                inner.starts_with("lib.").then(|| inner.to_string())
            })
            .collect(),
    )
}

#[test]
fn lib_names_match_upstreams_generated_list() {
    let Some(upstream) = upstream_lib_names() else {
        // The submodule is not initialized. Skipping is right here — the check is
        // about agreement with upstream, and with no upstream there is nothing to
        // agree with. CI initializes it, so the assertion still runs where it counts.
        return;
    };
    assert!(!upstream.is_empty(), "parsed libs_generated.go but found no entries");
    assert_eq!(LIB_NAMES.as_slice(), upstream.as_slice());
}

#[test]
fn every_lib_option_names_a_shipped_file() {
    for (option, file) in LIB_MAP {
        assert!(
            LIB_NAMES.contains(&file),
            "--lib {option} selects {file}, which is not a bundled lib file"
        );
    }
}

#[test]
fn lib_option_values_are_unique() {
    let mut seen: Vec<&str> = LIB_MAP.iter().map(|(option, _)| *option).collect();
    let count = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), count, "a --lib value is listed twice");
}

#[test]
fn the_load_order_starts_where_upstream_starts() {
    // Order is load order, and load order decides which declaration of a merged
    // global type wins. `es5` first is upstream's, and a generator that sorted the
    // map would silently change semantics while looking tidier.
    assert_eq!(LIB_MAP[0], ("es5", "lib.es5.d.ts"));
    assert!(LIB_MAP.iter().any(|(option, _)| *option == "dom"), "the DOM lib should be selectable");
}
