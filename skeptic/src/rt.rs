use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::SystemTime;

use cargo_metadata::Edition;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

// Persistent cache structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistentCache {
    pub cache_version: u32, // Defensive versioning to invalidate cache when data format changes
    pub entries: HashMap<String, CacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CacheEntry {
    fingerprints: Vec<Fingerprint>,
    cache_key_hash: u64,
    created_at: SystemTime,
}

impl Default for PersistentCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentCache {
    pub fn new() -> Self {
        Self {
            cache_version: 1,
            entries: HashMap::new(),
        }
    }

    pub fn get_cache_file_path(root_dir: &Path) -> PathBuf {
        root_dir.join(".skeptic-cache")
    }

    pub fn load_from_file(root_dir: &Path) -> Result<Self> {
        let cache_file = Self::get_cache_file_path(root_dir);
        if !cache_file.exists() {
            return Ok(Self::new());
        }

        let data = fs::read(&cache_file)?;
        // A cache written by an older release (binary) fails to parse and is
        // simply rebuilt.
        match serde_json::from_slice::<PersistentCache>(&data) {
            Ok(cache) => {
                if cache.cache_version == 1 {
                    Ok(cache)
                } else {
                    Ok(Self::new())
                }
            }
            Err(_) => Ok(Self::new()),
        }
    }

    pub fn save_to_file(&self, root_dir: &Path) -> Result<()> {
        let cache_file = Self::get_cache_file_path(root_dir);
        let data = serde_json::to_vec(self).map_err(|e| {
            SkepticError::Io(std::io::Error::other(format!("Serialization error: {}", e)))
        })?;
        // Write-then-rename so a concurrent reader never sees a torn file.
        let mut tmp = tempfile::NamedTempFile::new_in(root_dir)?;
        std::io::Write::write_all(&mut tmp, &data)?;
        tmp.persist(&cache_file).map_err(|e| e.error)?;
        Ok(())
    }

    pub fn generate_cache_key_hash(root_dir: &Path, target_dir: &Path) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash the paths
        root_dir.hash(&mut hasher);
        target_dir.hash(&mut hasher);

        // Hash the Cargo.toml modification time if it exists
        if let Ok(metadata) = fs::metadata(root_dir.join("Cargo.toml")) {
            if let Ok(modified) = metadata.modified() {
                modified.hash(&mut hasher);
            }
        }

        // Hash the Cargo.lock modification time if it exists
        if let Ok(metadata) = fs::metadata(root_dir.join("Cargo.lock")) {
            if let Ok(modified) = metadata.modified() {
                modified.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    pub fn get(&self, cache_key: &str, expected_hash: u64) -> Option<&Vec<Fingerprint>> {
        if let Some(entry) = self.entries.get(cache_key) {
            if entry.cache_key_hash == expected_hash {
                // Check if cache is not too old (1 hour)
                if let Ok(elapsed) = entry.created_at.elapsed() {
                    if elapsed.as_secs() < 3600 {
                        return Some(&entry.fingerprints);
                    }
                }
            }
        }
        None
    }

    pub fn insert(
        &mut self,
        cache_key: String,
        fingerprints: Vec<Fingerprint>,
        cache_key_hash: u64,
    ) {
        let entry = CacheEntry {
            fingerprints,
            cache_key_hash,
            created_at: SystemTime::now(),
        };
        self.entries.insert(cache_key, entry);
    }
}

pub fn compile_test(root_dir: &str, out_dir: &str, target_triple: &str, test_text: &str) {
    handle_test(
        root_dir,
        out_dir,
        target_triple,
        test_text,
        CompileType::Check,
    );
}

pub fn run_test(root_dir: &str, out_dir: &str, target_triple: &str, test_text: &str) {
    handle_test(
        root_dir,
        out_dir,
        target_triple,
        test_text,
        CompileType::Full,
    );
}

/// Everything needed to invoke `rustc` that is the same for every snippet of
/// a given project. Resolving it runs `cargo metadata` and walks the
/// fingerprint directory, so it is computed once and shared.
struct Prepared {
    edition: Option<&'static str>,
    target_dir: PathBuf,
    deps_dir: PathBuf,
    /// Directories holding the dependency rlibs (many, in the newer layout).
    search_dirs: Vec<PathBuf>,
    externs: Vec<(String, PathBuf)>,
}

type PreparedKey = (String, String);

static PREPARED: LazyLock<Mutex<HashMap<PreparedKey, Arc<Prepared>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Forget everything cached in memory and on disk for `root_dir`, so the next
/// snippet pays the full setup cost. Intended for benchmarks.
#[doc(hidden)]
pub fn clear_caches(root_dir: &str) {
    PREPARED.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = fs::remove_file(PersistentCache::get_cache_file_path(Path::new(root_dir)));
}

/// The `target/<profile>` directory, found from a build script's `OUT_DIR`.
/// Cargo has laid that out as both `build/<pkg>-<hash>/out` and
/// `build/<pkg>/<hash>/out`, so look for the outermost `build` directory
/// within reach instead of assuming a depth.
fn profile_dir(out_dir: &Path) -> PathBuf {
    out_dir
        .ancestors()
        .skip(1)
        .take(4)
        .filter(|dir| dir.file_name().is_some_and(|name| name == "build"))
        .last()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            // Unusual layout: fall back to the classic depth.
            let mut dir = out_dir.to_path_buf();
            dir.pop();
            dir.pop();
            dir.pop();
            dir
        })
}

/// Where cargo keeps per-unit fingerprints: `.fingerprint/` in the classic
/// layout, or inside `build/<pkg>/<hash>/fingerprint/` in the newer one.
fn fingerprint_root(target_dir: &Path) -> Option<PathBuf> {
    let classic = target_dir.join(".fingerprint");
    if classic.is_dir() {
        return Some(classic);
    }
    let build = target_dir.join("build");
    if build.is_dir() {
        return Some(build);
    }
    None
}

fn prepare(root_dir: &str, target_dir: &str) -> Arc<Prepared> {
    // Held while computing so concurrent tests wait for one resolution
    // instead of each doing their own.
    let mut cache = PREPARED.lock().unwrap_or_else(|e| e.into_inner());
    let key = (root_dir.to_owned(), target_dir.to_owned());
    if let Some(prepared) = cache.get(&key) {
        return Arc::clone(prepared);
    }

    let root_dir = PathBuf::from(root_dir);
    let target_dir = profile_dir(Path::new(target_dir));
    let deps_dir = target_dir.join("deps");

    let metadata_path = root_dir.join("Cargo.toml");
    let metadata = get_cargo_meta(&metadata_path).expect("failed to read Cargo.toml");
    let edition = metadata
        .packages
        .iter()
        .filter_map(|package| edition_str(&package.edition))
        .max()
        .unwrap();
    let edition = if edition != "2015" {
        Some(edition)
    } else {
        None
    };

    let externs: Vec<(String, PathBuf)> =
        get_rlib_dependencies(root_dir, target_dir.clone(), Some(metadata))
            .expect("failed to read dependencies")
            .into_iter()
            .map(|dep| (dep.libname, dep.rlib))
            .collect();

    // In the newer layout every unit's rlibs live in their own directory, and
    // rustc must be able to see all of them: a dependency may have been built
    // in several variants, and the one a crate was compiled against is not
    // necessarily the one we pass with --extern.
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    if !target_dir.join(".fingerprint").is_dir() {
        for pkg in fs::read_dir(target_dir.join("build"))
            .into_iter()
            .flatten()
            .flatten()
        {
            for unit in fs::read_dir(pkg.path()).into_iter().flatten().flatten() {
                let out = unit.path().join("out");
                if unit.path().join("fingerprint").is_dir() && out.is_dir() {
                    search_dirs.push(out);
                }
            }
        }
        search_dirs.sort();
    }

    let prepared = Arc::new(Prepared {
        edition,
        target_dir,
        deps_dir,
        search_dirs,
        externs,
    });
    cache.insert(key, Arc::clone(&prepared));
    prepared
}

/// Captured result of running one command.
struct Captured {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    failure: Option<String>,
}

/// Everything a worker produced for one snippet.
struct Outcome {
    steps: Vec<Captured>,
}

impl Outcome {
    fn failed(failure: String) -> Outcome {
        Outcome {
            steps: vec![Captured {
                stdout: Vec::new(),
                stderr: Vec::new(),
                failure: Some(failure),
            }],
        }
    }
}

struct Job {
    work: Box<dyn FnOnce() -> Outcome + Send>,
    reply: Sender<Outcome>,
}

/// Number of snippets compiled/run at once: `SKEPTIC_JOBS`, else the CPU count.
fn worker_count() -> usize {
    env::var("SKEPTIC_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| thread::available_parallelism().map_or(1, |n| n.get()))
}

/// A fixed pool of workers fed by a queue. libtest runs each generated test
/// on its own thread; those threads only enqueue and wait, so the number of
/// concurrent `rustc` processes stays bounded.
static QUEUE: LazyLock<Mutex<Sender<Job>>> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel::<Job>();
    let rx = Arc::new(Mutex::new(rx));
    for _ in 0..worker_count() {
        let rx = Arc::clone(&rx);
        thread::spawn(move || loop {
            let job = match rx.lock().unwrap_or_else(|e| e.into_inner()).recv() {
                Ok(job) => job,
                Err(_) => return,
            };
            // A panicking snippet must not take its worker down with it: the
            // pool is never replenished, so the remaining tests would hang.
            let outcome = panic::catch_unwind(AssertUnwindSafe(job.work)).unwrap_or_else(|_| {
                Outcome::failed("skeptic worker panicked while running a snippet".to_owned())
            });
            let _ = job.reply.send(outcome);
        });
    }
    Mutex::new(tx)
});

fn handle_test(
    root_dir: &str,
    target_dir: &str,
    target_triple: &str,
    test_text: &str,
    compile_type: CompileType,
) {
    let prepared = prepare(root_dir, target_dir);
    let target_triple = target_triple.to_owned();
    let test_text = test_text.to_owned();

    let (reply, result) = mpsc::channel();
    let job = Job {
        work: Box::new(move || execute(&prepared, &target_triple, &test_text, compile_type)),
        reply,
    };
    QUEUE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(job)
        .expect("skeptic worker pool is gone");
    let outcome = result.recv().expect("skeptic worker panicked");

    // Report from the test's own thread so libtest attributes output and
    // panics (including `should_panic`) to the right test.
    for step in outcome.steps {
        print!("{}", String::from_utf8_lossy(&step.stdout));
        eprint!("{}", String::from_utf8_lossy(&step.stderr));
        if let Some(failure) = step.failure {
            panic!("{}", failure);
        }
    }
}

fn execute(
    prepared: &Prepared,
    target_triple: &str,
    test_text: &str,
    compile_type: CompileType,
) -> Outcome {
    let out_dir = match tempfile::Builder::new().prefix("rust-skeptic").tempdir() {
        Ok(dir) => dir,
        Err(e) => return Outcome::failed(format!("Could not create a temp dir: {}", e)),
    };
    let testcase_path = out_dir.path().join("test.rs");
    if let Err(e) = fs::write(&testcase_path, test_text.as_bytes()) {
        return Outcome::failed(format!(
            "Could not write {}: {}",
            testcase_path.display(),
            e
        ));
    }

    // We use rustc directly, telling it where the rlibs are with -L and
    // their names with --extern (resolved once in `prepare`).
    let rustc = env::var("RUSTC").unwrap_or_else(|_| String::from("rustc"));
    let mut cmd = Command::new(rustc);
    cmd.arg(testcase_path)
        .arg("--verbose")
        .arg("--crate-type=bin");

    // This has to come before "-L".
    if let Some(edition) = prepared.edition {
        cmd.arg(format!("--edition={}", edition));
    }

    cmd.arg("-L")
        .arg(&prepared.target_dir)
        .arg("-L")
        .arg(&prepared.deps_dir)
        .arg("--target")
        .arg(target_triple);

    for dir in &prepared.search_dirs {
        let mut arg = std::ffi::OsString::from("dependency=");
        arg.push(dir);
        cmd.arg("-L").arg(arg);
    }

    for (libname, rlib) in &prepared.externs {
        cmd.arg("--extern");
        cmd.arg(format!(
            "{}={}",
            libname,
            rlib.to_str().expect("filename not utf8"),
        ));
    }

    let binary_path = out_dir.path().join("out.exe");
    match compile_type {
        CompileType::Full => cmd.arg("-o").arg(&binary_path),
        CompileType::Check => cmd.arg(format!(
            "--emit=dep-info={0}.d,metadata={0}.m",
            binary_path.display()
        )),
    };

    let mut steps = vec![capture(cmd)];

    if let CompileType::Full = compile_type {
        if steps[0].failure.is_none() {
            let mut cmd = Command::new(binary_path);
            cmd.current_dir(out_dir.path());
            steps.push(capture(cmd));
        }
    }

    Outcome { steps }
}

fn capture(mut command: Command) -> Captured {
    let output = match command.output() {
        Ok(output) => output,
        Err(e) => {
            return Captured {
                stdout: Vec::new(),
                stderr: Vec::new(),
                failure: Some(format!("Could not run {:?}: {}", command, e)),
            }
        }
    };
    let failure = if output.status.success() {
        None
    } else {
        Some(format!("Command failed:\n{:?}", command))
    };
    Captured {
        stdout: output.stdout,
        stderr: output.stderr,
        failure,
    }
}

// Retrieve the exact dependencies for a given build by
// cross-referencing the lockfile with the fingerprint file
fn get_rlib_dependencies(
    root_dir: PathBuf,
    target_dir: PathBuf,
    metadata: Option<cargo_metadata::Metadata>,
) -> Result<Vec<Fingerprint>> {
    let cache_key_str = format!("{}:{}", root_dir.display(), target_dir.display());
    let cache_key_hash = PersistentCache::generate_cache_key_hash(&root_dir, &target_dir);

    let mut persistent_cache = PersistentCache::load_from_file(&root_dir)?;

    // `cargo metadata` is slow, so run it at most once per call (callers that
    // already have it pass it in).
    let metadata = match metadata {
        Some(metadata) => metadata,
        None => get_cargo_meta(root_dir.join("Cargo.toml"))?,
    };
    let lock =
        LockedDeps::from_metadata(&metadata, &root_dir.join("Cargo.toml")).or_else(|_| {
            // could not find Cargo.lock in $CARGO_MAINFEST_DIR
            // try relative to target_dir
            let mut root_dir = target_dir.clone();
            root_dir.pop();
            root_dir.pop();
            LockedDeps::from_path(root_dir)
        })?;

    // Get direct dependencies first before consuming the lock
    let direct_deps = lock.get_direct_dependencies().clone();
    let locked_deps: HashMap<String, String> = lock.collect();

    // Check if cached dependencies are still valid: every locked dependency
    // must be represented (this detects dependencies added to Cargo.lock) and
    // every cached rlib must still exist (a rebuild replaces them).
    if let Some(cached_deps) = persistent_cache.get(&cache_key_str, cache_key_hash) {
        let cached_dep_names: HashSet<String> =
            cached_deps.iter().map(|d| d.name().to_string()).collect();

        let all_present = locked_deps
            .keys()
            .all(|dep_name| cached_dep_names.contains(dep_name));
        let all_exist = cached_deps.iter().all(|d| d.rlib.exists());

        if all_present && all_exist {
            return Ok(cached_deps.clone());
        }
    }

    let fingerprint_dir =
        fingerprint_root(&target_dir).unwrap_or_else(|| target_dir.join(".fingerprint"));

    // Collect all fingerprint paths first and sort for deterministic behavior
    // (`build/<pkg>/<hash>/fingerprint/<file>` is four levels down in the
    // newer layout; the classic one is two.)
    let mut fingerprint_paths: Vec<_> = WalkDir::new(fingerprint_dir)
        .max_depth(4)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().to_owned())
        .collect();

    fingerprint_paths.sort();

    // A crate can have several builds of the very same version (different
    // features, or built for build scripts), and rustc only accepts the one
    // the other crates were compiled against. Cargo records, in each unit's
    // fingerprint, which builds of its dependencies it used; so a build that
    // other candidates were compiled against is the one to link.
    struct Candidate {
        finger: Fingerprint,
        own_hash: Option<String>,
    }

    let mut candidates = Vec::new();
    for path in &fingerprint_paths {
        let Ok(finger) = Fingerprint::from_path(path, Some(&metadata)) else {
            continue;
        };
        let name = finger.name();
        let Some(locked_ver) = locked_deps.get(&name) else {
            continue;
        };
        // A direct dependency is linked at the version the root package asked
        // for, which may differ from the version of a transitive copy. Only
        // the exact version will do: a semver-compatible leftover build was
        // not what this project resolved to. Unversioned entries (they might
        // be workspace members) are kept, but never override versioned ones.
        let wanted = direct_deps.get(&name).unwrap_or(locked_ver);
        if finger.version().is_some_and(|ver| ver != *wanted) {
            continue;
        }
        let (own_hash, _) = unit_hashes(path);
        candidates.push(Candidate { finger, own_hash });
    }

    // Count the builds that depend on each one, over every unit (including
    // the root package's own test targets, which are the ground truth for
    // what a snippet is compiled alongside).
    let mut referrers: HashMap<String, usize> = HashMap::new();
    for path in fingerprint_paths
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
    {
        for hash in unit_hashes(path).1 {
            *referrers.entry(hash).or_default() += 1;
        }
    }

    // Prefer versioned, then most depended-upon, then freshest; on a tie the
    // later path wins, which keeps the choice deterministic.
    let rank = |candidate: &Candidate| {
        (
            candidate.finger.version.is_some(),
            candidate
                .own_hash
                .as_ref()
                .and_then(|hash| referrers.get(hash))
                .copied()
                .unwrap_or(0),
            candidate.finger.mtime,
        )
    };
    let mut best: std::collections::BTreeMap<String, &Candidate> =
        std::collections::BTreeMap::new();
    for candidate in &candidates {
        let slot = best.entry(candidate.finger.name());
        match slot {
            std::collections::btree_map::Entry::Occupied(mut e) => {
                if rank(candidate) >= rank(e.get()) {
                    e.insert(candidate);
                }
            }
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(candidate);
            }
        }
    }
    let found_deps: std::collections::BTreeMap<String, Fingerprint> = best
        .into_iter()
        .map(|(name, candidate)| (name, candidate.finger.clone()))
        .collect();

    let result: Vec<Fingerprint> = found_deps
        .into_iter()
        .filter_map(|(name, val)| {
            if val.rlib.exists() {
                Some(val)
            } else {
                eprintln!(
                    "Warning: rlib does not exist for {}: {}",
                    name,
                    val.rlib.display()
                );
                None
            }
        })
        .collect();

    // Save to persistent cache
    persistent_cache.insert(cache_key_str, result.clone(), cache_key_hash);
    let _ = persistent_cache.save_to_file(&root_dir);

    Ok(result)
}

/// The fingerprint hash of a unit and of the dependency builds it was
/// compiled against, as hex strings in the same (little-endian) form cargo
/// stores in the extensionless `lib-<name>` file next to the JSON.
fn unit_hashes(json_path: &Path) -> (Option<String>, HashSet<String>) {
    let own = fs::read_to_string(json_path.with_extension(""))
        .ok()
        .map(|hash| hash.trim().to_owned());
    let deps = fs::read_to_string(json_path)
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|json| {
            let deps = json.get("deps")?.as_array()?.clone();
            Some(
                deps.iter()
                    .filter_map(|dep| {
                        // `[index, name, public, hash]`, or an object in
                        // other cargo versions.
                        let hash = dep
                            .as_array()
                            .and_then(|dep| dep.last())
                            .or_else(|| dep.get("fingerprint"))?;
                        Some(format!("{:016x}", hash.as_u64()?.swap_bytes()))
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();
    (own, deps)
}

/// Populate the cache during the build phase to make test runs faster
pub fn populate_cache_during_build(root_dir: &Path, target_triple: &str) -> Result<()> {
    fn populate_cache_for_target(root_dir: &Path, target_dir: &Path) -> Result<bool> {
        if !target_dir.exists() || fingerprint_root(target_dir).is_none() {
            return Ok(false);
        }

        let cache_key_str = format!("{}:{}", root_dir.display(), target_dir.display());
        let cache_key_hash = PersistentCache::generate_cache_key_hash(root_dir, target_dir);

        let persistent_cache = PersistentCache::load_from_file(root_dir)?;
        if persistent_cache
            .get(&cache_key_str, cache_key_hash)
            .is_some()
        {
            return Ok(true);
        }

        let _ = get_rlib_dependencies(root_dir.to_path_buf(), target_dir.to_path_buf(), None)?;
        Ok(true)
    }

    // Try to find target directory from OUT_DIR during build script execution
    if let Ok(out_dir) = env::var("OUT_DIR") {
        let out_path = PathBuf::from(&out_dir);
        let mut target_dir = out_path.clone();

        // Go up from out_dir to find the target directory
        // OUT_DIR structure: target/{profile}/build/{package}-{hash}/out
        for _ in 0..4 {
            target_dir.pop();
            if populate_cache_for_target(root_dir, &target_dir)? {
                return Ok(());
            }
        }
    }

    // Fallback: try the traditional locations
    let potential_target_dirs = [
        root_dir.join("target").join(target_triple).join("debug"),
        root_dir.join("target").join(target_triple).join("release"),
        root_dir.join("target").join("debug"),
        root_dir.join("target").join("release"),
    ];

    for target_dir in &potential_target_dirs {
        if populate_cache_for_target(root_dir, target_dir)? {
            return Ok(());
        }
    }

    Ok(())
}

// An iterator over the root dependencies in a lockfile
#[derive(Debug)]
pub(crate) struct LockedDeps {
    dependencies: Vec<(String, String)>,
    direct_dependencies: HashMap<String, String>,
}

fn get_cargo_meta<P: AsRef<Path> + std::convert::AsRef<std::ffi::OsStr>>(
    path: P,
) -> Result<cargo_metadata::Metadata> {
    Ok(cargo_metadata::MetadataCommand::new()
        .manifest_path(&path)
        .exec()?)
}

impl LockedDeps {
    pub(crate) fn from_path<P: AsRef<Path>>(path: P) -> Result<LockedDeps> {
        let path = path.as_ref().join("Cargo.toml");
        let metadata = get_cargo_meta(&path)?;
        Self::from_metadata(&metadata, &path)
    }

    pub(crate) fn from_metadata(
        metadata: &cargo_metadata::Metadata,
        manifest_path: &Path,
    ) -> Result<LockedDeps> {
        let resolve = metadata
            .resolve
            .as_ref()
            .ok_or(SkepticError::MissingDependencyMetadata)?;
        let all_nodes: HashMap<_, _> = resolve.nodes.iter().map(|node| (&node.id, node)).collect();
        // Look names and versions up in the package list rather than parsing
        // the package id, whose format differs between cargo versions.
        let packages: HashMap<_, _> = metadata.packages.iter().map(|p| (&p.id, p)).collect();

        // The package whose manifest this is. Compare canonical paths so that
        // symlinks and separators don't matter; a virtual workspace has no
        // such package, so it starts from every workspace member (as the
        // original implementation did).
        let canonical = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let manifest = canonical(manifest_path);
        let root_ids: Vec<&cargo_metadata::PackageId> = metadata
            .root_package()
            .or_else(|| {
                metadata
                    .packages
                    .iter()
                    .find(|pkg| canonical(pkg.manifest_path.as_std_path()) == manifest)
            })
            .map(|pkg| vec![&pkg.id])
            .unwrap_or_else(|| metadata.workspace_members.iter().collect());
        if root_ids.is_empty() {
            return Err(SkepticError::RootPackageNotFound);
        }

        let mut direct_deps = HashMap::new();
        let mut to_visit = Vec::new();
        for id in &root_ids {
            let node = all_nodes.get(id).ok_or(SkepticError::RootPackageNotFound)?;
            for dep_id in &node.dependencies {
                if let Some(pkg) = packages.get(dep_id) {
                    direct_deps.insert(pkg.name.replace('-', "_"), pkg.version.to_string());
                }
                to_visit.push(dep_id);
            }
        }

        // Walk the transitive dependencies.
        let mut all_deps: HashSet<&cargo_metadata::PackageId> = root_ids.iter().copied().collect();
        while let Some(dep_id) = to_visit.pop() {
            if all_deps.insert(dep_id) {
                if let Some(dep_node) = all_nodes.get(dep_id) {
                    to_visit.extend(dep_node.dependencies.iter());
                }
            }
        }

        // Collect all dependencies, preferring the direct dependency's version
        let mut dep_pairs = Vec::new();
        for node_id in &all_deps {
            if let Some(pkg) = packages.get(node_id) {
                let name = pkg.name.replace('-', "_");
                let version = direct_deps
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| pkg.version.to_string());
                dep_pairs.push((name, version));
            }
        }

        Ok(LockedDeps {
            dependencies: dep_pairs,
            direct_dependencies: direct_deps,
        })
    }

    pub fn get_direct_dependencies(&self) -> &HashMap<String, String> {
        &self.direct_dependencies
    }
}

impl Iterator for LockedDeps {
    type Item = (String, String);
    fn next(&mut self) -> Option<(String, String)> {
        self.dependencies.pop()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Fingerprint {
    pub libname: String,
    pub version: Option<String>, // version might not be present on path or vcs deps
    pub rlib: PathBuf,
    pub mtime: SystemTime,
}

fn guess_ext(mut path: PathBuf, exts: &[&str]) -> Result<PathBuf> {
    for ext in exts {
        path.set_extension(ext);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(SkepticError::Fingerprint)
}

/// The crate name and unit hash a fingerprint file belongs to.
///
/// Classic layout: `.fingerprint/<lib>-<hash>/lib-<lib>.json`.
/// Newer layout: `build/<pkg>/<hash>/fingerprint/lib-<lib>.json`.
fn fingerprint_identity(path: &Path) -> Option<(String, String)> {
    let unit_dir = path.parent()?;
    let dir_name = unit_dir.file_name()?.to_str()?;

    if dir_name == "fingerprint" {
        let hash = unit_dir.parent()?.file_name()?.to_str()?;
        let stem = path.file_stem()?.to_str()?;
        let lib = stem.strip_prefix("lib-")?;
        return Some((lib.replace('-', "_"), hash.to_owned()));
    }

    let mut captures = dir_name.rsplit('-');
    let hash = captures.next()?;
    let mut name_parts = captures.collect::<Vec<_>>();
    name_parts.reverse();
    Some((name_parts.join("_"), hash.to_owned()))
}

/// Which version of a crate a build unit is, when the graph holds several.
///
/// A unit's dep-info file (`<lib>-<hash>.d`) lists the source files it was
/// compiled from, so the package whose directory they live in is the one.
fn version_from_dep_info(
    dep_info: &Path,
    candidates: &[&cargo_metadata::Package],
) -> Option<String> {
    let content = fs::read_to_string(dep_info).ok()?;
    candidates
        .iter()
        .find(|pkg| {
            let dir = pkg.manifest_path.parent().map(|d| d.as_str().to_owned());
            dir.is_some_and(|dir| {
                // Anchor on a separator so 0.8.5 doesn't match 0.8.50.
                content.contains(&format!("{}/", dir)) || content.contains(&format!("{}\\", dir))
            })
        })
        .map(|pkg| pkg.version.to_string())
}

fn extract_version_from_fingerprint(
    path: &Path,
    dep_info: &Path,
    metadata: Option<&cargo_metadata::Metadata>,
) -> Option<String> {
    let metadata = metadata?;
    let (lib_name, _) = fingerprint_identity(path)?;
    let mut matching_packages: Vec<_> = metadata
        .packages
        .iter()
        .filter(|pkg| pkg.name.replace('-', "_") == lib_name)
        .collect();

    match matching_packages.len() {
        0 => None,
        1 => Some(matching_packages[0].version.to_string()),
        _ => version_from_dep_info(dep_info, &matching_packages).or_else(|| {
            // Can't tell: fall back to the highest version.
            matching_packages.sort_by(|a, b| b.version.cmp(&a.version));
            Some(matching_packages[0].version.to_string())
        }),
    }
}

impl Fingerprint {
    pub fn from_path<P: AsRef<Path>>(
        path: P,
        metadata: Option<&cargo_metadata::Metadata>,
    ) -> Result<Fingerprint> {
        let path = path.as_ref();

        let (libname, hash) = fingerprint_identity(path).ok_or(SkepticError::Fingerprint)?;

        path.extension()
            .filter(|&e| e == "json")
            .ok_or(SkepticError::Fingerprint)?;

        // Classic layout: <profile>/.fingerprint/<lib>-<hash>/<file>.json with
        // the rlib in <profile>/deps. Newer layout:
        // <profile>/build/<pkg>/<hash>/fingerprint/<file>.json with the rlib in
        // the sibling `out` directory.
        let unit_dir = path.parent().ok_or(SkepticError::Fingerprint)?;
        let (rlib, dll) = if unit_dir.file_name().is_some_and(|n| n == "fingerprint") {
            let out = unit_dir.with_file_name("out");
            (
                out.join(format!("lib{}-{}", libname, hash)),
                out.join(format!("{}-{}", libname, hash)),
            )
        } else {
            let profile = unit_dir
                .parent()
                .and_then(Path::parent)
                .ok_or(SkepticError::Fingerprint)?;
            (
                profile.join(format!("deps/lib{}-{}", libname, hash)),
                profile.join(format!("deps/{}-{}", libname, hash)),
            )
        };
        let rlib =
            guess_ext(rlib, &["rlib", "so", "dylib"]).or_else(|_| guess_ext(dll, &["dll"]))?;

        let dep_info = rlib.with_file_name(format!("{}-{}.d", libname, hash));
        let version = extract_version_from_fingerprint(path, &dep_info, metadata);

        Ok(Fingerprint {
            libname,
            version,
            rlib,
            mtime: fs::metadata(path)?.modified()?,
        })
    }

    pub fn name(&self) -> String {
        self.libname
            .split('-')
            .next()
            .unwrap_or(&self.libname)
            .to_string()
    }

    pub fn version(&self) -> Option<String> {
        self.version.clone()
    }
}

#[derive(Debug, Error)]
pub enum SkepticError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Cargo metadata error: {0}")]
    Metadata(#[from] cargo_metadata::Error),
    #[error("Fingerprint error")]
    Fingerprint,
    #[error("Root package not found")]
    RootPackageNotFound,
    #[error("Missing dependency metadata")]
    MissingDependencyMetadata,
}

type Result<T> = std::result::Result<T, SkepticError>;

#[derive(Clone, Copy)]
enum CompileType {
    Full,
    Check,
}

fn edition_str(edition: &Edition) -> Option<&'static str> {
    Some(match edition {
        Edition::E2015 => "2015",
        Edition::E2018 => "2018",
        Edition::E2021 => "2021",
        _ => return None,
    })
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn profile_dir_classic_layout() {
        let out = Path::new("/w/target/debug/build/testing-0123abcd/out");
        assert_eq!(profile_dir(out), Path::new("/w/target/debug"));
    }

    #[test]
    fn profile_dir_new_layout() {
        let out = Path::new("/w/target/debug/build/testing/0123abcd/out");
        assert_eq!(profile_dir(out), Path::new("/w/target/debug"));
    }

    #[test]
    fn profile_dir_package_named_build() {
        let out = Path::new("/w/target/debug/build/build/0123abcd/out");
        assert_eq!(profile_dir(out), Path::new("/w/target/debug"));
    }

    #[test]
    fn profile_dir_ignores_build_dirs_above_target() {
        let out = Path::new("/home/me/build/proj/target/release/build/x-1/out");
        assert_eq!(
            profile_dir(out),
            Path::new("/home/me/build/proj/target/release")
        );
    }

    #[test]
    fn fingerprint_identity_classic() {
        let path = Path::new("/w/target/debug/.fingerprint/rand_core-0123abcd/lib-rand_core.json");
        assert_eq!(
            fingerprint_identity(path),
            Some(("rand_core".to_owned(), "0123abcd".to_owned()))
        );
    }

    #[test]
    fn fingerprint_identity_new_layout() {
        let path =
            Path::new("/w/target/debug/build/rand_core/0123abcd/fingerprint/lib-rand_core.json");
        assert_eq!(
            fingerprint_identity(path),
            Some(("rand_core".to_owned(), "0123abcd".to_owned()))
        );
    }

    #[test]
    fn fingerprint_identity_new_layout_rejects_non_lib_units() {
        let path = Path::new("/w/target/debug/build/foo/0123abcd/fingerprint/run-build-script-build-script-build.json");
        assert_eq!(fingerprint_identity(path), None);
    }
}

#[cfg(test)]
mod cache_format_tests {
    use super::*;

    #[test]
    fn unreadable_cache_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        // What an older, binary-format cache looks like to a JSON reader.
        fs::write(
            PersistentCache::get_cache_file_path(dir.path()),
            [0u8, 1, 0, 0, 0, 0, 0, 0, 0, 255, 254],
        )
        .unwrap();
        let cache = PersistentCache::load_from_file(dir.path()).unwrap();
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn cache_round_trips_through_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PersistentCache::new();
        let fingerprint = Fingerprint {
            libname: "serde".to_owned(),
            version: Some("1.0.0".to_owned()),
            rlib: PathBuf::from("/t/deps/libserde-abc.rlib"),
            mtime: SystemTime::now(),
        };
        cache.insert("k".to_owned(), vec![fingerprint], 42);
        cache.save_to_file(dir.path()).unwrap();

        let loaded = PersistentCache::load_from_file(dir.path()).unwrap();
        let entry = &loaded.entries["k"];
        assert_eq!(entry.cache_key_hash, 42);
        assert_eq!(entry.fingerprints[0].libname, "serde");
        assert_eq!(
            entry.fingerprints[0].rlib,
            Path::new("/t/deps/libserde-abc.rlib")
        );
    }
}

#[cfg(test)]
mod cache_write_tests {
    use super::*;

    #[test]
    fn save_leaves_only_the_cache_file() {
        let dir = tempfile::tempdir().unwrap();
        PersistentCache::new().save_to_file(dir.path()).unwrap();
        PersistentCache::new().save_to_file(dir.path()).unwrap();
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [".skeptic-cache"]);
    }
}
