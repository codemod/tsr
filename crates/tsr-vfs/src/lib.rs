//! The file system the compiler probes.
//!
//! Ported from `internal/vfs` and `internal/vfs/vfstest` at the pinned commit —
//! the part module resolution needs, which is *only* the five questions below.
//!
//! # Why this is a trait and not `std::fs`
//!
//! Module resolution is defined as a sequence of probes: does this file exist,
//! does this directory exist, what does this symlink point at. Upstream's
//! `.trace.json` baselines record **every one of those probes, in order**, which
//! makes them a far stricter oracle than "did the specifier resolve" — a resolver
//! can reach the right answer by the wrong route and still fail the baseline.
//!
//! Judging against that oracle means running the corpus's declared files as a
//! file system, without touching the disk: a case declares `@filename` units and
//! `@symlink` links, and nothing else exists. So the resolver takes a
//! [`FileSystem`], and the conformance harness hands it an [`InMemoryFileSystem`]
//! built from the case.
//!
//! # What is deliberately missing
//!
//! Writing and watching. Nothing needs them yet: emit does not exist, so there
//! is nothing to write, and there is no watch mode to invalidate.
//!
//! The real-disk implementation is no longer missing — see [`os`], added when
//! the driver needed to compile something that was not a corpus case. The
//! in-memory host remains the conformance harness's, and the two implement the
//! same six questions.

pub mod cached;
pub mod glob;
pub mod os;

pub use cached::CachedFileSystem;
pub use os::OsFileSystem;

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;
use tsr_path::{
    ensure_trailing_directory_separator, get_canonical_file_name, get_directory_path,
    get_root_length, normalize_path, remove_trailing_directory_separator,
};

/// Decode a file's bytes into text, honouring a byte-order mark.
///
/// Ported from `decodeBytes` (`internal/vfs/internal/internal.go:170`) and
/// `decodeUtf16` (`:187`). A leading `FF FE` or `FE FF` means the file is
/// **UTF-16**, little- or big-endian; a leading `EF BB BF` is a UTF-8 mark and is
/// dropped; anything else is UTF-8 already.
///
/// # Why this is not `String::from_utf8_lossy`
///
/// Five corpus files are checked in as UTF-16 — `compiler/bom-utf16be`,
/// `bom-utf16le`, `unicodeIdentifierNames`, `promiseTest`,
/// `collisionCodeGenModuleWithUnicodeNames`. Read as UTF-8 they become a run of
/// replacement characters with a NUL between every letter, and the scanner
/// dutifully reports `TS1127 Invalid character` on nearly every position: 1,812
/// diagnostics upstream does not emit, a quarter of every false positive the
/// parser produced. They are not malformed files, they are correctly encoded files
/// in the other encoding TypeScript accepts.
///
/// Invalid sequences are still replaced rather than rejected, because
/// `compiler/corrupted` *is* deliberately malformed and refusing to read it would
/// drop it from the corpus — which is the opposite of what a conformance harness
/// should do.
#[must_use]
pub fn decode_bytes(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, u16::from_be_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Decode UTF-16 code units of a known endianness.
///
/// A trailing odd byte is dropped, as upstream's `binary.Read` into a
/// `[]uint16` of length `len(s)/2` also ignores it.
fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| unit([pair[0], pair[1]])).collect();
    // Upstream's `utf16.Decode` substitutes U+FFFD for an unpaired surrogate;
    // `decode_utf16_lossy` is not stable, so this is the same thing spelled out.
    char::decode_utf16(units).map(|result| result.unwrap_or('\u{FFFD}')).collect()
}

/// What lives directly inside a directory (`vfs.FS.GetAccessibleEntries`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectoryEntries {
    /// Immediate subdirectory names, sorted.
    pub directories: Vec<String>,
    /// Immediate file names, sorted.
    pub files: Vec<String>,
}

/// The file system as module resolution sees it (`vfs.FS`).
///
/// Every method takes a file *name* rather than a canonical `Path`: resolution
/// probes the names it constructs, and the case-folding rule is the file system's
/// own business.
pub trait FileSystem {
    /// Whether the host distinguishes `A.ts` from `a.ts`.
    fn use_case_sensitive_file_names(&self) -> bool;

    /// Whether a regular file exists at `path`, following symlinks.
    fn file_exists(&self, path: &str) -> bool;

    /// The contents of `path`, or `None` if it is not a readable file.
    fn read_file(&self, path: &str) -> Option<String>;

    /// Whether a directory exists at `path`, following symlinks.
    fn directory_exists(&self, path: &str) -> bool;

    /// What lives directly inside `path`. Empty if it is not a directory.
    fn get_accessible_entries(&self, path: &str) -> DirectoryEntries;

    /// `path` with every symlink along it resolved.
    ///
    /// A path that does not exist realpaths to itself, which is what upstream's
    /// `iovfs` does and what the `Resolving real path for '0', result '1'` trace
    /// line reports.
    fn realpath(&self, path: &str) -> String;
}

/// A file system assembled from a map of paths to contents.
///
/// Upstream's `vfstest.MapFS`. Directories are *implicit*: a directory exists
/// exactly when some file lives beneath it, which is how a test case that
/// declares only `/node_modules/foo/index.d.ts` still answers yes to
/// "does `/node_modules` exist".
#[derive(Debug, Default)]
pub struct InMemoryFileSystem {
    /// Canonical path to contents.
    files: FxHashMap<String, String>,
    /// Canonical link path to its (non-canonical) target.
    ///
    /// A `BTreeMap` so the "is this path *under* a symlinked directory" scan
    /// below is deterministic; with more than one candidate prefix the answer
    /// would otherwise depend on hash order.
    symlinks: BTreeMap<String, String>,
    /// Canonical directory path to the set of names directly inside it.
    directories: BTreeMap<String, DirectoryEntries>,
    use_case_sensitive_file_names: bool,
}

impl InMemoryFileSystem {
    /// Build a file system from `(path, contents)` pairs.
    ///
    /// Paths must be rooted; a relative one is a caller bug that would make every
    /// lookup miss silently, so it is normalised against `/` rather than kept.
    #[must_use]
    pub fn new(
        files: impl IntoIterator<Item = (String, String)>,
        symlinks: impl IntoIterator<Item = (String, String)>,
        use_case_sensitive_file_names: bool,
    ) -> Self {
        let mut fs = Self { use_case_sensitive_file_names, ..Self::default() };
        for (path, contents) in files {
            let canonical = fs.canonical(&path);
            fs.record_directories(&canonical, /* is_directory */ false);
            fs.files.insert(canonical, contents);
        }
        for (link, target) in symlinks {
            let canonical = fs.canonical(&link);
            fs.record_directories(&canonical, /* is_directory */ false);
            fs.symlinks.insert(canonical, normalize_path(&target));
        }
        fs
    }

    fn canonical(&self, path: &str) -> String {
        let normalized = normalize_path(path);
        // A trailing separator is not part of a directory's identity — except
        // the root's, where it *is* the identity. `tspath`'s
        // `remove_trailing_directory_separator` is upstream-faithful and turns
        // `/` into the empty string, which is right for glob compilation and
        // wrong for a map key, so the root is guarded here rather than there.
        let key = if normalized.len() > get_root_length(&normalized) {
            remove_trailing_directory_separator(&normalized)
        } else {
            normalized
        };
        get_canonical_file_name(&key, self.use_case_sensitive_file_names)
    }

    /// Record every ancestor directory of `path`, and `path`'s own entry in its
    /// parent.
    fn record_directories(&mut self, canonical: &str, is_directory: bool) {
        let mut child = canonical.to_string();
        let mut child_is_directory = is_directory;
        loop {
            let parent = get_directory_path(&child).to_string();
            if parent == child {
                break;
            }
            let name = child[parent.len()..].trim_start_matches('/').to_string();
            let entry = self.directories.entry(parent.clone()).or_default();
            let bucket = if child_is_directory { &mut entry.directories } else { &mut entry.files };
            if let Err(index) = bucket.binary_search(&name) {
                bucket.insert(index, name);
            }
            child = parent;
            child_is_directory = true;
        }
    }

    /// Follow symlinks to the real canonical path, or `None` if nothing is there.
    ///
    /// Two ways a path can be a link: it is one itself, or it lives beneath a
    /// directory that is. The second is what makes `@symlink` on a directory
    /// work, and it is why this cannot be a single map lookup.
    fn resolve(&self, canonical: &str) -> Option<String> {
        self.resolve_worker(canonical, 0)
    }

    fn resolve_worker(&self, canonical: &str, depth: usize) -> Option<String> {
        // A symlink cycle is a malformed test case, not a compiler condition;
        // bounding the depth turns a hang into a miss.
        if depth > 32 {
            return None;
        }
        if self.files.contains_key(canonical) || self.directories.contains_key(canonical) {
            return Some(canonical.to_string());
        }
        if let Some(target) = self.symlinks.get(canonical) {
            return self.resolve_worker(&self.canonical(target), depth + 1);
        }
        for (link, target) in &self.symlinks {
            let prefix = ensure_trailing_directory_separator(link);
            if let Some(rest) = canonical.strip_prefix(&prefix) {
                let redirected = format!("{}/{rest}", target.trim_end_matches('/'));
                return self.resolve_worker(&self.canonical(&redirected), depth + 1);
            }
        }
        None
    }
}

impl FileSystem for InMemoryFileSystem {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.use_case_sensitive_file_names
    }

    fn file_exists(&self, path: &str) -> bool {
        let canonical = self.canonical(path);
        self.resolve(&canonical).is_some_and(|real| self.files.contains_key(&real))
    }

    fn read_file(&self, path: &str) -> Option<String> {
        let canonical = self.canonical(path);
        let real = self.resolve(&canonical)?;
        self.files.get(&real).cloned()
    }

    fn directory_exists(&self, path: &str) -> bool {
        let canonical = self.canonical(path);
        self.resolve(&canonical).is_some_and(|real| self.directories.contains_key(&real))
    }

    fn get_accessible_entries(&self, path: &str) -> DirectoryEntries {
        let canonical = self.canonical(path);
        self.resolve(&canonical)
            .and_then(|real| self.directories.get(&real).cloned())
            .unwrap_or_default()
    }

    fn realpath(&self, path: &str) -> String {
        let canonical = self.canonical(path);
        // Realpath preserves the *spelling* when nothing is symlinked, so a
        // case-insensitive host does not rewrite `/A/B.ts` to `/a/b.ts` — upstream
        // compares the two and prefers the original when they differ only by case.
        match self.resolve(&canonical) {
            Some(real) if real != canonical => real,
            _ => normalize_path(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs(files: &[(&str, &str)], symlinks: &[(&str, &str)]) -> InMemoryFileSystem {
        InMemoryFileSystem::new(
            files.iter().map(|(p, c)| ((*p).to_string(), (*c).to_string())),
            symlinks.iter().map(|(l, t)| ((*l).to_string(), (*t).to_string())),
            true,
        )
    }

    #[test]
    fn directories_are_implied_by_the_files_in_them() {
        // No test case declares its directories; they exist because files do.
        let fs = fs(&[("/a/b/c.ts", "")], &[]);
        assert!(fs.file_exists("/a/b/c.ts"));
        assert!(fs.directory_exists("/a/b"));
        assert!(fs.directory_exists("/a"));
        assert!(fs.directory_exists("/"));
        assert!(!fs.directory_exists("/a/b/c.ts"));
        assert!(!fs.file_exists("/a/b"));
    }

    #[test]
    fn entries_list_immediate_children_only() {
        let fs = fs(&[("/a/b.ts", ""), ("/a/c/d.ts", ""), ("/a/e.ts", "")], &[]);
        let entries = fs.get_accessible_entries("/a");
        assert_eq!(entries.files, ["b.ts", "e.ts"]);
        assert_eq!(entries.directories, ["c"]);
    }

    #[test]
    fn a_symlinked_directory_redirects_everything_beneath_it() {
        // The `@symlink` shape the corpus uses for duplicate-package tests: a
        // whole package directory aliased, not one file.
        let fs = fs(&[("/real/pkg/index.d.ts", "x")], &[("/link", "/real/pkg")]);
        assert!(fs.file_exists("/link/index.d.ts"));
        assert_eq!(fs.read_file("/link/index.d.ts").as_deref(), Some("x"));
        assert_eq!(fs.realpath("/link/index.d.ts"), "/real/pkg/index.d.ts");
        assert!(fs.directory_exists("/link"));
    }

    #[test]
    fn realpath_of_something_unlinked_is_itself() {
        // Which is what makes the `Resolving real path` trace line print the same
        // path twice for the overwhelmingly common case.
        let fs = fs(&[("/a/b.ts", "")], &[]);
        assert_eq!(fs.realpath("/a/b.ts"), "/a/b.ts");
        assert_eq!(fs.realpath("/nothing/here.ts"), "/nothing/here.ts");
    }

    #[test]
    fn a_broken_link_does_not_exist() {
        let fs = fs(&[], &[("/link", "/nowhere")]);
        assert!(!fs.file_exists("/link"));
        assert!(!fs.directory_exists("/link"));
    }

    #[test]
    fn a_symlink_cycle_terminates() {
        let fs = fs(&[], &[("/a", "/b"), ("/b", "/a")]);
        assert!(!fs.file_exists("/a"));
    }

    #[test]
    fn a_byte_order_mark_selects_the_encoding() {
        // `var a = 1;` as the corpus checks it in, three ways.
        let little: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("var a = 1;".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let big: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("var a = 1;".encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode_bytes(&little), "var a = 1;");
        assert_eq!(decode_bytes(&big), "var a = 1;");
        assert_eq!(decode_bytes(b"\xEF\xBB\xBFvar a = 1;"), "var a = 1;");
        assert_eq!(decode_bytes(b"var a = 1;"), "var a = 1;");
    }

    #[test]
    fn decoding_does_not_reject_a_malformed_file() {
        // `compiler/corrupted` is deliberately broken and must still be read, so
        // that it stays in the denominator rather than vanishing from it.
        assert_eq!(decode_bytes(b"a\xFFb"), "a\u{FFFD}b");
        // An unpaired surrogate: U+FFFD, as upstream's `utf16.Decode` gives.
        let lone_surrogate = [0xFF, 0xFE, 0x00, 0xD8];
        assert_eq!(decode_bytes(&lone_surrogate), "\u{FFFD}");
        // An odd trailing byte is dropped, not an error.
        let odd = [0xFF, 0xFE, 0x76, 0x00, 0x61];
        assert_eq!(decode_bytes(&odd), "v");
        // Two bytes that are only a mark decode to nothing.
        assert_eq!(decode_bytes(b"\xFF\xFE"), "");
        assert_eq!(decode_bytes(b""), "");
    }

    #[test]
    fn case_insensitivity_is_a_property_of_the_host() {
        let sensitive = fs(&[("/A/B.ts", "")], &[]);
        assert!(sensitive.file_exists("/A/B.ts"));
        assert!(!sensitive.file_exists("/a/b.ts"));

        let insensitive = InMemoryFileSystem::new(
            [("/A/B.ts".to_string(), String::new())],
            [],
            /* use_case_sensitive_file_names */ false,
        );
        assert!(insensitive.file_exists("/a/b.ts"));
    }
}
