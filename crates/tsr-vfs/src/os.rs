//! The real disk, as the compiler probes it.
//!
//! Ported from `internal/vfs/osvfs/os.go` and the shared implementation it
//! delegates to, `internal/vfs/internal/internal.go` (`Common`), at the pinned
//! commit. Upstream splits those two because `iovfs` reuses `Common` over an
//! arbitrary `fs.FS`; this port has one real-disk consumer and no second one in
//! sight, so the two collapse into a single type. If a second backing store ever
//! appears — a tarball, a zip, an editor's in-memory overlay — the split is the
//! obvious place to reintroduce.
//!
//! # What is deliberately missing
//!
//! Writing, `Stat`, `WalkDir`, `Remove`, `Chtimes`. [`FileSystem`] does not
//! declare them, because module resolution does not ask them, and this file
//! implements exactly that trait. Emit needs `WriteFile` and will widen the
//! trait when it exists; adding it now would be an unused method whose semantics
//! nothing checks.
//!
//! # The semaphores are not ported
//!
//! Upstream wraps every syscall in one of three counting semaphores — 128 for
//! blocking operations, 128 for reads, 32 for writes
//! (`osvfs/os.go:24-31`) — because `processAllProgramFiles` fans out across
//! goroutines and an unbounded fan-out exhausts the file-descriptor limit. This
//! port's loader is sequential (see [`tsr_compiler::Program::bind_source_files`]
//! for the same trade recorded on the binder side), so there is nothing yet to
//! bound. The semaphores belong with the parallel loader, not ahead of it: a
//! limiter around a single-threaded caller is a lock nobody contends.

use std::path::{Path as StdPath, PathBuf};
use std::sync::OnceLock;

use tsr_path::{get_root_length, normalize_slashes};

use crate::{DirectoryEntries, FileSystem, decode_bytes};

/// The file system backed by the real disk (`osvfs.FS()`).
///
/// Stateless — every method is a syscall — so a single value can be shared. The
/// one piece of cached state, whether the disk is case-sensitive, is a process
/// property rather than an instance one and lives in a `OnceLock` accordingly,
/// matching upstream's package-level `isFileSystemCaseSensitive`.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsFileSystem;

impl OsFileSystem {
    /// A handle to the real disk.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Panic unless `path` is absolute, as every `Common` method does.
///
/// Upstream reaches this through `RootAndPath` → `SplitPath` → `RootLength`
/// (`internal/vfs/internal/internal.go:22-27`), which panics on a path with no
/// root. It is load-bearing rather than defensive: module resolution constructs
/// candidate paths by combining a directory with a specifier, and a bug that
/// leaves one relative would otherwise surface as "that file does not exist" —
/// a resolution failure indistinguishable from a genuine miss, several thousand
/// probes away from its cause.
fn assert_rooted(path: &str) {
    assert!(get_root_length(path) != 0, "vfs: path {path:?} is not absolute");
}

/// Whether the disk distinguishes `A.ts` from `a.ts`.
///
/// Ported from `isFileSystemCaseSensitive` (`osvfs/os.go:46-73`), including its
/// admitted imprecision: the answer is a property of a *volume*, not of a
/// machine, and this probes exactly one path. Upstream accepts that and notes it
/// ("this is not entirely correct, since different OSs can have differing case
/// sensitivity in different paths, but this is largely good enough for our
/// purposes"). A case-folding macOS volume mounted next to a case-sensitive one
/// answers for both.
///
/// # Why probing rather than asking the platform
///
/// There is no portable syscall for it. Upstream's proxy — swap the case of the
/// running executable's own path and ask whether that still names a file — is
/// the same trick `sys.ts` played with `__filename`, and it has the property
/// that matters here: it measures the volume the compiler is actually installed
/// on rather than a compile-time guess.
///
/// # Why a failure panics
///
/// Upstream panics on both `os.Executable()` failing and `os.Stat` returning an
/// error other than not-exist. Kept, rather than softened to a default, because
/// every default is wrong somewhere: answering `true` on a case-folding disk
/// makes two spellings of one file into two program entries, and answering
/// `false` on a case-sensitive one merges files that are genuinely distinct.
/// Both are silent, and both surface as module resolution failures far from the
/// guess. A process that cannot determine this cannot compile correctly.
fn is_file_system_case_sensitive() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        // Upstream returns false for Windows unconditionally and does not probe.
        if cfg!(windows) {
            return false;
        }

        let exe = std::env::current_exe()
            .unwrap_or_else(|error| panic!("vfs: failed to get executable path: {error}"));
        let swapped = swap_case(&exe);

        match std::fs::metadata(&swapped) {
            // The executable is also reachable under the opposite casing, so the
            // disk folds case.
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                panic!("vfs: failed to stat {}: {error}", swapped.display())
            }
        }
    })
}

/// Lowercase every uppercase character and vice versa (`swapCase`).
///
/// Operates on the `OsString` rather than on a `&str` so a path that is not
/// valid UTF-8 still round-trips: on Unix a path is bytes, and a non-UTF-8
/// executable path is legal. `to_string_lossy` would replace those bytes and
/// produce a path that does not name the file even on a case-folding disk,
/// which would report case-sensitive on every such system.
fn swap_case(path: &StdPath) -> PathBuf {
    let Some(text) = path.to_str() else {
        // Not representable as UTF-8, so there is no case to swap that we can
        // reason about; hand back the original, which exists, so the probe
        // reports case-insensitive. Upstream cannot reach this branch because Go
        // strings are bytes and `strings.Map` passes invalid runes through.
        return path.to_path_buf();
    };
    let swapped: String = text
        .chars()
        .map(|character| {
            // Upstream: uppercase it, and if that changed nothing, lowercase it.
            // Not `is_uppercase`, because a character with no case must pass
            // through unchanged and both conversions are identity for it.
            let mut upper = character.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) if single != character => single,
                // Either the character is already uppercase (or caseless), or it
                // uppercases to several characters — 'ß' → "SS". Go's
                // `unicode.ToUpper` is rune-to-rune and cannot express the
                // multi-character expansion, so it leaves such a character
                // alone and then lowercases it. Same outcome here.
                _ => {
                    let mut lower = character.to_lowercase();
                    match (lower.next(), lower.next()) {
                        (Some(single), None) => single,
                        _ => character,
                    }
                }
            }
        })
        .collect();
    PathBuf::from(swapped)
}

impl FileSystem for OsFileSystem {
    fn use_case_sensitive_file_names(&self) -> bool {
        is_file_system_case_sensitive()
    }

    /// `Common.FileExists` — stat, and accept anything that is not a directory.
    ///
    /// Deliberately *not* `is_file()`. Upstream asks `stat != nil &&
    /// !stat.IsDir()`, which admits a FIFO, a device node, or a socket, whereas
    /// [`Self::get_accessible_entries`] admits only `mode.IsRegular()`. The two
    /// disagreeing is upstream's behaviour, not an oversight to correct here:
    /// resolution probing a named pipe by exact path finds it, while a directory
    /// listing does not offer it as a candidate.
    fn file_exists(&self, path: &str) -> bool {
        assert_rooted(path);
        std::fs::metadata(path).is_ok_and(|metadata| !metadata.is_dir())
    }

    /// `Common.ReadFile` — bytes off the disk, decoded by byte-order mark.
    ///
    /// Any error is `None`, matching upstream's `ok bool`: a missing file, a
    /// directory, and a permission denial are one answer, because resolution
    /// only ever asks whether it got text.
    fn read_file(&self, path: &str) -> Option<String> {
        assert_rooted(path);
        std::fs::read(path).ok().map(|bytes| decode_bytes(&bytes))
    }

    /// `Common.DirectoryExists` — stat, and require a directory.
    fn directory_exists(&self, path: &str) -> bool {
        assert_rooted(path);
        std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
    }

    /// `Common.GetAccessibleEntries` — one level, with symlinks followed.
    ///
    /// The classification is by the entry's own type first (`entry.Type()`,
    /// which does *not* follow links) and only then by the link target's, which
    /// is what makes a symlink to a directory list as a directory while a broken
    /// symlink lists as neither.
    ///
    /// # Sorting is this port's job
    ///
    /// Upstream gets it free: `fs.ReadDir` is specified to return entries
    /// sorted by filename. `std::fs::read_dir` yields them in whatever order the
    /// directory happens to hold, so the sort is explicit here. It is not
    /// cosmetic — [`DirectoryEntries`] is documented as sorted, `include`/
    /// `exclude` globbing walks it, and an unsorted walk makes program file
    /// order depend on the inode layout of the checkout.
    fn get_accessible_entries(&self, path: &str) -> DirectoryEntries {
        assert_rooted(path);
        let Ok(entries) = std::fs::read_dir(path) else {
            return DirectoryEntries::default();
        };

        let mut result = DirectoryEntries::default();
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                // A name that is not valid UTF-8 cannot be a module specifier or
                // a file name in a program, and every path in this port is a
                // `str`. Upstream, whose names are bytes, would carry it along
                // and fail to resolve it later.
                continue;
            };

            if kind.is_dir() {
                result.directories.push(name);
            } else if kind.is_file() {
                result.files.push(name);
            } else if kind.is_symlink() {
                // Follow it and classify by the target, as upstream's
                // `addToResult(entry.Name(), stat.Mode(), true)` does. A broken
                // link stats as an error and is dropped, which is why it appears
                // in neither list.
                //
                // Upstream also records the name in `Entries.Symlinks`; this
                // port's `DirectoryEntries` has no such field, because the only
                // consumer of it upstream is the watcher, which is not ported.
                if let Ok(metadata) = std::fs::metadata(entry.path()) {
                    if metadata.is_dir() {
                        result.directories.push(name);
                    } else if metadata.is_file() {
                        result.files.push(name);
                    }
                }
            }
            // Anything else — a device, a socket, a Windows reparse point that
            // is not a symlink — is dropped, as upstream's `addToResult`
            // returning false drops it. Upstream has one more arm here,
            // `ModeIrregular` plus an OS-specific `IsReparsePoint` probe for
            // Windows junctions; `std::fs` reports a junction as a symlink
            // already, so the arm above covers it.
        }

        result.directories.sort_unstable();
        result.files.sort_unstable();
        result
    }

    /// `osFSRealpath` — resolve symlinks and correct casing, or give up.
    ///
    /// Upstream is `nativepath.Realpath` then `filepath.Abs` then
    /// `NormalizeSlashes`, returning the **original spelling untouched** if
    /// either step errors. [`std::fs::canonicalize`] is the same syscall
    /// (`realpath(3)`) and already absolutises, so the two steps collapse.
    ///
    /// Returning the input on failure is what makes a path that does not exist
    /// realpath to itself — relied upon by the `Resolving real path for '0',
    /// result '1'` trace line, which prints the same path twice for every probe
    /// that missed.
    fn realpath(&self, path: &str) -> String {
        assert_rooted(path);
        let Ok(resolved) = std::fs::canonicalize(path) else {
            return path.to_string();
        };
        let Some(text) = resolved.to_str() else {
            return path.to_string();
        };
        normalize_slashes(strip_verbatim_prefix(text))
    }
}

/// Drop Windows' extended-length prefix from a canonicalised path.
///
/// A platform shim with no upstream counterpart, and it exists because the two
/// languages' `realpath` differ: Go's `filepath.EvalSymlinks` returns a plain
/// `C:\foo`, while [`std::fs::canonicalize`] returns the verbatim form
/// `\\?\C:\foo`. Leaving it on would make every realpathed path unequal to the
/// same path spelled normally, and `Path` comparison is how the program decides
/// two files are one file.
///
/// Not `dunce` or an equivalent crate: the whole rule is the two lines below,
/// and this port takes dependencies for algorithms, not for string prefixes.
fn strip_verbatim_prefix(path: &str) -> &str {
    // `\\?\UNC\server\share` denotes a network path whose plain form is
    // `\\server\share`, so the prefix cannot simply be cut; leave it alone
    // rather than produce a path that names something else.
    match path.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest,
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every test here touches the real disk, which is the point of the type,
    /// so each one builds its own tree under the process's temp directory.
    ///
    /// Deliberately not a `tempfile` dependency: the workspace does not have one
    /// and this is the only consumer. The directory name carries the test's own
    /// name plus the process id, so a parallel `cargo test` and a crashed
    /// earlier run cannot collide.
    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("tsr-vfs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("creating the test tree");
            // Canonicalised because macOS puts the temp directory behind
            // `/var` → `/private/var`, and a test that asserts on `realpath`
            // output would otherwise compare against the symlinked spelling.
            let root = std::fs::canonicalize(&root).expect("canonicalising the test tree");
            Self { root }
        }

        fn path(&self, relative: &str) -> String {
            normalize_slashes(self.root.join(relative).to_str().expect("utf-8 temp path"))
        }

        fn file(&self, relative: &str, contents: &[u8]) -> String {
            let full = self.root.join(relative);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("creating a parent directory");
            }
            std::fs::write(&full, contents).expect("writing a test file");
            self.path(relative)
        }

        fn dir(&self, relative: &str) -> String {
            std::fs::create_dir_all(self.root.join(relative)).expect("creating a directory");
            self.path(relative)
        }

        fn root(&self) -> String {
            normalize_slashes(self.root.to_str().expect("utf-8 temp path"))
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_file_is_read_and_a_missing_one_is_none() {
        let tree = TempTree::new("read");
        let file = tree.file("a.ts", b"export const a = 1;");
        let fs = OsFileSystem::new();

        assert_eq!(fs.read_file(&file).as_deref(), Some("export const a = 1;"));
        assert_eq!(fs.read_file(&tree.path("missing.ts")), None);
        // A directory is not text, and upstream answers the same `ok=false` for
        // it as for a file that is not there.
        assert_eq!(fs.read_file(&tree.root()), None);
    }

    #[test]
    fn a_byte_order_mark_is_honoured_off_the_disk() {
        // The five UTF-16 corpus files reach the compiler through this path once
        // a real host exists, so the decode has to happen here and not only in
        // the in-memory host.
        let tree = TempTree::new("bom");
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "var a = 1;".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let little = tree.file("le.ts", &utf16);
        let utf8_bom = tree.file("bom.ts", b"\xEF\xBB\xBFvar a = 1;");

        let fs = OsFileSystem::new();
        assert_eq!(fs.read_file(&little).as_deref(), Some("var a = 1;"));
        assert_eq!(fs.read_file(&utf8_bom).as_deref(), Some("var a = 1;"));
    }

    #[test]
    fn existence_distinguishes_files_from_directories() {
        let tree = TempTree::new("exists");
        let file = tree.file("a.ts", b"");
        let dir = tree.dir("sub");
        let fs = OsFileSystem::new();

        assert!(fs.file_exists(&file));
        assert!(!fs.file_exists(&dir));
        assert!(fs.directory_exists(&dir));
        assert!(!fs.directory_exists(&file));
        assert!(!fs.file_exists(&tree.path("nothing")));
        assert!(!fs.directory_exists(&tree.path("nothing")));
    }

    #[test]
    fn entries_are_split_by_kind_and_sorted() {
        let tree = TempTree::new("entries");
        tree.file("b.ts", b"");
        tree.file("a.ts", b"");
        tree.dir("z");
        tree.dir("m");
        let fs = OsFileSystem::new();

        let entries = fs.get_accessible_entries(&tree.root());
        // Sorted, which `std::fs::read_dir` does not do for us.
        assert_eq!(entries.files, ["a.ts", "b.ts"]);
        assert_eq!(entries.directories, ["m", "z"]);
    }

    #[test]
    fn entries_of_a_non_directory_are_empty() {
        let tree = TempTree::new("entries-missing");
        let file = tree.file("a.ts", b"");
        let fs = OsFileSystem::new();

        assert_eq!(fs.get_accessible_entries(&tree.path("nothing")), DirectoryEntries::default());
        assert_eq!(fs.get_accessible_entries(&file), DirectoryEntries::default());
    }

    #[test]
    fn realpath_of_something_unlinked_is_itself() {
        let tree = TempTree::new("realpath");
        let file = tree.file("a.ts", b"");
        let fs = OsFileSystem::new();

        assert_eq!(fs.realpath(&file), file);
        // The missing-path case, which every failed resolution probe hits.
        let missing = tree.path("nothing/here.ts");
        assert_eq!(fs.realpath(&missing), missing);
    }

    #[test]
    #[should_panic(expected = "is not absolute")]
    fn a_relative_path_is_a_bug_and_says_so() {
        OsFileSystem::new().file_exists("a.ts");
    }

    #[cfg(unix)]
    mod unix {
        use super::*;

        #[test]
        fn a_symlinked_directory_is_followed() {
            let tree = TempTree::new("symlink");
            tree.file("real/pkg/index.d.ts", b"declare const x: number;");
            std::os::unix::fs::symlink(tree.root.join("real/pkg"), tree.root.join("link"))
                .expect("creating a symlink");
            let fs = OsFileSystem::new();

            assert!(fs.directory_exists(&tree.path("link")));
            assert!(fs.file_exists(&tree.path("link/index.d.ts")));
            assert_eq!(
                fs.realpath(&tree.path("link/index.d.ts")),
                tree.path("real/pkg/index.d.ts")
            );

            // The link classifies as a directory in its parent's listing,
            // because the target is stat'd rather than the link itself.
            let entries = fs.get_accessible_entries(&tree.root());
            assert_eq!(entries.directories, ["link", "real"]);
            assert!(entries.files.is_empty());
        }

        #[test]
        fn a_broken_symlink_appears_in_neither_list() {
            let tree = TempTree::new("broken");
            std::os::unix::fs::symlink(tree.root.join("nowhere"), tree.root.join("link"))
                .expect("creating a symlink");
            let fs = OsFileSystem::new();

            let entries = fs.get_accessible_entries(&tree.root());
            assert!(entries.files.is_empty());
            assert!(entries.directories.is_empty());
            assert!(!fs.file_exists(&tree.path("link")));
            assert!(!fs.directory_exists(&tree.path("link")));
        }
    }

    #[test]
    fn swapping_case_is_reversible_for_simple_ascii() {
        assert_eq!(swap_case(StdPath::new("/Usr/Local/bin")), PathBuf::from("/uSR/lOCAL/BIN"));
        // A caseless character passes through both conversions unchanged.
        assert_eq!(swap_case(StdPath::new("/a/1-2_3")), PathBuf::from("/A/1-2_3"));
    }

    #[test]
    fn a_multi_character_uppercasing_is_left_alone() {
        // 'ß' uppercases to "SS", which Go's rune-to-rune `unicode.ToUpper`
        // cannot represent, so it lowercases instead — identity here.
        assert_eq!(swap_case(StdPath::new("/ß")), PathBuf::from("/ß"));
    }

    #[test]
    fn the_verbatim_prefix_is_dropped_but_a_unc_path_is_not() {
        assert_eq!(strip_verbatim_prefix(r"\\?\C:\foo"), r"C:\foo");
        assert_eq!(strip_verbatim_prefix(r"\\?\UNC\server\share"), r"\\?\UNC\server\share");
        assert_eq!(strip_verbatim_prefix("/a/b"), "/a/b");
    }
}
