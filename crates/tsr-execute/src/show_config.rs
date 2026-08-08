//! `--showConfig` — the configuration, resolved, as JSON.
//!
//! Ported from `internal/tsoptions/showconfig.go` at the pinned commit.
//!
//! # Why this is worth having early
//!
//! It is the highest-value phase on `STATUS-cli.md`'s board for its size: all 17
//! of the `showConfig` baselines are emit-free, and the output is a pure
//! function of the option table — no program, no checker, nothing that can be
//! wrong for a reason outside this file.
//!
//! # What it prints, and what upstream prints
//!
//! Upstream serialises the options that were **set**, in declaration order,
//! rendering each through its own declaration so an enum comes out as the name
//! the user would write rather than as an integer. This does the same for the
//! options this port declares. It does **not** yet print `files`, `include`,
//! `exclude` or `references`, which upstream includes when the config had them —
//! those live on `ParsedCommandLine`'s raw map, which the config parser does not
//! keep past the options it understands.

use tsr_tsoptions::value::ConfigValue;

/// Render the resolved configuration (`showConfig`).
///
/// The shape, verified against `showConfig/Show-TSConfig-with-references.js`:
///
/// ```text
/// {
///     "compilerOptions": {
///         "composite": true,
///         "strict": true
///     },
///     "references": [
///         {
///             "path": "./packages/a"
///         }
///     ],
///     "files": [
///         "./src/index.ts"
///     ]
/// }
/// ```
///
/// # Rendered from the config as written, not from `CompilerOptions`
///
/// The first attempt walked the resolved [`CompilerOptions`] and printed every
/// field that looked set, in the declaration table's order. Both halves of that
/// were wrong. **Upstream prints the options in the order the config file wrote
/// them** — `module, strict, target, esModuleInterop` in
/// `Show-TSConfig-with-compileOnSave-and-more.js`, which is neither alphabetical
/// nor the table's order — and it prints the *written* value, so an option this
/// port has no field for still appears. Walking the raw map gives both for free
/// and deletes the per-option arm the first version needed.
///
/// **Section order is fixed and is not the config's**: `compilerOptions`,
/// `references`, `files`, `include`, `exclude`.
///
/// # What is still missing
///
/// **Implied options.** Upstream appends the options a setting implies —
/// `useDefineForClassFields` under `target: es2022`, the `module: nodenext`
/// family — computed from the resolved options rather than read from the config.
/// `Show-TSConfig-with-transitively-implied-options.js` is the case that wants
/// them and it fails for that reason alone.
#[must_use]
pub fn show_config(
    root_files: &[String],
    raw: &tsr_core::OrderedMap<ConfigValue>,
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    let options = match raw.get("compilerOptions") {
        Some(ConfigValue::Map(map)) => map
            .entries()
            .filter_map(|(name, value)| {
                render_value(value, 2).map(|rendered| format!("        \"{name}\": {rendered}"))
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    if options.is_empty() {
        sections.push("    \"compilerOptions\": {}".to_string());
    } else {
        sections.push(format!("    \"compilerOptions\": {{\n{}\n    }}", options.join(",\n")));
    }

    if let Some(ConfigValue::List(references)) = raw.get("references") {
        let rendered: Vec<String> = references
            .iter()
            .filter_map(|reference| render_value(reference, 2))
            .map(|text| format!("        {text}"))
            .collect();
        if !rendered.is_empty() {
            sections.push(format!("    \"references\": [\n{}\n    ]", rendered.join(",\n")));
        }
    }

    if !root_files.is_empty() {
        let compare = tsr_path::ComparePathsOptions {
            use_case_sensitive_file_names,
            current_directory: current_directory.to_string(),
        };
        let relative: Vec<String> =
            root_files.iter().map(|file| relative_for_display(file, &compare)).collect();
        sections.push(format!("    \"files\": {}", string_array(&relative, 1)));
    }

    for key in ["include", "exclude"] {
        if let Some(ConfigValue::List(values)) = raw.get(key) {
            let strings: Vec<String> =
                values.iter().filter_map(|value| value.as_str().map(str::to_string)).collect();
            if !strings.is_empty() {
                sections.push(format!("    \"{key}\": {}", string_array(&strings, 1)));
            }
        }
    }

    format!("{{\n{}\n}}\n", sections.join(",\n"))
}

/// A config value as JSON, indented `level` steps of four.
///
/// `None` for a value that upstream omits: an explicit `null`, which means
/// "unset this" rather than "print null".
fn render_value(value: &ConfigValue, level: usize) -> Option<String> {
    let inner = "    ".repeat(level + 1);
    let outer = "    ".repeat(level);
    match value {
        ConfigValue::Null => None,
        ConfigValue::Bool(flag) => Some(flag.to_string()),
        ConfigValue::Number(number) => Some(render_number(*number)),
        ConfigValue::String(text) => Some(format!("\"{}\"", escape(text))),
        ConfigValue::List(values) => {
            let rendered: Vec<String> = values
                .iter()
                .filter_map(|entry| render_value(entry, level + 1))
                .map(|text| format!("{inner}{text}"))
                .collect();
            if rendered.is_empty() {
                Some("[]".to_string())
            } else {
                Some(format!("[\n{}\n{outer}]", rendered.join(",\n")))
            }
        }
        ConfigValue::Map(map) => {
            let rendered: Vec<String> = map
                .entries()
                .filter_map(|(name, entry)| {
                    render_value(entry, level + 1)
                        .map(|text| format!("{inner}\"{}\": {text}", escape(name)))
                })
                .collect();
            if rendered.is_empty() {
                Some("{}".to_string())
            } else {
                Some(format!("{{\n{}\n{outer}}}", rendered.join(",\n")))
            }
        }
    }
}

/// A JSON number, printed as an integer when it is one.
///
/// JSON has one numeric type and so does [`ConfigValue`], but `maxNodeModuleJsDepth: 2`
/// must not render as `2.0`.
fn render_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{number:.0}")
    } else {
        number.to_string()
    }
}

/// A path as `--showConfig` prints it: relative, and explicitly so.
///
/// The `./` prefix distinguishes a relative path from a bare module name in a
/// `files` array, and upstream emits it.
fn relative_for_display(path: &str, compare: &tsr_path::ComparePathsOptions) -> String {
    let relative = tsr_path::convert_to_relative_path(path, compare);
    if relative.starts_with('.') || relative.starts_with('/') {
        relative
    } else {
        format!("./{relative}")
    }
}

/// A JSON array of strings, one per line, indented `level` steps of four.
fn string_array(values: &[String], level: usize) -> String {
    let inner = "    ".repeat(level + 1);
    let outer = "    ".repeat(level);
    let rendered: Vec<String> =
        values.iter().map(|value| format!("{inner}\"{}\"", escape(value))).collect();
    format!("[\n{}\n{outer}]", rendered.join(",\n"))
}

/// The two escapes a path or an option value can need.
///
/// Deliberately not a full JSON string escaper: a value here is a path, an
/// identifier or a flag, and the control characters a general escaper handles
/// cannot reach this point. A backslash matters because Windows paths carry them.
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_core::OrderedMap;

    fn config(entries: &[(&str, ConfigValue)]) -> OrderedMap<ConfigValue> {
        let mut map = OrderedMap::default();
        for (name, value) in entries {
            map.set((*name).to_string(), value.clone());
        }
        map
    }

    fn options(entries: &[(&str, ConfigValue)]) -> OrderedMap<ConfigValue> {
        config(&[("compilerOptions", ConfigValue::Map(config(entries)))])
    }

    #[test]
    fn an_empty_configuration_collapses_to_an_empty_object() {
        assert_eq!(
            show_config(&[], &OrderedMap::default(), "/home/project", true),
            "{\n    \"compilerOptions\": {}\n}\n"
        );
    }

    #[test]
    fn options_keep_the_order_the_config_wrote_them_in() {
        // Not alphabetical and not the declaration table's — this is the whole
        // reason the renderer walks the raw map.
        let raw = options(&[
            ("module", ConfigValue::String("commonjs".to_string())),
            ("strict", ConfigValue::Bool(true)),
            ("target", ConfigValue::String("es5".to_string())),
        ]);
        let text = show_config(&[], &raw, "/home/project", true);
        let module = text.find("\"module\"").expect("module");
        let strict = text.find("\"strict\"").expect("strict");
        let target = text.find("\"target\"").expect("target");
        assert!(module < strict && strict < target, "{text}");
    }

    #[test]
    fn a_root_file_is_relative_and_dot_prefixed() {
        let text = show_config(
            &["/home/project/src/index.ts".to_string()],
            &OrderedMap::default(),
            "/home/project",
            true,
        );
        assert!(text.contains("\"./src/index.ts\""), "{text}");
    }

    #[test]
    fn references_are_objects_and_come_before_files() {
        let mut reference = OrderedMap::default();
        reference.set("path".to_string(), ConfigValue::String("./packages/a".to_string()));
        let raw = config(&[("references", ConfigValue::List(vec![ConfigValue::Map(reference)]))]);
        let text = show_config(&["/home/project/a.ts".to_string()], &raw, "/home/project", true);
        assert!(text.contains("\"path\": \"./packages/a\""), "{text}");
        assert!(
            text.find("\"references\"") < text.find("\"files\""),
            "sections have a fixed order: {text}"
        );
    }

    #[test]
    fn an_explicit_null_is_omitted_rather_than_printed() {
        let raw = options(&[("outDir", ConfigValue::Null), ("strict", ConfigValue::Bool(true))]);
        let text = show_config(&[], &raw, "/home/project", true);
        assert!(!text.contains("outDir"), "{text}");
        assert!(text.contains("\"strict\": true"), "{text}");
    }

    #[test]
    fn a_whole_number_does_not_render_a_decimal_point() {
        assert_eq!(render_number(2.0), "2");
        assert_eq!(render_number(2.5), "2.5");
    }

    #[test]
    fn a_backslash_in_a_path_is_escaped() {
        // A Windows `outDir` would otherwise render invalid JSON.
        let raw = options(&[("outDir", ConfigValue::String(r"C:\out".to_string()))]);
        assert!(show_config(&[], &raw, "/home/project", true).contains(r#""outDir": "C:\\out""#));
    }
}
