//! Which bundled `lib.*.d.ts` files a set of options selects, and in what order.
//!
//! Ported from `internal/tsoptions/enummaps.go` at the pinned commit. The two
//! tables this reads — [`crate::LIB_MAP`] and [`crate::LIB_NAMES`] — are
//! generated; what is here is the three questions the file loader asks of them.
//!
//! # Why the option name and the file name are not interchangeable
//!
//! `--lib` accepts *either*: `es2015` and `lib.es2015.d.ts` both select the same
//! file, and `GetLibFileName` (`enummaps.go:132`) tries the file-name spelling
//! **first**. The set it tries against is built from `LibMap`'s *values*
//! (`enummaps.go:129`), not from the shipped file list, and that distinction is
//! load-bearing rather than incidental: `lib.d.ts` is a shipped file that no
//! `--lib` value selects, so `--lib lib.d.ts` is an error upstream and is an error
//! here. Checking against [`crate::LIB_NAMES`] instead would quietly accept it.
//!
//! # Order is behaviour
//!
//! [`lib_option_index`] exists for `fileLoader.getDefaultLibFilePriority`
//! (`internal/compiler/fileloader.go:321`), which sorts the lib files of a program
//! by their position in upstream's `Libs` — and `Libs` is `LibMap`'s keys in
//! declaration order (`enummaps.go:128`). Load order decides which declaration of
//! a merged global interface is seen first, so a sort that looked tidier by
//! sorting alphabetically would change what the program means.

/// Every `--lib` value, in upstream's declaration order.
///
/// The order is the error message's: `Argument for '--lib' option must be:
/// 'es5', 'es6', 'es2015', …` lists them exactly like this, and the list is
/// baselined. `LIB_MAP` is generated from upstream's own table, so this cannot
/// drift from what [`get_lib_file_name`] accepts.
pub fn lib_option_names() -> impl Iterator<Item = &'static str> {
    crate::LIB_MAP.iter().map(|(name, _)| *name)
}

/// The file a `--lib` value selects (`tsoptions.GetLibFileName`).
///
/// Accepts the option spelling (`es2015`) or the file spelling
/// (`lib.es2015.d.ts`), case-insensitively. `None` for a value that names
/// neither, which upstream reports as an unknown-lib error.
#[must_use]
pub fn get_lib_file_name(lib_name: &str) -> Option<&'static str> {
    // `tspath.ToFileNameLowerCase`, which is ASCII-only on purpose: a file
    // system's case folding is not Unicode's.
    let lowered = lib_name.to_ascii_lowercase();
    // `LibFilesSet` — the *values* of `LibMap`, so a file name that no option
    // selects is not accepted. See the module docs.
    if let Some((_, file)) = crate::LIB_MAP.iter().find(|(_, file)| *file == lowered) {
        return Some(file);
    }
    crate::LIB_MAP.iter().find(|(option, _)| *option == lowered).map(|(_, file)| *file)
}

/// Where a lib file sits in upstream's load order (`tsoptions.Libs`).
///
/// The index of the **first** `--lib` option that selects `file_name`, since
/// several options may name one file (`esnext.symbol` and `es2019.symbol` are both
/// `lib.es2019.symbol.d.ts`) and upstream's `slices.Index` takes the first hit.
/// `None` for a file no option selects, which includes `lib.d.ts` and every
/// `lib.*.full.d.ts`.
#[must_use]
pub fn lib_option_index(file_name: &str) -> Option<usize> {
    // Upstream indexes `Libs` — the option *names* — by the name recovered from
    // the file name (`fileloader.go:333`: strip `lib.` and `.d.ts`). That
    // recovery and this lookup agree wherever the recovered name is an option,
    // and this way round needs no string surgery.
    crate::LIB_MAP.iter().position(|(_, file)| *file == file_name)
}

/// Every lib file a set of options loads, in the order they are added
/// (`fileloader.go:157`).
///
/// Empty under `noLib`, or when the program has no root files — upstream guards
/// the whole block on `len(rootFiles) > 0`, and that guard is the caller's here
/// because this function does not know about root files.
///
/// An unknown `--lib` entry is **skipped**, matching upstream's `if name, ok :=
/// …; ok` with its `// !!! error on unknown name` still outstanding. It is not
/// substituted with a default: a program that asked for a lib it cannot have gets
/// fewer libs, not different ones.
#[must_use]
pub fn lib_file_names(options: &tsr_core::CompilerOptions) -> Vec<&'static str> {
    lib_file_names_with_index(options).into_iter().map(|(_, name)| name).collect()
}

/// [`lib_file_names`], each with the index of the `lib` entry that named it
/// (`fileIncludeKindLibFile`'s `data`, `fileloader.go:158-170`): `None` for
/// the default library, which no entry names.
#[must_use]
pub fn lib_file_names_with_index(
    options: &tsr_core::CompilerOptions,
) -> Vec<(Option<usize>, &'static str)> {
    if options.no_lib.is_true() {
        return Vec::new();
    }
    if options.lib.is_empty() {
        // Upstream's condition is `compilerOptions.Lib == nil`, distinguishing an
        // unset `lib` from an explicitly empty one. `CompilerOptions::lib` is a
        // `Vec<String>` with no such distinction, so an explicit `"lib": []`
        // loads the default here and loads nothing upstream. Named as a
        // divergence in docs/architecture/program.md rather than papered over:
        // no corpus case writes `"lib": []`, checked.
        return vec![(None, options.default_lib_file_name())];
    }
    options
        .lib
        .iter()
        .enumerate()
        .filter_map(|(index, lib)| get_lib_file_name(lib).map(|name| (Some(index), name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use tsr_core::{CompilerOptions, ScriptTarget, Tristate};

    use super::*;

    #[test]
    fn a_lib_option_selects_its_file_by_either_spelling() {
        assert_eq!(get_lib_file_name("es2015"), Some("lib.es2015.d.ts"));
        assert_eq!(get_lib_file_name("lib.es2015.d.ts"), Some("lib.es2015.d.ts"));
        assert_eq!(get_lib_file_name("ES2015"), Some("lib.es2015.d.ts"), "case-insensitive");
        assert_eq!(get_lib_file_name("es6"), Some("lib.es2015.d.ts"), "an alias");
    }

    #[test]
    fn a_shipped_file_that_no_option_selects_is_not_a_lib_value() {
        // The distinction the module docs argue for. `lib.d.ts` ships and is the
        // ES5 default, but `LibFilesSet` is built from `LibMap`'s values and does
        // not contain it, so `--lib lib.d.ts` is an error upstream.
        assert!(crate::LIB_NAMES.contains(&"lib.d.ts"));
        assert_eq!(get_lib_file_name("lib.d.ts"), None);
        assert_eq!(get_lib_file_name("es2020.full"), None);
        assert_eq!(get_lib_file_name("nonsense"), None);
    }

    #[test]
    fn the_load_order_index_is_the_options_position_not_the_alphabet() {
        // `es5` is first and `dom` is well before the `es2015.*` sub-libs, which
        // alphabetical order would reverse.
        assert_eq!(lib_option_index("lib.es5.d.ts"), Some(0));
        let dom = lib_option_index("lib.dom.d.ts").expect("dom is selectable");
        let core = lib_option_index("lib.es2015.core.d.ts").expect("es2015.core is selectable");
        assert!(dom < core, "dom {dom} must load before es2015.core {core}");
        assert_eq!(lib_option_index("lib.d.ts"), None, "no option selects it");
    }

    #[test]
    fn a_file_named_by_two_options_takes_the_first() {
        // `es2019.symbol` and `esnext.symbol` both name `lib.es2019.symbol.d.ts`,
        // and upstream's `slices.Index` stops at the first.
        let index = lib_option_index("lib.es2019.symbol.d.ts").expect("selectable");
        assert_eq!(LIB_MAP_ENTRY(index).0, "es2019.symbol");
    }

    #[expect(non_snake_case, reason = "reads as the table it indexes")]
    fn LIB_MAP_ENTRY(index: usize) -> (&'static str, &'static str) {
        crate::LIB_MAP[index]
    }

    #[test]
    fn no_lib_loads_nothing_and_an_unset_lib_loads_the_target_default() {
        let default = CompilerOptions::default();
        assert_eq!(lib_file_names(&default), vec![default.default_lib_file_name()]);

        let target = CompilerOptions { target: ScriptTarget::ES2020, ..Default::default() };
        assert_eq!(lib_file_names(&target), vec!["lib.es2020.full.d.ts"]);

        let none = CompilerOptions { no_lib: Tristate::True, ..Default::default() };
        assert!(lib_file_names(&none).is_empty());
    }

    #[test]
    fn an_explicit_lib_list_replaces_the_default_and_keeps_its_own_order() {
        // The order given, not the table's: `--lib es2015,es5` loads es2015 first.
        // Upstream sorts lib files afterwards by `getDefaultLibFilePriority`, but
        // that is the *loader's* step and this list is the input to it.
        let options = CompilerOptions {
            lib: vec!["es2015".into(), "es5".into(), "nonsense".into()],
            ..Default::default()
        };
        assert_eq!(lib_file_names(&options), vec!["lib.es2015.d.ts", "lib.es5.d.ts"]);
    }

    #[test]
    fn no_lib_beats_an_explicit_lib_list() {
        let options = CompilerOptions {
            no_lib: Tristate::True,
            lib: vec!["es2015".into()],
            ..Default::default()
        };
        assert!(lib_file_names(&options).is_empty());
    }
}
