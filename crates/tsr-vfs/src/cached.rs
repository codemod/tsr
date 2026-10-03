//! Project-local filesystem probe reuse (`internal/vfs/cachedvfs`).

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashMap;

use crate::{DirectoryEntries, FileSystem};

/// Native `cachedvfs.FS`, restricted to the operations this port exposes.
///
/// Keys retain the caller's exact spelling. Each operation has its own cache,
/// including negative results; file contents always come from the backing host.
/// Create one per compilation snapshot, never retain it across filesystem edits.
/// Like the current loader, this wrapper is sequential. Parallel loading needs
/// synchronized caches and a shareable backing host, as native uses `SyncMap`.
pub struct CachedFileSystem<'fs> {
    fs: &'fs dyn FileSystem,
    enabled: Cell<bool>,
    directories: RefCell<FxHashMap<String, bool>>,
    files: RefCell<FxHashMap<String, bool>>,
    entries: RefCell<FxHashMap<String, DirectoryEntries>>,
    realpaths: RefCell<FxHashMap<String, String>>,
}

impl<'fs> CachedFileSystem<'fs> {
    /// Wrap a backing host with empty, enabled caches (`cachedvfs.From`).
    #[must_use]
    pub fn new(fs: &'fs dyn FileSystem) -> Self {
        Self {
            fs,
            enabled: Cell::new(true),
            directories: RefCell::default(),
            files: RefCell::default(),
            entries: RefCell::default(),
            realpaths: RefCell::default(),
        }
    }

    /// Discard probe results without changing whether caching is enabled.
    pub fn clear_cache(&self) {
        self.directories.borrow_mut().clear();
        self.files.borrow_mut().clear();
        self.entries.borrow_mut().clear();
        self.realpaths.borrow_mut().clear();
    }

    /// Bypass cache reads and writes until [`Self::enable`] (`DisableAndClearCache`).
    pub fn disable_and_clear_cache(&self) {
        if self.enabled.replace(false) {
            self.clear_cache();
        }
    }

    /// Resume caching; disabled queries did not populate it (`Enable`).
    pub fn enable(&self) {
        self.enabled.set(true);
    }

    fn cached<T: Clone>(
        &self,
        cache: &RefCell<FxHashMap<String, T>>,
        path: &str,
        load: impl FnOnce() -> T,
    ) -> T {
        if self.enabled.get()
            && let Some(value) = cache.borrow().get(path)
        {
            return value.clone();
        }
        let value = load();
        if self.enabled.get() {
            cache.borrow_mut().insert(path.into(), value.clone());
        }
        value
    }
}

impl FileSystem for CachedFileSystem<'_> {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.fs.use_case_sensitive_file_names()
    }

    fn file_exists(&self, path: &str) -> bool {
        self.cached(&self.files, path, || self.fs.file_exists(path))
    }

    fn read_file(&self, path: &str) -> Option<String> {
        self.fs.read_file(path)
    }

    fn directory_exists(&self, path: &str) -> bool {
        self.cached(&self.directories, path, || self.fs.directory_exists(path))
    }

    fn get_accessible_entries(&self, path: &str) -> DirectoryEntries {
        self.cached(&self.entries, path, || self.fs.get_accessible_entries(path))
    }

    fn realpath(&self, path: &str) -> String {
        self.cached(&self.realpaths, path, || self.fs.realpath(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryFileSystem;

    struct Host {
        snapshot: RefCell<InMemoryFileSystem>,
        calls: RefCell<FxHashMap<&'static str, usize>>,
    }

    impl Host {
        fn record(&self, operation: &'static str) {
            *self.calls.borrow_mut().entry(operation).or_default() += 1;
        }

        fn count(&self, operation: &str) -> usize {
            self.calls.borrow().get(operation).copied().unwrap_or(0)
        }

        fn replace(&self, files: &[(&str, &str)]) {
            self.snapshot.replace(InMemoryFileSystem::new(
                files.iter().map(|(path, text)| (path.to_string(), text.to_string())),
                [],
                true,
            ));
        }
    }

    impl FileSystem for Host {
        fn use_case_sensitive_file_names(&self) -> bool {
            self.snapshot.borrow().use_case_sensitive_file_names()
        }
        fn file_exists(&self, path: &str) -> bool {
            self.record("file");
            self.snapshot.borrow().file_exists(path)
        }
        fn read_file(&self, path: &str) -> Option<String> {
            self.record("read");
            self.snapshot.borrow().read_file(path)
        }
        fn directory_exists(&self, path: &str) -> bool {
            self.record("directory");
            self.snapshot.borrow().directory_exists(path)
        }
        fn get_accessible_entries(&self, path: &str) -> DirectoryEntries {
            self.record("entries");
            self.snapshot.borrow().get_accessible_entries(path)
        }
        fn realpath(&self, path: &str) -> String {
            self.record("realpath");
            self.snapshot.borrow().realpath(path)
        }
    }

    fn host() -> Host {
        let host = Host { snapshot: RefCell::default(), calls: RefCell::default() };
        host.replace(&[("/src/a.ts", "first")]);
        host
    }

    #[test]
    fn existence_queries_cache_positive_and_negative_results_independently() {
        let host = host();
        let cache = CachedFileSystem::new(&host);
        for _ in 0..2 {
            assert!(cache.file_exists("/src/a.ts"));
            assert!(!cache.file_exists("/src/absent.ts"));
            assert!(!cache.file_exists("/src"));
            assert!(cache.directory_exists("/src"));
            assert!(!cache.directory_exists("/src/a.ts"));
            assert!(!cache.directory_exists("/absent"));
        }
        assert_eq!(host.count("file"), 3);
        assert_eq!(host.count("directory"), 3);
    }

    #[test]
    fn keys_keep_exact_spelling_and_backing_case_rules() {
        let host = host();
        let cache = CachedFileSystem::new(&host);
        assert!(cache.use_case_sensitive_file_names());
        assert!(cache.file_exists("/src/a.ts"));
        assert!(cache.file_exists("/src/./a.ts"));
        assert!(!cache.file_exists("/src/A.ts"));
        assert_eq!(host.count("file"), 3);
        // A case-insensitive backing host still gets distinct spelling keys.
        host.snapshot.replace(InMemoryFileSystem::new(
            [("/src/a.ts".into(), "first".into())],
            [],
            false,
        ));
        let insensitive = CachedFileSystem::new(&host);
        assert!(!insensitive.use_case_sensitive_file_names());
        assert!(insensitive.file_exists("/src/a.ts"));
        assert!(insensitive.file_exists("/src/A.ts"));
        assert_eq!(host.count("file"), 5);
    }

    #[test]
    fn entries_and_realpaths_keep_empty_results_symlinks_and_owned_values() {
        let host = host();
        host.snapshot.replace(InMemoryFileSystem::new(
            [("/store/a.ts".into(), "first".into())],
            [("/src".into(), "/store".into())],
            true,
        ));
        let cache = CachedFileSystem::new(&host);
        assert_eq!(cache.realpath("/src/a.ts"), "/store/a.ts");
        assert_eq!(cache.realpath("/absent"), "/absent");
        let mut entries = cache.get_accessible_entries("/src");
        assert_eq!(entries.files, ["a.ts"]);
        entries.files.clear();
        for _ in 0..2 {
            assert_eq!(cache.realpath("/src/a.ts"), "/store/a.ts");
            assert_eq!(cache.realpath("/absent"), "/absent");
            assert_eq!(cache.get_accessible_entries("/src").files, ["a.ts"]);
            assert_eq!(cache.get_accessible_entries("/absent"), DirectoryEntries::default());
        }
        assert_eq!(host.count("realpath"), 2);
        assert_eq!(host.count("entries"), 2);
    }

    #[test]
    fn clear_disable_and_enable_observe_new_metadata_without_caching_reads() {
        let host = host();
        let cache = CachedFileSystem::new(&host);
        assert!(!cache.file_exists("/late/b.ts"));
        assert!(!cache.directory_exists("/late"));
        assert_eq!(cache.get_accessible_entries("/late"), DirectoryEntries::default());
        assert_eq!(cache.realpath("/late/b.ts"), "/late/b.ts");
        assert_eq!(cache.read_file("/src/a.ts").as_deref(), Some("first"));
        host.replace(&[("/src/a.ts", "second"), ("/late/b.ts", "new")]);
        assert_eq!(cache.read_file("/src/a.ts").as_deref(), Some("second"));
        assert!(!cache.file_exists("/late/b.ts"));
        assert!(CachedFileSystem::new(&host).file_exists("/late/b.ts"));
        cache.clear_cache();
        assert!(cache.file_exists("/late/b.ts"));
        assert!(cache.directory_exists("/late"));
        assert_eq!(cache.get_accessible_entries("/late").files, ["b.ts"]);
        let file_calls = host.count("file");
        cache.disable_and_clear_cache();
        for _ in 0..2 {
            assert!(cache.file_exists("/late/b.ts"));
        }
        assert_eq!(host.count("file"), file_calls + 2);
        host.replace(&[("/src/a.ts", "third")]);
        cache.disable_and_clear_cache();
        assert!(!cache.file_exists("/late/b.ts"));
        cache.enable();
        assert!(!cache.file_exists("/late/b.ts"));
        let file_calls = host.count("file");
        assert!(!cache.file_exists("/late/b.ts"));
        assert_eq!(host.count("file"), file_calls);
        assert!(!cache.directory_exists("/late"));
        assert_eq!(cache.get_accessible_entries("/late"), DirectoryEntries::default());
        assert_eq!(cache.read_file("/src/a.ts").as_deref(), Some("third"));
        assert_eq!(host.count("read"), 3);
    }

    #[test]
    fn retargeted_symlinks_require_fresh_metadata_and_disabled_queries_do_not_populate() {
        let host = host();
        let retarget = |target: &str| {
            host.snapshot.replace(InMemoryFileSystem::new(
                [(format!("/{target}/a.ts"), "first".into())],
                [("/link".into(), format!("/{target}"))],
                true,
            ));
        };
        retarget("first");
        let cache = CachedFileSystem::new(&host);
        assert_eq!(cache.realpath("/link/a.ts"), "/first/a.ts");
        retarget("second");
        assert_eq!(cache.realpath("/link/a.ts"), "/first/a.ts");
        cache.clear_cache();
        assert_eq!(cache.realpath("/link/a.ts"), "/second/a.ts");
        cache.disable_and_clear_cache();
        assert_eq!(cache.realpath("/link/a.ts"), "/second/a.ts");
        retarget("third");
        assert_eq!(cache.realpath("/link/a.ts"), "/third/a.ts");
        let calls = host.count("realpath");
        cache.enable();
        assert_eq!(cache.realpath("/link/a.ts"), "/third/a.ts");
        assert_eq!(host.count("realpath"), calls + 1);
        assert_eq!(cache.realpath("/link/a.ts"), "/third/a.ts");
        assert_eq!(host.count("realpath"), calls + 1);
    }
}
