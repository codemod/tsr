//! Node-shape conformance against typescript-go's AST definition.
//!
//! `kind_conformance.rs` proves we have the right *set of node kinds*. This proves
//! we have the right *fields on each node*.
//!
//! The check is textual: it re-derives the expected field set from
//! `_scripts/ast.json` and compares it against the doc comments the generator
//! emits above each field. That is deliberate — Rust has no reflection, and
//! reading back the generated source is what catches the failure mode we actually
//! hit while building this (a member silently dropped because a JSON shape was
//! mismatched). Asserting the generator's output against its own in-memory model
//! would have caught nothing.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsr-ast is two levels below the repo root")
        .to_path_buf()
}

/// Fields upstream declares but our AST intentionally omits.
///
/// `goOnly` members are binder and checker state; under PLAN.md §3.2 they live in
/// id-keyed side tables. `Flags` and `modifierFlags` live on the node envelope
/// (`NodeTable`) rather than on each node struct.
fn is_intentionally_omitted(name: &str) -> bool {
    matches!(name, "Flags" | "modifierFlags")
}

/// Collect a base's transitive fields, nearest declaration winning.
fn base_fields(bases: &BTreeMap<String, Value>, name: &str, out: &mut BTreeMap<String, Value>) {
    let Some(base) = bases.get(name) else { return };
    if let Some(parents) = base.get("extends").and_then(Value::as_array) {
        for parent in parents.iter().filter_map(Value::as_str) {
            base_fields(bases, parent, out);
        }
    }
    if let Some(fields) = base.get("fields").and_then(Value::as_object) {
        for (field_name, field) in fields {
            out.entry(field_name.clone()).or_insert_with(|| field.clone());
        }
    }
}

fn skipped(entry: &Value) -> bool {
    entry.get("goOnly").and_then(Value::as_bool).unwrap_or(false)
        || entry.get("noGo").and_then(Value::as_bool).unwrap_or(false)
}

/// Extract `struct name -> field names` from the generated source.
///
/// Reads the doc line the generator emits above each field, which has the form
/// `Name`: `Type` inside a `///` comment.
fn generated_structs(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut fields = BTreeSet::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("pub struct ") {
            if let Some(name) = current.take() {
                out.insert(name, std::mem::take(&mut fields));
            }
            let name = rest.split('<').next().unwrap_or(rest).trim().to_string();
            current = Some(name);
        } else if trimmed == "}" {
            if let Some(name) = current.take() {
                out.insert(name, std::mem::take(&mut fields));
            }
        } else if let Some(rest) = trimmed.strip_prefix("/// `") {
            if let Some(name) = rest.split('`').next() {
                fields.insert(name.to_string());
            }
        }
    }
    if let Some(name) = current {
        out.insert(name, fields);
    }
    out
}

#[test]
fn every_node_definition_has_a_struct_with_all_its_fields() {
    let ast_path = repo_root().join("vendor/typescript-go/_scripts/ast.json");
    let Ok(raw) = std::fs::read_to_string(&ast_path) else {
        eprintln!("skipping: vendor/typescript-go not checked out");
        return;
    };
    let ast: Value = serde_json::from_str(&raw).expect("ast.json is valid JSON");

    let bases: BTreeMap<String, Value> = ast["bases"]
        .as_object()
        .expect("bases is an object")
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let definitions = ast["nodes"]["definitions"].as_object().expect("definitions is an object");

    let generated_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/generated/nodes.rs");
    let generated = std::fs::read_to_string(generated_path).expect("generated nodes.rs exists");
    let structs = generated_structs(&generated);

    let mut problems: Vec<String> = Vec::new();

    for (node_name, def) in definitions {
        // The generic `Token[TKind]` is hand-written in lib.rs; it has no distinct
        // shape per instantiation.
        if node_name == "Token" {
            continue;
        }

        let Some(actual) = structs.get(node_name.as_str()) else {
            problems.push(format!("{node_name}: no generated struct"));
            continue;
        };

        // Expected = own members (in order) plus inherited base fields.
        let mut inherited = BTreeMap::new();
        if let Some(extends) = def.get("extends").and_then(Value::as_array) {
            for base in extends.iter().filter_map(Value::as_str) {
                base_fields(&bases, base, &mut inherited);
            }
        }

        let mut expected: BTreeSet<String> = BTreeSet::new();
        if let Some(members) = def.get("members").and_then(Value::as_array) {
            for member in members {
                let Some(name) = member.get("name").and_then(Value::as_str) else { continue };
                if skipped(member) || is_intentionally_omitted(name) {
                    continue;
                }
                // An inherited member with no type of its own resolves to the base
                // declaration; if that is `goOnly`, the field does not exist here.
                let inherited_decl = inherited.get(name);
                let has_own_type = member.get("type").is_some();
                if !has_own_type {
                    match inherited_decl {
                        Some(decl) if skipped(decl) => continue,
                        Some(decl) if decl.get("type").is_none() => continue,
                        None => continue,
                        _ => {}
                    }
                }
                expected.insert(name.to_string());
            }
        }
        for (name, decl) in &inherited {
            if skipped(decl) || is_intentionally_omitted(name) || decl.get("type").is_none() {
                continue;
            }
            expected.insert(name.clone());
        }

        let missing: Vec<&String> = expected.difference(actual).collect();
        if !missing.is_empty() {
            problems.push(format!("{node_name}: missing fields {missing:?}"));
        }
    }

    assert!(
        problems.is_empty(),
        "node shape conformance failed ({} nodes):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

#[test]
fn every_node_definition_appears_in_the_node_union() {
    let ast_path = repo_root().join("vendor/typescript-go/_scripts/ast.json");
    let Ok(raw) = std::fs::read_to_string(&ast_path) else {
        eprintln!("skipping: vendor/typescript-go not checked out");
        return;
    };
    let ast: Value = serde_json::from_str(&raw).expect("ast.json is valid JSON");
    let definitions = ast["nodes"]["definitions"].as_object().expect("definitions is an object");

    let alias_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/generated/alias.rs");
    let alias_src = std::fs::read_to_string(alias_path).expect("generated alias.rs exists");

    let node_enum = alias_src
        .split_once("pub enum Node<'a> {")
        .expect("Node union is generated")
        .1
        .split_once("\n}")
        .expect("Node union is terminated")
        .0;

    let missing: Vec<&String> = definitions
        .keys()
        .filter(|name| !node_enum.contains(&format!("{name}(&'a {name}<'a>)")))
        .collect();

    assert!(missing.is_empty(), "nodes absent from the Node union: {missing:?}");
}
