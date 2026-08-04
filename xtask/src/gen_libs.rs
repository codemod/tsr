//! Generates `crates/tsr-tsoptions/src/generated/libs.rs`.
//!
//! Two tables that together describe TypeScript's bundled `lib.*.d.ts` files:
//!
//! - **`LIB_NAMES`** — every file shipped in `internal/bundled/libs/`, sorted.
//!   Upstream generates the same list into `bundled/libs_generated.go`, from the
//!   same directory.
//! - **`LIB_MAP`** — the `--lib` option values (`es2015`, `dom`, …) and the file
//!   each selects, **in upstream's declaration order**, which is also the order
//!   the compiler loads them in.
//!
//! # Two different sources of truth, and why the second one bends a rule
//!
//! `LIB_NAMES` is [ADR-0007](../../docs/adr/0007-generated-code-policy.md)
//! Category A without qualification: the input is a directory of `.d.ts` files
//! from the TypeScript submodule, exactly what upstream's own generator reads.
//!
//! `LIB_MAP` is not. Its source of truth is `internal/tsoptions/enummaps.go` —
//! hand-written Go, so ADR-0007 rule 1 ("read upstream's input, not upstream's
//! output") has nothing to point at. It is still generated here rather than
//! transcribed, because the rule's *purpose* is drift tracking, and this table has
//! every property that makes drift dangerous: it changes whenever TypeScript adds
//! a lib, the order is load-bearing, and a stale entry fails as a missing global
//! type thousands of lines away from the mistake. A hand-copied list of 90 pairs
//! would rot silently.
//!
//! This is a *data* table that happens to live in Go source, which is a different
//! thing from `stringer` output — Category B is "a mechanical restatement of a
//! declaration", and this carries information no Rust feature supplies. The
//! register in ADR-0007 records the distinction.
//!
//! # The cross-check
//!
//! Every file `LIB_MAP` names must exist in `LIB_NAMES`, and generation fails if
//! one does not (ADR-0007 rule 2: no permissive fallback). That single assertion
//! is what would catch a lib renamed upstream, a submodule left un-updated, or a
//! parse that quietly matched nothing.

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// One `--lib` option value and the file it selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibEntry {
    /// The value written on the command line or in `tsconfig.json`.
    pub option: String,
    /// The `lib.*.d.ts` file it resolves to.
    pub file: String,
}

/// The bundled lib file names, sorted.
///
/// Sorted rather than directory order so the output is stable across filesystems;
/// upstream sorts for the same reason.
pub fn read_lib_names(libs_dir: &Path) -> Result<Vec<String>> {
    if !libs_dir.is_dir() {
        bail!(
            "no bundled libs at {}\n\n\
             The vendored submodule may not be initialized; run:\n  \
             git submodule update --init --recursive",
            libs_dir.display()
        );
    }

    let mut names = BTreeSet::new();
    for entry in
        std::fs::read_dir(libs_dir).with_context(|| format!("reading {}", libs_dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("lib.") && name.ends_with(".d.ts") {
            names.insert(name);
        }
    }

    if names.is_empty() {
        bail!("{} contains no lib.*.d.ts files", libs_dir.display());
    }
    Ok(names.into_iter().collect())
}

/// Parse `LibMap` out of `internal/tsoptions/enummaps.go`, preserving order.
///
/// The shape is fixed and machine-written:
///
/// ```text
/// var LibMap = collections.NewOrderedMapFromList([]collections.MapEntry[string, any]{
///     // JavaScript only
///     {Key: "es5", Value: "lib.es5.d.ts"},
/// ```
///
/// Anything inside the literal that is neither a comment, a blank line, nor a
/// well-formed entry is an error rather than a skipped line — a permissive parser
/// here would silently drop libs and report a smaller table as success.
pub fn parse_lib_map(source: &str) -> Result<Vec<LibEntry>> {
    let start = source
        .find("var LibMap = ")
        .context("`var LibMap = ` not found in enummaps.go; upstream may have renamed it")?;
    let body = &source[start..];
    let open = body.find('{').context("no opening brace after `var LibMap`")?;

    let mut entries = Vec::new();
    for line in body[open + 1..].lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        // `})` closes the literal.
        if line.starts_with("})") {
            return finish(entries);
        }
        let entry = parse_entry(line)
            .with_context(|| format!("unrecognised line in the LibMap literal: {line:?}"))?;
        entries.push(entry);
    }
    bail!("the LibMap literal was never closed")
}

fn finish(entries: Vec<LibEntry>) -> Result<Vec<LibEntry>> {
    if entries.is_empty() {
        bail!("parsed LibMap but found no entries");
    }
    Ok(entries)
}

/// `{Key: "es5", Value: "lib.es5.d.ts"},` → one entry.
fn parse_entry(line: &str) -> Option<LibEntry> {
    let rest = line.strip_prefix('{')?;
    let (key, rest) = quoted_after(rest, "Key:")?;
    let (value, _) = quoted_after(rest, "Value:")?;
    Some(LibEntry { option: key, file: value })
}

/// The quoted string following `label`, plus what comes after it.
fn quoted_after<'a>(source: &'a str, label: &str) -> Option<(String, &'a str)> {
    let after_label = source.find(label)? + label.len();
    let rest = &source[after_label..];
    let open = rest.find('"')? + 1;
    let close = rest[open..].find('"')? + open;
    Some((rest[open..close].to_string(), &rest[close + 1..]))
}

/// Render the generated module.
pub fn generate(names: &[String], map: &[LibEntry]) -> Result<String> {
    // ADR-0007 rule 2: an entry naming a file we do not ship is a hard error, not
    // a warning. This is the check that catches a stale submodule.
    for entry in map {
        if !names.iter().any(|name| name == &entry.file) {
            bail!(
                "LibMap entry {:?} names {:?}, which is not among the {} bundled lib files",
                entry.option,
                entry.file,
                names.len()
            );
        }
    }

    let mut out = String::new();
    out.push_str(
        "//! TypeScript's bundled `lib.*.d.ts` files.\n\
         //!\n\
         //! @generated by `cargo xtask codegen`. Do not edit by hand.\n\
         //!\n\
         //! `LIB_NAMES` mirrors upstream's `bundled/libs_generated.go`; `LIB_MAP`\n\
         //! mirrors `tsoptions.LibMap` and is in load order. See\n\
         //! `xtask/src/gen_libs.rs` for why the two have different sources of truth.\n\n",
    );

    writeln!(out, "/// Every bundled lib file, sorted by name.")?;
    writeln!(out, "///")?;
    writeln!(out, "/// Upstream counterpart: `LibNames` in `internal/bundled/libs_generated.go`.")?;
    writeln!(out, "pub static LIB_NAMES: [&str; {}] = [", names.len())?;
    for name in names {
        writeln!(out, "    {name:?},")?;
    }
    out.push_str("];\n\n");

    writeln!(out, "/// `--lib` option values and the file each selects, in load order.")?;
    writeln!(out, "///")?;
    writeln!(out, "/// Upstream counterpart: `LibMap` in `internal/tsoptions/enummaps.go`.")?;
    writeln!(out, "/// Order is upstream's declaration order and is load-bearing.")?;
    writeln!(out, "pub static LIB_MAP: [(&str, &str); {}] = [", map.len())?;
    for entry in map {
        writeln!(out, "    ({:?}, {:?}),", entry.option, entry.file)?;
    }
    out.push_str("];\n");

    Ok(out)
}

/// Read both inputs and render, given the repository root.
pub fn run(root: &Path) -> Result<(usize, usize)> {
    let libs_dir: PathBuf = root.join("vendor/typescript-go/internal/bundled/libs");
    let enummaps = root.join("vendor/typescript-go/internal/tsoptions/enummaps.go");

    let names = read_lib_names(&libs_dir)?;
    let source = std::fs::read_to_string(&enummaps)
        .with_context(|| format!("reading {}", enummaps.display()))?;
    let map = parse_lib_map(&source)?;

    let rendered = generate(&names, &map)?;
    let out_dir = root.join("crates/tsr-tsoptions/src/generated");
    std::fs::create_dir_all(&out_dir).context("creating generated output directory")?;
    std::fs::write(out_dir.join("libs.rs"), rendered).context("writing libs.rs")?;
    std::fs::write(
        out_dir.join("mod.rs"),
        "//! Generated tables.\n\
         //!\n\
         //! @generated by `cargo xtask codegen`. Do not edit by hand.\n\n\
         pub mod libs;\n",
    )
    .context("writing mod.rs")?;

    Ok((names.len(), map.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lib_map_literal_parses_in_order() {
        let source = "var LibMap = collections.NewOrderedMapFromList([]collections.MapEntry[string, any]{\n\
             \t// JavaScript only\n\
             \t{Key: \"es5\", Value: \"lib.es5.d.ts\"},\n\
             \t{Key: \"es6\", Value: \"lib.es2015.d.ts\"},\n\
             })\n";
        let entries = parse_lib_map(source).expect("parses");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], LibEntry { option: "es5".into(), file: "lib.es5.d.ts".into() });
        // Order is load order, so it must survive the round trip.
        assert_eq!(entries[1].option, "es6");
    }

    #[test]
    fn an_unrecognised_line_is_an_error_rather_than_a_skip() {
        // A permissive parser would report a short table as a successful run, and
        // the missing libs would surface as unresolved globals much later.
        let source =
            "var LibMap = x({\n\t{Key: \"es5\", Value: \"lib.es5.d.ts\"},\n\tsomething_new,\n})\n";
        assert!(parse_lib_map(source).is_err());
    }

    #[test]
    fn a_renamed_lib_fails_generation() {
        let names = vec!["lib.es5.d.ts".to_string()];
        let map = vec![LibEntry { option: "es2015".into(), file: "lib.es2015.d.ts".into() }];
        assert!(generate(&names, &map).is_err());
    }

    #[test]
    fn a_missing_lib_map_is_an_error() {
        assert!(parse_lib_map("package tsoptions\n").is_err());
    }
}
