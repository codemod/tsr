//! The default libraries, compiled into the binary.
//!
//! Ported from `internal/bundled/embed.go` at the pinned commit (the default
//! `!noembed` build): `libPath` (`:21`) names the library directory
//! `bundled:///libs`, and `wrappedFS` (`:35`) answers every query under the
//! `bundled:///` scheme from the embedded table, forwarding everything else
//! to the wrapped file system. The table is `embed_generated.go`'s, produced
//! by this crate's `build.rs` from the pinned submodule.
//!
//! Embedding is what keeps library loading free of I/O: a program over
//! `lib.dom.d.ts` and the ES libraries opens no file for them, and
//! [`FileSystem::read_static`] lends the loader the embedded text itself, so
//! the 2–4 MB of library source is neither read nor copied per compilation.

use crate::{DirectoryEntries, FileSystem};

include!(concat!(env!("OUT_DIR"), "/bundled_libs.rs"));

/// The scheme every embedded path starts with (`embed.go:15`).
const SCHEME: &str = "bundled:///";

/// The directory the default libraries live in (`bundled.LibPath`).
pub const LIB_PATH: &str = "bundled:///libs";

/// `splitPath`: the part of `path` after the scheme, if it has it.
fn split_path(path: &str) -> Option<&str> {
    path.strip_prefix(SCHEME)
}

/// `embeddedContents[rest]`.
fn embedded(rest: &str) -> Option<&'static str> {
    let name = rest.strip_prefix("libs/")?;
    EMBEDDED_LIBS
        .binary_search_by(|(candidate, _)| (*candidate).cmp(name))
        .ok()
        .map(|index| EMBEDDED_LIBS[index].1)
}

/// `bundled.IsBundled`.
#[must_use]
pub fn is_bundled(path: &str) -> bool {
    split_path(path).is_some()
}

/// A file system with the embedded libraries mounted at [`LIB_PATH`]
/// (`bundled.WrapFS`).
#[derive(Debug)]
pub struct BundledFileSystem<F> {
    fs: F,
}

impl<F: FileSystem> BundledFileSystem<F> {
    /// Mount the embedded libraries over `fs`.
    #[must_use]
    pub const fn new(fs: F) -> Self {
        Self { fs }
    }
}

impl<F: FileSystem> FileSystem for BundledFileSystem<F> {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.fs.use_case_sensitive_file_names()
    }

    fn file_exists(&self, path: &str) -> bool {
        match split_path(path) {
            Some(rest) => embedded(rest).is_some(),
            None => self.fs.file_exists(path),
        }
    }

    fn read_file(&self, path: &str) -> Option<String> {
        match split_path(path) {
            Some(rest) => embedded(rest).map(str::to_owned),
            None => self.fs.read_file(path),
        }
    }

    fn read_static(&self, path: &str) -> Option<&'static str> {
        match split_path(path) {
            Some(rest) => embedded(rest),
            None => self.fs.read_static(path),
        }
    }

    fn read_files(&self, paths: &[&str]) -> Vec<Option<String>> {
        if paths.iter().any(|path| is_bundled(path)) {
            return paths.iter().map(|path| self.read_file(path)).collect();
        }
        self.fs.read_files(paths)
    }

    fn directory_exists(&self, path: &str) -> bool {
        match split_path(path) {
            Some(rest) => rest == "libs",
            None => self.fs.directory_exists(path),
        }
    }

    fn get_accessible_entries(&self, path: &str) -> DirectoryEntries {
        match split_path(path) {
            Some("") => {
                DirectoryEntries { directories: vec!["libs".to_string()], files: Vec::new() }
            }
            Some("libs") => DirectoryEntries {
                directories: Vec::new(),
                files: EMBEDDED_LIBS.iter().map(|(name, _)| (*name).to_string()).collect(),
            },
            Some(_) => DirectoryEntries::default(),
            None => self.fs.get_accessible_entries(path),
        }
    }

    fn realpath(&self, path: &str) -> String {
        if is_bundled(path) { path.to_string() } else { self.fs.realpath(path) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryFileSystem;

    fn mounted() -> BundledFileSystem<InMemoryFileSystem> {
        BundledFileSystem::new(InMemoryFileSystem::new(
            [("/disk/lib.d.ts".to_string(), "disk".to_string())],
            [],
            true,
        ))
    }

    #[test]
    fn embedded_libraries_answer_only_under_the_scheme() {
        let fs = mounted();
        let es5 = fs.read_static("bundled:///libs/lib.es5.d.ts").expect("embedded es5");
        assert!(es5.contains("interface Array<T>"));
        assert_eq!(fs.read_file("bundled:///libs/lib.es5.d.ts").as_deref(), Some(es5));
        assert!(fs.file_exists("bundled:///libs/lib.dom.d.ts"));
        assert!(!fs.file_exists("bundled:///libs/lib.missing.d.ts"));
        assert!(!fs.file_exists("bundled:///lib.d.ts"));
        assert_eq!(fs.read_file("/disk/lib.d.ts").as_deref(), Some("disk"));
        assert_eq!(fs.read_static("/disk/lib.d.ts"), None);
    }

    #[test]
    fn the_mount_lists_every_library_sorted() {
        let fs = mounted();
        assert!(fs.directory_exists("bundled:///libs"));
        assert!(!fs.directory_exists("bundled:///libs/lib.d.ts"));
        assert_eq!(fs.get_accessible_entries("bundled:///").directories, ["libs"]);
        let files = fs.get_accessible_entries("bundled:///libs").files;
        assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(files.iter().all(|name| fs.file_exists(&format!("{LIB_PATH}/{name}"))));
        assert_eq!(fs.realpath("bundled:///libs/lib.d.ts"), "bundled:///libs/lib.d.ts");
    }
}
