//! Maintenance tasks for tsr.
//!
//! ```text
//! cargo xtask codegen    regenerate crates/tsr-ast/src/generated
//! cargo xtask perf       compare against typescript-go; write perf artifacts
//! cargo xtask anchors    verify every upstream anchor still resolves
//!                        --upstream <path> checks a newer checkout, which is
//!                        the drift report (bd tsr-l68)
//! cargo xtask issue-ids  verify every `bd` id cited in docs/ actually exists
//! cargo xtask gate       the five per-build checks, in one command that FAILS
//! cargo xtask measure    clippy, then — only if it is clean — the conformance run
//! ```
//!
//! Codegen reads `vendor/typescript-go/_scripts/ast.json`, the same
//! schema-validated definition upstream uses to generate its own Go AST. Deriving
//! our AST from that file is what makes conformance a build property rather than a
//! claim: when upstream adds a node kind, regeneration picks it up and the
//! conformance tests fail until we do.

mod anchors;
mod ast_json;
mod gate;
mod gen_diagnostics;
mod gen_kind;
mod gen_libs;
mod gen_nodes;
mod gen_unicode;
mod issue_ids;
mod measure;
mod perf;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};

use ast_json::AstDefinition;

fn main() -> Result<()> {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        Some("codegen") => codegen(),
        Some("perf") => perf::run(&workspace_root()),
        Some("anchors") => {
            let mut args = std::env::args().skip(2);
            let mut upstream = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--upstream" => upstream = args.next().map(PathBuf::from),
                    other => bail!("unknown flag {other:?}; expected `--upstream <path>`"),
                }
            }
            anchors::run(&workspace_root(), upstream)
        }
        Some("issue-ids") => issue_ids::run(&workspace_root()),
        Some("gate") => gate::run(&workspace_root()),
        Some("measure") => measure::run(&workspace_root()),
        Some(other) => {
            bail!(
                "unknown task {other:?}; expected `codegen`, `perf`, `anchors`, `issue-ids`, `gate` or `measure`"
            )
        }
        None => {
            eprintln!("usage: cargo xtask <codegen|perf|anchors|issue-ids|gate|measure>");
            Ok(())
        }
    }
}

/// Repository root, derived from this crate's manifest directory.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn codegen() -> Result<()> {
    let root = workspace_root();
    let ast_path = root.join("vendor/typescript-go/_scripts/ast.json");
    let source = fs::read_to_string(&ast_path).with_context(|| {
        format!(
            "reading {}\n\nThe vendored submodule may not be initialized; run:\n  \
             git submodule update --init vendor/typescript-go",
            ast_path.display()
        )
    })?;

    let ast: AstDefinition =
        serde_json::from_str(&source).context("parsing ast.json against our schema model")?;

    println!(
        "loaded ast.json: {} kinds, {} bases, {} node definitions, {} aliases",
        ast.kinds.kind_names().len(),
        ast.bases.len(),
        ast.nodes.definitions.len(),
        ast.nodes.aliases.len(),
    );

    let out_dir = root.join("crates/tsr-ast/src/generated");
    fs::create_dir_all(&out_dir).context("creating generated output directory")?;

    // Node-typed fields are nullable pointers in Go; `ast.json` does not say so.
    let go_ast = root.join("vendor/typescript-go/internal/ast/ast_generated.go");
    let nullability = gen_nodes::GoNullability::parse(
        &fs::read_to_string(&go_ast).with_context(|| format!("reading {}", go_ast.display()))?,
    );

    write_generated(&out_dir.join("kind.rs"), &gen_kind::generate(&ast)?)?;
    write_generated(&out_dir.join("nodes.rs"), &gen_nodes::generate_nodes(&ast, &nullability)?)?;
    write_generated(&out_dir.join("alias.rs"), &gen_nodes::generate_aliases(&ast, &nullability)?)?;
    write_generated(&out_dir.join("visit.rs"), &gen_nodes::generate_visit(&ast, &nullability)?)?;

    // A machine-readable record of what the generator saw, so the conformance test
    // can assert against upstream without re-parsing ast.json at test time.
    let manifest = serde_json::json!({
        "source": "vendor/typescript-go/_scripts/ast.json",
        "kind_count": ast.kinds.kind_names().len(),
        "kinds": ast.kinds.kind_names(),
        "node_count": ast.nodes.definitions.len(),
        "alias_count": ast.nodes.aliases.len(),
    });
    fs::write(
        out_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).context("serializing manifest")?,
    )
    .context("writing manifest.json")?;

    write_generated(&out_dir.join("mod.rs"), MOD_RS)?;
    println!("wrote {}", out_dir.display());

    codegen_diagnostics(&root)?;
    codegen_unicode(&root, &ast)?;

    let (lib_files, lib_options) = gen_libs::run(&root)?;
    println!("wrote bundled libs: {lib_files} files, {lib_options} --lib option values");

    Ok(())
}

const MOD_RS: &str = "//! Generated AST definitions.\n\
     //!\n\
     //! @generated by `cargo xtask codegen`. Do not edit by hand.\n\n\
     pub mod alias;\n\
     pub mod kind;\n\
     pub mod nodes;\n\
     pub mod visit;\n";

fn write_generated(path: &std::path::Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Generate the diagnostic message catalogue.
fn codegen_diagnostics(root: &std::path::Path) -> Result<()> {
    use std::collections::BTreeMap;

    use gen_diagnostics::RawMessage;

    let read = |path: PathBuf| -> Result<BTreeMap<String, RawMessage>> {
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        // The file is JSON with comments in places; strip a leading BOM which
        // several of these upstream data files carry.
        let source = source.strip_prefix('\u{feff}').unwrap_or(&source).to_string();
        serde_json::from_str(&source).with_context(|| format!("parsing {}", path.display()))
    };

    let base =
        read(root.join(
            "vendor/typescript-go/_submodules/TypeScript/src/compiler/diagnosticMessages.json",
        ))?;
    let extra =
        read(root.join("vendor/typescript-go/internal/diagnostics/extraDiagnosticMessages.json"))?;
    println!("loaded diagnostics: {} base + {} tsgo-specific", base.len(), extra.len());

    let out_dir = root.join("crates/tsr-diagnostics/src/generated");
    fs::create_dir_all(&out_dir).context("creating diagnostics output directory")?;
    write_generated(&out_dir.join("messages.rs"), &gen_diagnostics::generate(&base, &extra)?)?;
    write_generated(
        &out_dir.join("mod.rs"),
        "//! Generated diagnostic definitions.\n\
         //!\n\
         //! @generated by `cargo xtask codegen`. Do not edit by hand.\n\n\
         pub mod messages;\n",
    )?;
    println!("wrote {}", out_dir.display());
    Ok(())
}

/// Generate the Unicode identifier tables the scanner needs.
fn codegen_unicode(root: &std::path::Path, ast: &ast_json::AstDefinition) -> Result<()> {
    let path = root.join("vendor/typescript-go/internal/stringutil/identifier_parts_generated.go");
    let source =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;

    let out_dir = root.join("crates/tsr-scanner/src/generated");
    fs::create_dir_all(&out_dir).context("creating scanner output directory")?;
    write_generated(&out_dir.join("unicode.rs"), &gen_unicode::generate(&source)?)?;
    write_generated(&out_dir.join("keywords.rs"), &gen_kind::generate_keywords(ast)?)?;
    write_generated(
        &out_dir.join("mod.rs"),
        "//! Generated scanner tables.\n\
         //!\n\
         //! @generated by `cargo xtask codegen`. Do not edit by hand.\n\n\
         pub mod keywords;\n\
         pub mod unicode;\n",
    )?;
    println!("wrote {}", out_dir.display());
    Ok(())
}
