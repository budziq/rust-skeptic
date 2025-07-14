use unindent::unindent;

use super::*;
use crate::rt::{Fingerprint, LockedDeps, PersistentCache};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use tempfile::TempDir;

#[test]
fn test_omitted_lines() {
    let lines = unindent(
        r###"
        # use std::collections::BTreeMap as Map;
        #
        #[allow(dead_code)]
        fn main() {
            let map = Map::new();
            #
            # let _ = map;
        }"###,
    );

    let expected = unindent(
        r###"
        use std::collections::BTreeMap as Map;

        #[allow(dead_code)]
        fn main() {
            let map = Map::new();

        let _ = map;
        }
        "###,
    );

    assert_eq!(create_test_input(&get_lines(lines)), expected);
}

#[test]
fn test_markdown_files_of_directory() {
    let files = [
        "../testing/tests/hashtag-test.md",
        "../testing/tests/section-names.md",
        "../testing/tests/should-panic-test.md",
        "../testing/tests/two-rand-versions-real.md",
    ];
    let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    assert_eq!(markdown_files_of_directory("../testing/tests/"), files);
}

#[test]
fn test_sanitization_of_testnames() {
    assert_eq!(sanitize_test_name("My_Fun"), "my_fun");
    assert_eq!(sanitize_test_name("__my_fun_"), "my_fun");
    assert_eq!(sanitize_test_name("^$@__my@#_fun#$@"), "my_fun");
    assert_eq!(
        sanitize_test_name("my_long__fun___name___with____a_____lot______of_______spaces",),
        "my_long_fun_name_with_a_lot_of_spaces"
    );
    assert_eq!(sanitize_test_name("Löwe 老虎 Léopard"), "l_we_l_opard");
}

#[test]
fn line_numbers_displayed_are_for_the_beginning_of_each_code_block() {
    let lines = unindent(
        r###"
        Rust code that should panic when running it.

        ```rust,should_panic",/
        fn main() {
            panic!(\"I should panic\");
        }
        ```

        Rust code that should panic when compiling it.

        ```rust,no_run,should_panic",//
        fn add(a: u32, b: u32) -> u32 {
            a + b
        }

        fn main() {
            add(1);
        }
        ```"###,
    );

    let tests =
        extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));

    let test_names: Vec<String> = tests
        .0
        .into_iter()
        .map(get_line_number_from_test_name)
        .collect();

    assert_eq!(test_names, vec!["3", "11"]);
}

#[test]
fn line_numbers_displayed_are_for_the_beginning_of_each_section() {
    let lines = unindent(
        r###"
        ## Test Case  Names   With    weird     spacing       are        generated      without        error.

        ```rust", /
        struct Person<'a>(&'a str);
        fn main() {
          let _ = Person(\"bors\");
        }
        ```

        ## !@#$ Test Cases )(() with {}[] non alphanumeric characters ^$23 characters are \"`#`\" generated correctly @#$@#$  22.

        ```rust", //
        struct Person<'a>(&'a str);
        fn main() {
          let _ = Person(\"bors\");
        }
        ```

        ## Test cases with non ASCII ö_老虎_é characters are generated correctly.

        ```rust",//
        struct Person<'a>(&'a str);
        fn main() {
          let _ = Person(\"bors\");
        }
        ```"###,
    );

    let tests =
        extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));

    let test_names: Vec<String> = tests
        .0
        .into_iter()
        .map(get_line_number_from_test_name)
        .collect();

    assert_eq!(test_names, vec!["3", "12", "21"]);
}

#[test]
fn old_template_is_returned_for_old_skeptic_template_format() {
    let lines = unindent(
        r###"
        ```rust,skeptic-template
        ```rust,ignore
        use std::path::PathBuf;

        fn main() {{
            {}
        }}
        ```
        ```
        "###,
    );
    let expected = unindent(
        r###"
        ```rust,ignore
        use std::path::PathBuf;

        fn main() {{
            {}
        }}
        "###,
    );
    let tests =
        extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));
    assert_eq!(tests.1, Some(expected));
}

#[test]
fn old_template_is_not_returned_if_old_skeptic_template_is_not_specified() {
    let lines = unindent(
        r###"
        ```rust", /
        struct Person<'a>(&'a str);
        fn main() {
          let _ = Person(\"bors\");
        }
        ```
        "###,
    );
    let tests =
        extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));
    assert_eq!(tests.1, None);
}

// Cache validation tests
#[test]
fn test_persistent_cache_new() {
    let cache = PersistentCache::new();
    assert_eq!(cache.cache_version, 1);
    assert!(cache.entries.is_empty());
}

#[test]
fn test_persistent_cache_insert_and_get() {
    let mut cache = PersistentCache::new();
    let fingerprints = vec![Fingerprint {
        libname: "test_lib".to_string(),
        version: Some("1.0.0".to_string()),
        rlib: PathBuf::from("/path/to/test_lib.rlib"),
        mtime: SystemTime::now(),
    }];
    let cache_key = "test_key".to_string();
    let hash = 12345u64;

    cache.insert(cache_key.clone(), fingerprints.clone(), hash);

    let retrieved = cache.get(&cache_key, hash);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().len(), 1);
    assert_eq!(retrieved.unwrap()[0].libname, "test_lib");
}

#[test]
fn test_persistent_cache_get_with_wrong_hash() {
    let mut cache = PersistentCache::new();
    let fingerprints = vec![Fingerprint {
        libname: "test_lib".to_string(),
        version: Some("1.0.0".to_string()),
        rlib: PathBuf::from("/path/to/test_lib.rlib"),
        mtime: SystemTime::now(),
    }];
    let cache_key = "test_key".to_string();
    let hash = 12345u64;
    let wrong_hash = 54321u64;

    cache.insert(cache_key.clone(), fingerprints.clone(), hash);

    let retrieved = cache.get(&cache_key, wrong_hash);
    assert!(retrieved.is_none());
}

#[test]
fn test_persistent_cache_save_and_load() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();

    // Create and populate cache
    let mut cache = PersistentCache::new();
    let fingerprints = vec![Fingerprint {
        libname: "test_lib".to_string(),
        version: Some("1.0.0".to_string()),
        rlib: PathBuf::from("/path/to/test_lib.rlib"),
        mtime: SystemTime::now(),
    }];
    let cache_key = "test_key".to_string();
    let hash = 12345u64;

    cache.insert(cache_key.clone(), fingerprints.clone(), hash);

    // Save cache
    cache.save_to_file(root_dir).unwrap();

    // Load cache
    let loaded_cache = PersistentCache::load_from_file(root_dir).unwrap();

    // Verify loaded cache
    let retrieved = loaded_cache.get(&cache_key, hash);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().len(), 1);
    assert_eq!(retrieved.unwrap()[0].libname, "test_lib");
}

#[test]
fn test_persistent_cache_generate_cache_key_hash() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();
    let target_dir = root_dir.join("target");

    let hash1 = PersistentCache::generate_cache_key_hash(root_dir, &target_dir);
    let hash2 = PersistentCache::generate_cache_key_hash(root_dir, &target_dir);

    // Same inputs should produce same hash
    assert_eq!(hash1, hash2);

    // Different target_dir should produce different hash
    let different_target = root_dir.join("different_target");
    let hash3 = PersistentCache::generate_cache_key_hash(root_dir, &different_target);
    assert_ne!(hash1, hash3);
}

#[test]
fn test_locked_deps_parsing() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();

    // Create Cargo.toml in the root directory
    let cargo_toml_content = r#"
[package]
name = "test_package"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_package"
path = "src/lib.rs"

[dependencies]
rand = "0.8.5"
"#;
    fs::write(root_dir.join("Cargo.toml"), cargo_toml_content).unwrap();

    // Create src/lib.rs
    fs::create_dir_all(root_dir.join("src")).unwrap();
    fs::write(root_dir.join("src/lib.rs"), "// Test library\n").unwrap();

    let cargo_lock_path = root_dir.join("Cargo.lock");
    let cargo_lock_content = r#"
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "autocfg"
version = "1.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "d468802bab17cbc0cc575e9b053f41e72aa36bfa6b7f55e3529ffa43161b97fa"

[[package]]
name = "cfg-if"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "baf1de4339761588bc0619e3cbc0120ee582ebb74b53b4efbf79117bd2da40fd"

[[package]]
name = "getrandom"
version = "0.2.8"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "c05aeb6a22b8f62540c194aac980f2115af067bfe15a0734d7277a768d396b31"
dependencies = [
 "cfg-if",
 "libc",
]

[[package]]
name = "libc"
version = "0.2.139"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "201de327520df007757c1f0adce6e827fe8562fbc28bfd9c15571c66ca1f5f79"

[[package]]
name = "ppv-lite86"
version = "0.2.17"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "5b40af805b3121feab8a3c29f04d8ad262fa8e0561883e7653e024ae4479e6de"

[[package]]
name = "rand"
version = "0.8.5"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "34af8d1a0e25924bc5b7c43c079c942339d8f0a8b57c39049bef581b46327404"
dependencies = [
 "libc",
 "rand_chacha",
 "rand_core",
]

[[package]]
name = "rand_chacha"
version = "0.3.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "e6c10a63a0fa32252be49d21e7709d4d4baf8d231c2dbce1eaa8141b9b127d88"
dependencies = [
 "ppv-lite86",
 "rand_core",
]

[[package]]
name = "rand_core"
version = "0.6.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "ec0be4795e2f6a28069bec0b5ff3e2ac9bafc99e6a9a7dc3547996c5c816922c"
dependencies = [
 "getrandom",
]

[[package]]
name = "test_package"
version = "0.1.0"
dependencies = [
 "rand",
]
"#;

    fs::write(&cargo_lock_path, cargo_lock_content).unwrap();

    let locked_deps = LockedDeps::from_path(root_dir).unwrap();
    let deps: HashMap<String, String> = locked_deps.collect();

    // Verify some expected dependencies - only test for what we know should be there
    assert!(deps.contains_key("rand"));
    assert_eq!(deps.get("rand"), Some(&"0.8.5".to_string()));
    assert!(deps.contains_key("test_package"));
    assert_eq!(deps.get("test_package"), Some(&"0.1.0".to_string()));

    // Verify some transitive dependencies are also included
    assert!(deps.contains_key("rand_core"));
    assert!(deps.contains_key("cfg_if"));
}

#[test]
fn test_locked_deps_direct_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();

    // Create Cargo.toml in the root directory
    let cargo_toml_content = r#"
[package]
name = "test_package"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_package"
path = "src/lib.rs"

[dependencies]
rand = "0.8.5"
"#;
    fs::write(root_dir.join("Cargo.toml"), cargo_toml_content).unwrap();

    // Create src/lib.rs
    fs::create_dir_all(root_dir.join("src")).unwrap();
    fs::write(root_dir.join("src/lib.rs"), "// Test library\n").unwrap();

    let cargo_lock_path = root_dir.join("Cargo.lock");
    let cargo_lock_content = r#"
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "rand"
version = "0.8.5"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "34af8d1a0e25924bc5b7c43c079c942339d8f0a8b57c39049bef581b46327404"
dependencies = [
 "rand_core",
]

[[package]]
name = "rand_core"
version = "0.6.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "ec0be4795e2f6a28069bec0b5ff3e2ac9bafc99e6a9a7dc3547996c5c816922c"

[[package]]
name = "test_package"
version = "0.1.0"
dependencies = [
 "rand",
]
"#;

    fs::write(&cargo_lock_path, cargo_lock_content).unwrap();

    let locked_deps = LockedDeps::from_path(root_dir).unwrap();
    let direct_deps = locked_deps.get_direct_dependencies();

    // test_package directly depends on rand
    assert!(direct_deps.contains_key("rand"));
    assert_eq!(direct_deps.get("rand"), Some(&"0.8.5".to_string()));

    // rand_core is a transitive dependency, not direct
    assert!(!direct_deps.contains_key("rand_core"));
}

#[test]
fn test_fingerprint_name_and_version() {
    let fingerprint = Fingerprint {
        libname: "test_lib-1.0.0".to_string(),
        version: Some("1.0.0".to_string()),
        rlib: PathBuf::from("/path/to/test_lib.rlib"),
        mtime: SystemTime::now(),
    };

    assert_eq!(fingerprint.name(), "test_lib");
    assert_eq!(fingerprint.version(), Some("1.0.0".to_string()));
}

#[test]
fn test_fingerprint_name_without_version() {
    let fingerprint = Fingerprint {
        libname: "test_lib".to_string(),
        version: None,
        rlib: PathBuf::from("/path/to/test_lib.rlib"),
        mtime: SystemTime::now(),
    };

    assert_eq!(fingerprint.name(), "test_lib");
    assert_eq!(fingerprint.version(), None);
}

#[test]
fn test_fingerprint_name_with_complex_version() {
    let fingerprint = Fingerprint {
        libname: "complex_lib-1.2.3_beta".to_string(),
        version: Some("1.2.3-beta".to_string()),
        rlib: PathBuf::from("/path/to/complex_lib.rlib"),
        mtime: SystemTime::now(),
    };

    assert_eq!(fingerprint.name(), "complex_lib");
    assert_eq!(fingerprint.version(), Some("1.2.3-beta".to_string()));
}

// Integration tests for cache invalidation behavior
#[test]
fn test_cache_invalidation_with_missing_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();

    // Create Cargo.toml in the root directory
    let cargo_toml_content = r#"
[package]
name = "test_package"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_package"
path = "src/lib.rs"

[dependencies]
rand = "0.8.5"
ndarray = "0.15.6"
"#;
    fs::write(root_dir.join("Cargo.toml"), cargo_toml_content).unwrap();

    // Create src/lib.rs
    fs::create_dir_all(root_dir.join("src")).unwrap();
    fs::write(root_dir.join("src/lib.rs"), "// Test library\n").unwrap();

    // Create a mock Cargo.lock with multiple dependencies
    let cargo_lock_path = root_dir.join("Cargo.lock");
    let cargo_lock_content = r#"
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "rand"
version = "0.8.5"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "34af8d1a0e25924bc5b7c43c079c942339d8f0a8b57c39049bef581b46327404"

[[package]]
name = "ndarray"
version = "0.15.6"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "adb12d4e967ec485a5f71c6311fe28158e9d6f4bc4a447b474184d0f91a8fa32"

[[package]]
name = "test_package"
version = "0.1.0"
dependencies = [
 "rand",
 "ndarray",
]
"#;

    fs::write(&cargo_lock_path, cargo_lock_content).unwrap();

    // Create a cache that only has 'rand' but missing 'ndarray'
    let mut cache = PersistentCache::new();
    let incomplete_fingerprints = vec![Fingerprint {
        libname: "rand-0.8.5".to_string(),
        version: Some("0.8.5".to_string()),
        rlib: PathBuf::from("/path/to/rand.rlib"),
        mtime: SystemTime::now(),
    }];

    let target_dir = root_dir.join("target");
    let cache_key = format!("{}:{}", root_dir.display(), target_dir.display());
    let cache_key_hash = PersistentCache::generate_cache_key_hash(root_dir, &target_dir);

    cache.insert(
        cache_key.clone(),
        incomplete_fingerprints.clone(),
        cache_key_hash,
    );
    cache.save_to_file(root_dir).unwrap();

    // Load the cache and check if it correctly identifies missing dependencies
    let loaded_cache = PersistentCache::load_from_file(root_dir).unwrap();
    let cached_deps = loaded_cache.get(&cache_key, cache_key_hash);
    assert!(cached_deps.is_some());

    // Verify that the cache validation logic would detect missing 'ndarray'
    let locked_deps = LockedDeps::from_path(root_dir).unwrap();
    let expected_deps: HashMap<String, String> = locked_deps.collect();

    let cached_dep_names: std::collections::HashSet<String> = cached_deps
        .unwrap()
        .iter()
        .map(|d| d.name().to_string())
        .collect();

    let missing_deps: Vec<&String> = expected_deps
        .keys()
        .filter(|dep_name| !cached_dep_names.contains(*dep_name))
        .collect();

    // Should find 'ndarray' as missing
    assert!(!missing_deps.is_empty());
    assert!(missing_deps.contains(&&"ndarray".to_string()));
    assert!(!missing_deps.contains(&&"rand".to_string()));
}

#[test]
fn test_cache_validation_with_complete_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();

    // Create Cargo.toml in the root directory
    let cargo_toml_content = r#"
[package]
name = "test_package"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_package"
path = "src/lib.rs"

[dependencies]
rand = "0.8.5"
"#;
    fs::write(root_dir.join("Cargo.toml"), cargo_toml_content).unwrap();

    // Create src/lib.rs
    fs::create_dir_all(root_dir.join("src")).unwrap();
    fs::write(root_dir.join("src/lib.rs"), "// Test library\n").unwrap();

    // Create a mock Cargo.lock
    let cargo_lock_path = root_dir.join("Cargo.lock");
    let cargo_lock_content = r#"
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "rand"
version = "0.8.5"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "34af8d1a0e25924bc5b7c43c079c942339d8f0a8b57c39049bef581b46327404"

[[package]]
name = "test_package"
version = "0.1.0"
dependencies = [
 "rand",
]
"#;

    fs::write(&cargo_lock_path, cargo_lock_content).unwrap();

    // First, get the actual dependencies from the Cargo.lock to create a complete cache
    let locked_deps = LockedDeps::from_path(root_dir).unwrap();
    let expected_deps: HashMap<String, String> = locked_deps.collect();

    // Create a cache that has all expected dependencies
    let mut cache = PersistentCache::new();
    let complete_fingerprints: Vec<Fingerprint> = expected_deps
        .iter()
        .map(|(name, version)| Fingerprint {
            libname: format!("{}-{}", name, version),
            version: Some(version.clone()),
            rlib: PathBuf::from(format!("/path/to/{}.rlib", name)),
            mtime: SystemTime::now(),
        })
        .collect();

    let target_dir = root_dir.join("target");
    let cache_key = format!("{}:{}", root_dir.display(), target_dir.display());
    let cache_key_hash = PersistentCache::generate_cache_key_hash(root_dir, &target_dir);

    cache.insert(
        cache_key.clone(),
        complete_fingerprints.clone(),
        cache_key_hash,
    );
    cache.save_to_file(root_dir).unwrap();

    // Load the cache and verify it has all expected dependencies
    let loaded_cache = PersistentCache::load_from_file(root_dir).unwrap();
    let cached_deps = loaded_cache.get(&cache_key, cache_key_hash);
    assert!(cached_deps.is_some());

    // Verify that the cache validation logic finds no missing dependencies
    // We already have expected_deps from above

    let cached_dep_names: std::collections::HashSet<String> = cached_deps
        .unwrap()
        .iter()
        .map(|d| d.name().to_string())
        .collect();

    let missing_deps: Vec<&String> = expected_deps
        .keys()
        .filter(|dep_name| !cached_dep_names.contains(*dep_name))
        .collect();

    // Should find no missing dependencies
    assert!(missing_deps.is_empty());
}

#[test]
fn test_cache_key_hash_stability() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();
    let target_dir = root_dir.join("target");

    // Generate hash multiple times - should be stable
    let hash1 = PersistentCache::generate_cache_key_hash(root_dir, &target_dir);
    let hash2 = PersistentCache::generate_cache_key_hash(root_dir, &target_dir);
    let hash3 = PersistentCache::generate_cache_key_hash(root_dir, &target_dir);

    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);

    // Different paths should produce different hashes
    let different_root = temp_dir.path().join("different");
    fs::create_dir_all(&different_root).unwrap();
    let hash4 = PersistentCache::generate_cache_key_hash(&different_root, &target_dir);

    assert_ne!(hash1, hash4);
}

#[test]
fn test_locked_deps_error_handling() {
    let temp_dir = TempDir::new().unwrap();
    let nonexistent_path = temp_dir.path().join("nonexistent");

    // Should return an error for nonexistent directory
    let result = LockedDeps::from_path(&nonexistent_path);
    assert!(result.is_err());
}

#[test]
fn test_locked_deps_malformed_cargo_lock() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();

    // Create malformed Cargo.toml
    let malformed_toml = r#"
This is not a valid Cargo.toml file
It should cause parsing to fail
"#;

    fs::write(root_dir.join("Cargo.toml"), malformed_toml).unwrap();

    // Should return an error for malformed file
    let result = LockedDeps::from_path(root_dir);
    assert!(result.is_err());
}

#[test]
fn test_persistent_cache_file_operations() {
    let temp_dir = TempDir::new().unwrap();
    let root_dir = temp_dir.path();

    // Test loading from nonexistent file (should create new cache)
    let cache = PersistentCache::load_from_file(root_dir).unwrap();
    assert_eq!(cache.cache_version, 1);
    assert!(cache.entries.is_empty());

    // Test saving and loading
    let mut cache = PersistentCache::new();
    let fingerprints = vec![Fingerprint {
        libname: "test_lib".to_string(),
        version: Some("1.0.0".to_string()),
        rlib: PathBuf::from("/path/to/test_lib.rlib"),
        mtime: SystemTime::now(),
    }];

    cache.insert("test_key".to_string(), fingerprints.clone(), 12345);
    cache.save_to_file(root_dir).unwrap();

    // Verify file was created
    let cache_file = PersistentCache::get_cache_file_path(root_dir);
    assert!(cache_file.exists());

    // Load and verify
    let loaded_cache = PersistentCache::load_from_file(root_dir).unwrap();
    assert_eq!(loaded_cache.cache_version, 1);
    assert_eq!(loaded_cache.entries.len(), 1);

    let retrieved = loaded_cache.get("test_key", 12345);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().len(), 1);
}

fn get_line_number_from_test_name(test: Test) -> String {
    String::from(
        test.name
            .split('_')
            .next_back()
            .expect("There were no underscores!"),
    )
}

fn get_lines(lines: String) -> Vec<String> {
    lines
        .split('\n')
        .map(|string_slice| format!("{}\n", string_slice)) //restore line endings since they are removed by split.
        .collect()
}
