//! What does the absence of control-flow narrowing actually cost?
//!
//! `bd tsr-4sc.11` has been ranked for four cycles on the strength of a
//! prediction rather than a measurement — `docs/architecture/checker.md` said
//! narrowing "adds no bucket of its own and cannot be seen by this histogram".
//! That is true of the *shape* histogram, and it is not true of the baselines.
//!
//! # This reads baselines only. It does not run the checker or the corpus.
//!
//! Upstream's own `.types` baseline records both halves of every narrowing:
//!
//! ```text
//! var strOrBool: string | boolean;
//! >strOrBool : string | boolean      ← the declared type
//! …
//! if (typeof strOrBool === "boolean") {
//!     bool = strOrBool;
//! >strOrBool : boolean               ← narrowed
//! ```
//!
//! So the population is findable without asking this port anything: for each
//! name, the **first** assertion in a file section is its declared type, and any
//! later assertion whose type is a strict narrowing of it is a line this port
//! answers **wrongly** today — `checkIdentifier` is ported only as far as
//! "resolve the name, take the symbol's type", so we say the declared type at
//! every reference.
//!
//! That it reads only baselines is the point: it costs no corpus run, competes
//! with no other agent's measurement, and can be re-run by anyone in seconds.
//!
//! # What counts as a narrowing, and why the filter has to be strict
//!
//! An unfiltered "the type differs from the declared type" count reads **24,016**
//! and is worthless: it is dominated by generic instantiation (`f` declared
//! `(x: "foo") => "foo"`, referenced as `(x: T) => T`), by two different symbols
//! sharing a name across scopes, and by property names colliding with variable
//! names. Narrowing has a specific signature and only these shapes are counted:
//!
//! - the reference is a **proper subset** of the declared union's constituents;
//! - the reference is a **literal of** the declared primitive (`boolean` → `true`);
//! - every reference constituent is a literal of some declared constituent.
//!
//! Constituents are split at **top-level** `|` only, respecting bracket depth —
//! a substring test counts nested unions inside signatures and object types, an
//! error this project has now made twice.
//!
//! # What the number is *not*
//!
//! It is an **upper bound**, for two reasons that both cut the same way. A line
//! is only lost to narrowing if this port can compute the declared type at all,
//! and it currently types 43.40% of aligned lines. And the walker's comparison is
//! positional, so a case already failing earlier loses these lines regardless.

use std::collections::HashMap;

use tsr_conformance::{Corpus, repo_root};

/// The shapes counted, in report order.
const KINDS: [&str; 3] =
    ["union constituent(s)", "literal of the primitive", "literals of the constituents"];

fn main() {
    let corpus = Corpus::new(repo_root().join("vendor/typescript-go"));
    let root = corpus.baselines_root();
    let mut bare = [0usize; 3];
    let mut dotted = [0usize; 3];
    let mut files = 0usize;
    let mut examples: Vec<(String, String, String, String)> = Vec::new();

    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "types") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let mut hit = false;
            let mut first: HashMap<&str, &str> = HashMap::new();
            for line in text.lines() {
                if line.starts_with("=== ") {
                    first.clear();
                    continue;
                }
                let Some((reference, ty)) = assertion(line) else { continue };
                let Some(&declared) = first.get(reference) else {
                    first.insert(reference, ty);
                    continue;
                };
                let Some(kind) = narrowing_kind(declared, ty) else { continue };
                hit = true;
                let bucket = if reference.contains('.') { &mut dotted } else { &mut bare };
                bucket[kind] += 1;
                if examples.len() < 8 && !reference.contains('.') {
                    examples.push((
                        name.clone(),
                        reference.to_string(),
                        declared.to_string(),
                        ty.to_string(),
                    ));
                }
            }
            if hit {
                files += 1;
            }
        }
    }

    let total: usize = bare.iter().chain(dotted.iter()).sum();
    println!("narrowed reference lines, over baselines only\n");
    println!("{:<34}{:>12}{:>14}", "", "bare name", "a.b / this.x");
    for (index, kind) in KINDS.iter().enumerate() {
        println!("{kind:<34}{:>12}{:>14}", bare[index], dotted[index]);
    }
    println!(
        "{:<34}{:>12}{:>14}",
        "total",
        bare.iter().sum::<usize>(),
        dotted.iter().sum::<usize>()
    );
    println!("\n{total} lines across {files} baseline files");
    #[allow(clippy::cast_precision_loss, reason = "a line count is far inside f64's exact range")]
    let share = 100.0 * total as f64 / 468_921.0;
    println!("{share:.2}% of the 468,921 assertion lines the walker aligns");
    println!("\nworked examples:");
    for (file, name, declared, narrowed) in &examples {
        let file: String = file.chars().take(30).collect();
        println!("  {file:<32}{name:<14}{declared:<26} -> {narrowed}");
    }
}

/// `>{reference} : {type}` where the reference is a plain or dotted name.
///
/// Anything else — a call, an index, a literal — is not a reference whose type
/// narrowing could change, and including it would count unrelated differences.
fn assertion(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('>')?;
    let (reference, ty) = rest.split_once(" : ")?;
    let mut chars = reference.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.') {
        return None;
    }
    Some((reference, ty))
}

/// Which narrowing shape this pair is, if any. See the module docs.
fn narrowing_kind(declared: &str, reference: &str) -> Option<usize> {
    if declared == reference {
        return None;
    }
    let d = split_top_level(declared);
    let r = split_top_level(reference);
    if d.len() > 1 && !r.is_empty() && r.len() < d.len() && r.iter().all(|part| d.contains(part)) {
        return Some(0);
    }
    if d.len() == 1 && r.len() == 1 && is_literal_of(declared.trim(), reference.trim()) {
        return Some(1);
    }
    if !r.is_empty()
        && r != d
        && r.iter().all(|part| d.contains(part) || d.iter().any(|base| is_literal_of(base, part)))
    {
        return Some(2);
    }
    None
}

/// Whether `literal` is a literal type of the primitive `base`.
fn is_literal_of(base: &str, literal: &str) -> bool {
    match base {
        "string" => literal.starts_with('"') && literal.ends_with('"') && literal.len() >= 2,
        "boolean" => literal == "true" || literal == "false",
        "number" => literal.parse::<f64>().is_ok(),
        "bigint" => literal.strip_suffix('n').is_some_and(|n| n.parse::<i128>().is_ok()),
        _ => false,
    }
}

/// Split at top-level `|`, respecting bracket depth.
///
/// A substring test counts nested unions inside a signature or an object type,
/// which is how a union-line denominator got inflated from 17,532 to 30,943.
fn split_top_level(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let (mut parts, mut depth, mut start, mut index) = (Vec::new(), 0i32, 0usize, 0usize);
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            _ => {}
        }
        if depth == 0 && bytes[index..].starts_with(b" | ") {
            parts.push(text[start..index].trim());
            index += 3;
            start = index;
            continue;
        }
        index += 1;
    }
    parts.push(text[start..].trim());
    parts.retain(|part| !part.is_empty());
    parts
}
