use std::collections::btree_map::Entry;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use cargo_metadata::Edition;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

// Persistent cache structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentCache {
    pub cache_version: u32, // Defensive versioning to invalidate cache when data format changes
    pub entries: HashMap<String, CacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
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
        match bincode::deserialize::<PersistentCache>(&data) {
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
        let data = bincode::serialize(self).map_err(|e| {
            SkepticError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Serialization error: {}", e),
            ))
        })?;
        fs::write(&cache_file, data)?;
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

fn handle_test(
    root_dir: &str,
    target_dir: &str,
    target_triple: &str,
    test_text: &str,
    compile_type: CompileType,
) {
    let out_dir = tempfile::Builder::new()
        .prefix("rust-skeptic")
        .tempdir()
        .unwrap();
    let testcase_path = out_dir.path().join("test.rs");
    fs::write(&testcase_path, test_text.as_bytes()).unwrap();

    // OK, here's where a bunch of magic happens using assumptions
    // about cargo internals. We are going to use rustc to compile
    // the examples, but to do that we've got to tell it where to
    // look for the rlibs with the -L flag, and what their names
    // are with the --extern flag. This is going to involve
    // parsing fingerprints out of the lockfile and looking them
    // up in the fingerprint file.

    let root_dir = PathBuf::from(root_dir);
    let mut target_dir = PathBuf::from(target_dir);
    target_dir.pop();
    target_dir.pop();
    target_dir.pop();
    let mut deps_dir = target_dir.clone();
    deps_dir.push("deps");

    let rustc = env::var("RUSTC").unwrap_or_else(|_| String::from("rustc"));
    let mut cmd = Command::new(rustc);
    cmd.arg(testcase_path)
        .arg("--verbose")
        .arg("--crate-type=bin");

    // Find the edition

    // This has to come before "-L".
    let metadata_path = root_dir.join("Cargo.toml");
    let metadata = get_cargo_meta(&metadata_path).expect("failed to read Cargo.toml");
    let edition = metadata
        .packages
        .iter()
        .filter_map(|package| edition_str(&package.edition))
        .max()
        .unwrap();
    if edition != "2015" {
        cmd.arg(format!("--edition={}", edition));
    }

    cmd.arg("-L")
        .arg(&target_dir)
        .arg("-L")
        .arg(&deps_dir)
        .arg("--target")
        .arg(target_triple);

    for dep in get_rlib_dependencies(root_dir, target_dir).expect("failed to read dependencies") {
        cmd.arg("--extern");
        cmd.arg(format!(
            "{}={}",
            dep.libname,
            dep.rlib.to_str().expect("filename not utf8"),
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

    interpret_output(cmd);

    if let CompileType::Check = compile_type {
        return;
    }

    let mut cmd = Command::new(binary_path);
    cmd.current_dir(out_dir.path());
    interpret_output(cmd);
}

fn interpret_output(mut command: Command) {
    let output = command.output().unwrap();
    print!("{}", String::from_utf8(output.stdout).unwrap());
    eprint!("{}", String::from_utf8(output.stderr).unwrap());
    if !output.status.success() {
        panic!("Command failed:\n{:?}", command);
    }
}

// Retrieve the exact dependencies for a given build by
// cross-referencing the lockfile with the fingerprint file
fn get_rlib_dependencies(root_dir: PathBuf, target_dir: PathBuf) -> Result<Vec<Fingerprint>> {
    let cache_key_str = format!("{}:{}", root_dir.display(), target_dir.display());
    let cache_key_hash = PersistentCache::generate_cache_key_hash(&root_dir, &target_dir);

    let mut persistent_cache = PersistentCache::load_from_file(&root_dir)?;

    let lock = LockedDeps::from_path(root_dir.clone()).or_else(|_| {
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

    // Check if cached dependencies are still valid by ensuring key dependencies are present
    // This helps detect when new dependencies have been added to Cargo.lock
    if let Some(cached_deps) = persistent_cache.get(&cache_key_str, cache_key_hash) {
        let cached_dep_names: HashSet<String> =
            cached_deps.iter().map(|d| d.name().to_string()).collect();

        // Check if all locked dependencies are represented in the cache
        let missing_deps: Vec<&String> = locked_deps
            .keys()
            .filter(|dep_name| !cached_dep_names.contains(*dep_name))
            .collect();

        if missing_deps.is_empty() {
            // Cache is valid - all expected dependencies are present
            return Ok(cached_deps.clone());
        }
    }

    let fingerprint_dir = target_dir.join(".fingerprint/");

    // Get cargo metadata once and reuse it for all fingerprints
    let metadata_path = root_dir.join("Cargo.toml");
    let metadata = get_cargo_meta(&metadata_path).ok();

    let mut found_deps: std::collections::BTreeMap<String, Fingerprint> =
        std::collections::BTreeMap::new();

    // Collect all fingerprint paths first and sort for deterministic behavior
    let mut fingerprint_paths: Vec<_> = WalkDir::new(fingerprint_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().to_owned())
        .collect();

    fingerprint_paths.sort();

    let fingerprints: Vec<_> = fingerprint_paths
        .iter()
        .filter_map(|path| Fingerprint::from_path(path, metadata.as_ref()).ok())
        .collect();

    for finger in fingerprints {
        let locked_ver = match locked_deps.get(&finger.name()) {
            Some(ver) => ver,
            None => continue,
        };

        // Check if this is a direct dependency that requires strict version matching
        let is_direct_dep = direct_deps.contains_key(&finger.name());
        let required_version = if is_direct_dep {
            Some(&direct_deps[&finger.name()])
        } else {
            None
        };
        match (found_deps.entry(finger.name()), finger.version()) {
            (Entry::Occupied(mut e), Some(ver)) => {
                // For direct dependencies, require exact version match
                if let Some(req_ver) = required_version {
                    if *req_ver == ver {
                        e.insert(finger);
                    }
                } else {
                    // For transitive dependencies, use the existing logic
                    // First try exact version match (highest priority)
                    if *locked_ver == ver {
                        e.insert(finger);
                    } else {
                        // Then try semantic version matching
                        if let (Ok(req), Ok(version)) =
                            (VersionReq::parse(locked_ver), Version::parse(&ver))
                        {
                            if req.matches(&version) {
                                // Only replace if we don't have an exact match already
                                let current = e.get();
                                if let Some(current_ver) = &current.version {
                                    if *locked_ver != *current_ver {
                                        // If current is not an exact match, replace with this one
                                        e.insert(finger);
                                    }
                                } else {
                                    e.insert(finger);
                                }
                            }
                        } else {
                            // Fallback: try to parse the locked version as a semver requirement
                            // This handles cases where the project specifies "0.8" but Cargo resolves to "0.8.5"
                            let req_str = if locked_ver.matches('.').count() == 1 {
                                // If it's like "0.8", convert to "^0.8.0"
                                format!("^{}.0", locked_ver)
                            } else {
                                // If it's already a full version like "0.8.5", convert to "^0.8.5"
                                format!("^{}", locked_ver)
                            };

                            if let (Ok(req), Ok(version)) =
                                (VersionReq::parse(&req_str), Version::parse(&ver))
                            {
                                if req.matches(&version) {
                                    let current = e.get();
                                    if let Some(current_ver) = &current.version {
                                        if *locked_ver != *current_ver {
                                            e.insert(finger);
                                        }
                                    } else {
                                        e.insert(finger);
                                    }
                                }
                            } else {
                                // Final fallback to exact match if parsing fails
                                if *locked_ver == ver && e.get().mtime < finger.mtime {
                                    e.insert(finger);
                                }
                            }
                        }
                    }
                }
            }
            (Entry::Vacant(e), Some(ver)) => {
                // For direct dependencies, require exact version match
                if let Some(req_ver) = required_version {
                    if *req_ver == ver {
                        e.insert(finger);
                    }
                } else if *locked_ver == ver {
                    e.insert(finger);
                } else if let (Ok(req), Ok(version)) =
                    (VersionReq::parse(locked_ver), Version::parse(&ver))
                {
                    if req.matches(&version) {
                        e.insert(finger);
                    }
                } else {
                    let req_str = if locked_ver.matches('.').count() == 1 {
                        format!("^{}.0", locked_ver)
                    } else {
                        format!("^{}", locked_ver)
                    };

                    if let (Ok(req), Ok(version)) =
                        (VersionReq::parse(&req_str), Version::parse(&ver))
                    {
                        if req.matches(&version) {
                            e.insert(finger);
                        }
                    } else if *locked_ver == ver {
                        e.insert(finger);
                    }
                }
            }
            (Entry::Vacant(e), None) => {
                // For unversioned entries, insert them (they might be workspace members)
                e.insert(finger);
            }
            (Entry::Occupied(_), None) => {
                // If we already have an entry and this one is unversioned, skip it
                // This prevents unversioned entries from overriding versioned ones
            }
        }
    }

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

/// Populate the cache during the build phase to make test runs faster
pub fn populate_cache_during_build(root_dir: &Path, target_triple: &str) -> Result<()> {
    fn populate_cache_for_target(root_dir: &Path, target_dir: &Path) -> Result<bool> {
        if !target_dir.exists() || !target_dir.join(".fingerprint").exists() {
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

        let _ = get_rlib_dependencies(root_dir.to_path_buf(), target_dir.to_path_buf())?;
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
pub struct LockedDeps {
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

// Update LockedDeps to accept a package name
impl LockedDeps {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<LockedDeps> {
        let path = path.as_ref().join("Cargo.toml");
        let metadata = get_cargo_meta(&path)?;
        let resolve = metadata
            .resolve
            .ok_or(SkepticError::MissingDependencyMetadata)?;
        let all_nodes: std::collections::HashMap<_, _> = resolve
            .nodes
            .into_iter()
            .map(|node| (node.id.clone(), node))
            .collect();
        // Find the root package (the one matching the manifest path)
        let root_package = metadata
            .packages
            .iter()
            .find(|pkg| pkg.manifest_path.as_str() == path.to_str().unwrap())
            .ok_or(SkepticError::RootPackageNotFound)?;
        let root_id = &root_package.id;

        // First, collect direct dependencies with their versions
        let mut direct_deps = std::collections::HashMap::new();
        if let Some(root_node) = all_nodes.get(root_id) {
            // Collect direct dependencies first
            for dep_id in &root_node.dependencies {
                if let Some(pkg) = metadata.packages.iter().find(|p| &p.id == dep_id) {
                    let name = pkg.name.replace('-', "_");
                    direct_deps.insert(name.clone(), pkg.version.to_string());
                }
            }

            // Then walk transitive dependencies, but don't override direct dependencies
            let mut all_deps = std::collections::HashSet::new();
            all_deps.insert(root_node.id.clone());
            let mut to_visit = root_node.dependencies.clone();
            while let Some(dep_id) = to_visit.pop() {
                if all_deps.insert(dep_id.clone()) {
                    if let Some(dep_node) = all_nodes.get(&dep_id) {
                        to_visit.extend(dep_node.dependencies.clone());
                    }
                }
            }

            // Collect all dependencies, prioritizing direct ones
            let mut dep_pairs = Vec::new();
            for node_id in &all_deps {
                if let Some(pkg) = metadata.packages.iter().find(|p| &p.id == node_id) {
                    let name = pkg.name.replace('-', "_");
                    let version = pkg.version.to_string();

                    // If this is a direct dependency, use it
                    if direct_deps.contains_key(&name) {
                        dep_pairs.push((name.clone(), direct_deps[&name].clone()));
                    } else {
                        // Otherwise, use the transitive dependency version
                        dep_pairs.push((name, version));
                    }
                }
            }

            Ok(LockedDeps {
                dependencies: dep_pairs,
                direct_dependencies: direct_deps,
            })
        } else {
            Err(SkepticError::RootPackageNotFound)
        }
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
pub struct Fingerprint {
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

fn extract_version_from_fingerprint<P: AsRef<Path>>(
    path: P,
    metadata: Option<&cargo_metadata::Metadata>,
) -> Result<Option<String>> {
    let path = path.as_ref();

    // First, try to extract version from the directory name using cached metadata
    if let Some(metadata) = metadata {
        if let Some(parent) = path.parent() {
            if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
                let lib_name = dir_name.split('-').next().unwrap_or("").replace('-', "_");

                // Find all packages with this name
                let matching_packages: Vec<_> = metadata
                    .packages
                    .iter()
                    .filter(|pkg| pkg.name.replace('-', "_") == lib_name)
                    .collect();

                match matching_packages.len() {
                    1 => {
                        // Only one version, use it
                        return Ok(Some(matching_packages[0].version.to_string()));
                    }
                    n if n > 1 => {
                        // Multiple versions - need to read fingerprint JSON to determine which one
                        let json_path =
                            parent.join(format!("lib-{}.json", lib_name.replace('_', "-")));
                        if json_path.exists() {
                            if let Ok(json_content) = std::fs::read_to_string(&json_path) {
                                if let Ok(fingerprint_data) =
                                    serde_json::from_str::<serde_json::Value>(&json_content)
                                {
                                    if let Some(features) = fingerprint_data
                                        .get("features")
                                        .and_then(|f| f.as_str())
                                        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                                    {
                                        // Try to match features against package versions
                                        for pkg in &matching_packages {
                                            // Check if this package's features match the fingerprint
                                            if lib_name == "rand" {
                                                // For rand, use thread_rng as a distinguishing feature
                                                let has_thread_rng =
                                                    features.contains(&"thread_rng".to_string());
                                                let pkg_has_thread_rng =
                                                    pkg.features.contains_key("thread_rng");

                                                if has_thread_rng == pkg_has_thread_rng {
                                                    return Ok(Some(pkg.version.to_string()));
                                                }
                                            } else {
                                                // For other packages, use a more general approach
                                                // This is a simplified check - in practice, you might need more sophisticated matching
                                                let feature_set: std::collections::HashSet<_> =
                                                    features.iter().collect();
                                                let pkg_feature_set: std::collections::HashSet<_> =
                                                    pkg.features.keys().collect();

                                                // Check if there's significant overlap
                                                let intersection_count = feature_set
                                                    .intersection(&pkg_feature_set)
                                                    .count();
                                                if intersection_count > 0 {
                                                    return Ok(Some(pkg.version.to_string()));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // If we can't determine from features, fall back to highest version
                        let mut sorted_packages = matching_packages;
                        sorted_packages.sort_by(|a, b| b.version.cmp(&a.version)); // Sort descending
                        return Ok(Some(sorted_packages[0].version.to_string()));
                    }
                    _ => {
                        // No matching packages - continue to fallback logic
                    }
                }
            }
        }
    }

    // Fallback to the original logic - try to find version in the fingerprint file
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if line.contains("version:") {
                if let Some(version) = line.split("version:").nth(1) {
                    let version = version.trim();
                    if !version.is_empty() {
                        return Ok(Some(version.to_string()));
                    }
                }
            }
        }
    }

    Ok(None)
}

impl Fingerprint {
    pub fn from_path<P: AsRef<Path>>(
        path: P,
        metadata: Option<&cargo_metadata::Metadata>,
    ) -> Result<Fingerprint> {
        let path = path.as_ref();

        // Use the parent path to get libname and hash, replacing - with _
        let mut captures = path
            .parent()
            .and_then(Path::file_stem)
            .and_then(OsStr::to_str)
            .ok_or(SkepticError::Fingerprint)?
            .rsplit('-');
        let hash = captures.next().ok_or(SkepticError::Fingerprint)?;
        let mut libname_parts = captures.collect::<Vec<_>>();
        libname_parts.reverse();
        let libname = libname_parts.join("_");

        path.extension()
            .and_then(|e| if e == "json" { Some(e) } else { None })
            .ok_or(SkepticError::Fingerprint)?;

        let mut rlib = PathBuf::from(path);
        rlib.pop();
        rlib.pop();
        rlib.pop();
        let mut dll = rlib.clone();
        rlib.push(format!("deps/lib{}-{}", libname, hash));
        dll.push(format!("deps/{}-{}", libname, hash));
        rlib = guess_ext(rlib, &["rlib", "so", "dylib"]).or_else(|_| guess_ext(dll, &["dll"]))?;

        // Try to extract version from the fingerprint file content
        let version = extract_version_from_fingerprint(path, metadata)?;

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
