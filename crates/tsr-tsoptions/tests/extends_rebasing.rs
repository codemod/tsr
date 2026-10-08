//! `extends` chains loaded from a real temporary directory, asserting native's
//! rebasing of inherited `include`/`exclude`/`files` and path options.
//!
//! Native rules (`parseConfig`, `vendor/typescript-go/internal/tsoptions/
//! tsconfigparsing.go:1101-1170` @ `5b1047d`):
//!
//! - A base's `include`/`exclude`/`files` pass to the extending config only
//!   when it writes none of its own, each relative spec prefixed with the path
//!   from the extending config's directory to the base's directory; rooted and
//!   `${configDir}` specs are unchanged. Applied per hop, so a two-level chain
//!   rebases twice.
//! - Path-valued options (`outDir`, `rootDir`, ...) are made absolute against
//!   the directory of the config that wrote them, before any merge.
//! - Every other root key (`references`, a base's own `extends`) stays with
//!   the file that wrote it.
//!
//! Each expectation was cross-checked with native `tsgo --showConfig` and
//! `--listFilesOnly` on the same tree (docs/parity/notes/r4-config.md).

use std::path::{Path, PathBuf};

use tsr_tsoptions::{ParsedCommandLine, parse_config_file, value::ConfigValue};
use tsr_vfs::OsFileSystem;

/// A fresh directory under the system temp dir, removed on drop.
struct TempTree(PathBuf);

impl TempTree {
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!("tsr-extends-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        // Canonical, so a symlinked temp dir (macOS `/var`) matches the
        // loader's own paths.
        Self(std::fs::canonicalize(&root).unwrap())
    }

    fn path(&self, relative: &str) -> String {
        tsr_path::normalize_slashes(&self.0.join(relative).to_string_lossy())
    }

    fn root(&self) -> String {
        tsr_path::normalize_slashes(&self.0.to_string_lossy())
    }

    fn parse(&self, config: &str) -> ParsedCommandLine {
        let config = self.path(config);
        let text = std::fs::read_to_string(&config).unwrap();
        let directory = tsr_path::get_directory_path(&config).to_string();
        parse_config_file(&config, &text, &directory, &OsFileSystem::new())
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(Path::new(&self.0));
    }
}

fn specs(parsed: &ParsedCommandLine, name: &str) -> Option<Vec<String>> {
    Some(
        parsed
            .raw
            .get(name)?
            .as_list()?
            .iter()
            .map(|spec| spec.as_str().unwrap().to_string())
            .collect(),
    )
}

#[test]
fn inherited_include_and_exclude_resolve_against_the_base_directory() {
    let tree = TempTree::new(
        "one-hop",
        &[
            (
                "base/tsconfig.json",
                r#"{ "compilerOptions": { "outDir": "out", "rootDir": "." },
                     "include": ["*.ts"], "exclude": ["skip.ts"] }"#,
            ),
            ("base/a.ts", ""),
            ("base/skip.ts", ""),
            ("ext/tsconfig.json", r#"{ "extends": "../base/tsconfig.json" }"#),
            ("ext/own.ts", ""),
        ],
    );
    let parsed = tree.parse("ext/tsconfig.json");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.file_names, [tree.path("base/a.ts")]);
    assert_eq!(specs(&parsed, "include").unwrap(), ["../base/*.ts"]);
    assert_eq!(specs(&parsed, "exclude").unwrap(), ["../base/skip.ts"]);
    assert_eq!(parsed.compiler_options.out_dir, tree.path("base/out"));
    assert_eq!(parsed.compiler_options.root_dir, tree.path("base"));
}

#[test]
fn a_two_level_chain_rebases_at_each_hop() {
    let tree = TempTree::new(
        "two-hop",
        &[
            (
                "configs/base/tsconfig.base.json",
                r#"{ "compilerOptions": { "declarationDir": "../../types" },
                     "files": ["../../src/main.ts"],
                     "include": ["../../src/lib/**/*"],
                     "references": [{ "path": "../../other" }] }"#,
            ),
            (
                "configs/mid/tsconfig.json",
                r#"{ "extends": "../base/tsconfig.base.json",
                     "compilerOptions": { "outDir": "../../dist" },
                     "exclude": ["../../src/lib/gen/**"] }"#,
            ),
            ("app/tsconfig.json", r#"{ "extends": "../configs/mid/tsconfig.json" }"#),
            ("src/main.ts", ""),
            ("src/lib/util.ts", ""),
            ("src/lib/gen/out.ts", ""),
            ("app/local.ts", ""),
        ],
    );
    let parsed = tree.parse("app/tsconfig.json");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(specs(&parsed, "files").unwrap(), ["../configs/mid/../base/../../src/main.ts"]);
    assert_eq!(specs(&parsed, "include").unwrap(), ["../configs/mid/../base/../../src/lib/**/*"]);
    assert_eq!(specs(&parsed, "exclude").unwrap(), ["../configs/mid/../../src/lib/gen/**"]);
    assert_eq!(parsed.file_names, [tree.path("src/main.ts"), tree.path("src/lib/util.ts")]);
    assert_eq!(parsed.literal_file_count, 1);
    assert_eq!(parsed.compiler_options.out_dir, tree.path("dist"));
    assert_eq!(parsed.compiler_options.declaration_dir, tree.path("types"));
    // `references` is not an inherited key.
    assert!(parsed.raw.get("references").is_none());
    // Nor is the mid config's own `extends`: `extends` here is the app's.
    assert_eq!(
        parsed.raw.get("extends"),
        Some(&ConfigValue::String("../configs/mid/tsconfig.json".to_string()))
    );
}

#[test]
fn own_specs_win_and_only_missing_ones_are_inherited() {
    let tree = TempTree::new(
        "own-wins",
        &[
            (
                "base/tsconfig.json",
                r#"{ "include": ["base-only/**/*"], "exclude": ["**/*.spec.ts"] }"#,
            ),
            ("proj/tsconfig.json", r#"{ "extends": "../base/tsconfig.json", "include": ["src"] }"#),
            ("proj/src/a.ts", ""),
            ("proj/src/a.spec.ts", ""),
            ("base/base-only/b.ts", ""),
        ],
    );
    let parsed = tree.parse("proj/tsconfig.json");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(specs(&parsed, "include").unwrap(), ["src"]);
    assert_eq!(specs(&parsed, "exclude").unwrap(), ["../base/**/*.spec.ts"]);
    // The rebased exclude matches nothing under `proj/`, as in native.
    assert_eq!(parsed.file_names, [tree.path("proj/src/a.spec.ts"), tree.path("proj/src/a.ts")]);
}

#[test]
fn rooted_and_config_dir_specs_are_not_rebased() {
    let tree = TempTree::new(
        "rooted",
        &[
            ("shared/tsconfig.json", ""), // replaced below once the root is known
            ("proj/tsconfig.json", r#"{ "extends": "../shared/tsconfig.json" }"#),
            ("proj/src/a.ts", ""),
            ("abs/b.ts", ""),
        ],
    );
    std::fs::write(
        tree.path("shared/tsconfig.json"),
        format!(r#"{{ "include": ["${{configDir}}/src", "{}/abs"] }}"#, tree.root()),
    )
    .unwrap();
    let parsed = tree.parse("proj/tsconfig.json");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(
        specs(&parsed, "include").unwrap(),
        ["${configDir}/src".to_string(), format!("{}/abs", tree.root())]
    );
    // `${configDir}` is the invoked config's directory.
    assert_eq!(parsed.file_names, [tree.path("proj/src/a.ts"), tree.path("abs/b.ts")]);
}

#[test]
fn a_later_extends_entry_overwrites_an_earlier_ones_specs() {
    let tree = TempTree::new(
        "array",
        &[
            ("one/tsconfig.json", r#"{ "include": ["first/*.ts"] }"#),
            ("two/sub/tsconfig.json", r#"{ "include": ["second/*.ts"], "compileOnSave": true }"#),
            (
                "proj/tsconfig.json",
                r#"{ "extends": ["../one/tsconfig.json", "../two/sub/tsconfig.json"] }"#,
            ),
            ("two/sub/second/c.ts", ""),
            ("one/first/d.ts", ""),
        ],
    );
    let parsed = tree.parse("proj/tsconfig.json");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(specs(&parsed, "include").unwrap(), ["../two/sub/second/*.ts"]);
    assert_eq!(parsed.file_names, [tree.path("two/sub/second/c.ts")]);
    assert_eq!(parsed.raw.get("compileOnSave"), Some(&ConfigValue::Bool(true)));
}

#[test]
fn a_base_in_the_same_directory_keeps_its_specs_verbatim() {
    let tree = TempTree::new(
        "same-dir",
        &[
            ("p/tsconfig.base.json", r#"{ "include": ["src/**/*"] }"#),
            ("p/tsconfig.json", r#"{ "extends": "./tsconfig.base.json" }"#),
            ("p/src/a.ts", ""),
            ("p/other.ts", ""),
        ],
    );
    let parsed = tree.parse("p/tsconfig.json");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(specs(&parsed, "include").unwrap(), ["src/**/*"]);
    assert_eq!(parsed.file_names, [tree.path("p/src/a.ts")]);
}
