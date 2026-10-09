//! Per-snippet phase: `cargo metadata`, rlib resolution and `rustc`.
//!
//! These spawn child processes, which CodSpeed's instruction-counting
//! simulation cannot see, so CI does not run this target; run it locally with
//! `cargo bench --bench rt`. Set `COOKBOOK_DIR` to a checked-out and
//! already-built rust-cookbook to use its real dependency tree.

mod common;

use std::path::Path;

use common::{cookbook, triple};
use divan::{black_box, Bencher};

fn main() {
    divan::main();
}

/// Throwaway projects of increasing dependency complexity, built once into
/// `target/fixtures` so benches only measure skeptic's own work.
mod fixtures {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub const NAMES: &[&str] = &["no_deps", "many_deps", "workspace", "conflicting"];

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn lib(dir: &Path) {
        write(&dir.join("src/lib.rs"), "");
    }

    fn build(dir: &Path) {
        let status = Command::new("cargo")
            .arg("build")
            .arg("--quiet")
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success(), "building fixture {}", dir.display());
    }

    /// Path of the (built) fixture, creating it on first use.
    pub fn path(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fixtures");
        let dir = root.join(name);
        // The workspace fixture is benchmarked through its `root` member.
        let entry = if name == "workspace" {
            dir.join("root")
        } else {
            dir.clone()
        };
        if entry.join("target").exists() {
            return entry;
        }
        let pkg = |deps: &str| {
            format!(
                "[package]\nname = \"fx-{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\n{deps}"
            )
        };
        match name {
            "no_deps" => {
                write(&dir.join("Cargo.toml"), &pkg(""));
                lib(&dir);
            }
            "many_deps" => {
                write(
                    &dir.join("Cargo.toml"),
                    &pkg("serde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\nregex = \"1\"\nrand = \"0.8\"\nchrono = \"0.4\"\nclap = { version = \"4\", features = [\"derive\"] }\nanyhow = \"1\"\nbyteorder = \"1\"\nwalkdir = \"2\"\ntempfile = \"3\"\nflate2 = \"1\"\nurl = \"2\"\n"),
                );
                lib(&dir);
            }
            "workspace" => {
                let members: Vec<_> = (0..8).map(|i| format!("\"m{i}\"")).collect();
                write(
                    &dir.join("Cargo.toml"),
                    &format!(
                        "[workspace]\nmembers = [{}, \"root\"]\nresolver = \"2\"\n",
                        members.join(", ")
                    ),
                );
                for i in 0..8 {
                    write(
                        &dir.join(format!("m{i}/Cargo.toml")),
                        &format!("[package]\nname = \"m{i}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\nregex = \"1\"\n"),
                    );
                    lib(&dir.join(format!("m{i}")));
                }
                let deps: String = (0..8)
                    .map(|i| format!("m{i} = {{ path = \"../m{i}\" }}\n"))
                    .collect();
                write(
                    &dir.join("root/Cargo.toml"),
                    &format!("[package]\nname = \"root\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{deps}"),
                );
                lib(&dir.join("root"));
                // skeptic is pointed at the root package's manifest.
                let root = dir.join("root");
                build(&dir);
                fs::rename(dir.join("target"), root.join("target")).ok();
                return root;
            }
            "conflicting" => {
                // Direct rand 0.9 plus a path crate that pins rand 0.8, so two
                // versions of the same crate (and of its rlibs) coexist.
                write(
                    &dir.join("Cargo.toml"),
                    &pkg("rand = \"0.9\"\nold = { path = \"old\" }\n"),
                );
                lib(&dir);
                write(
                    &dir.join("old/Cargo.toml"),
                    "[package]\nname = \"old\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nrand = \"0.8\"\n",
                );
                lib(&dir.join("old"));
            }
            _ => unreachable!(),
        }
        build(&dir);
        dir
    }
}

mod rt {
    use super::*;

    const SNIPPET: &str =
        "fn main() { let v: Vec<u32> = (0..10).collect(); assert_eq!(v.len(), 10); }\n";

    fn root_dir() -> String {
        match cookbook() {
            Some(dir) => dir.to_str().unwrap().to_owned(),
            None => env!("CARGO_MANIFEST_DIR").to_owned(),
        }
    }

    /// `rt` finds rlibs relative to `<target>/<profile>/build/<pkg>/out`.
    fn out_dir_in(root: &Path) -> String {
        root.join("target/debug/build/bench-x/out")
            .to_str()
            .unwrap()
            .to_owned()
    }

    fn out_dir() -> String {
        match cookbook() {
            Some(dir) => out_dir_in(&dir),
            None => {
                let exe = std::env::current_exe().unwrap();
                let profile_dir = exe.parent().unwrap().parent().unwrap();
                profile_dir
                    .join("build/bench-x/out")
                    .to_str()
                    .unwrap()
                    .to_owned()
            }
        }
    }

    /// A `no_run` snippet: metadata lookup + `rustc --emit=metadata`.
    #[divan::bench(sample_count = 10)]
    fn compile_test() {
        skeptic::rt::compile_test(&root_dir(), &out_dir(), triple(), black_box(SNIPPET));
    }

    /// A normal snippet: full compile, link and execute.
    #[divan::bench(sample_count = 10)]
    fn run_test() {
        skeptic::rt::run_test(&root_dir(), &out_dir(), triple(), black_box(SNIPPET));
    }

    /// Per-snippet cost on projects of increasing dependency complexity.
    #[divan::bench(args = fixtures::NAMES, sample_count = 10)]
    fn compile_fixture(name: &str) {
        let root = fixtures::path(name);
        skeptic::rt::compile_test(
            root.to_str().unwrap(),
            &out_dir_in(&root),
            triple(),
            black_box(SNIPPET),
        );
    }

    /// First snippet in a fresh process: includes `cargo metadata` and rlib
    /// resolution (needs `skeptic::rt::clear_caches`, see `cold` below).
    #[divan::bench(args = fixtures::NAMES, sample_count = 5)]
    fn cold_fixture(bencher: Bencher, name: &str) {
        let root = fixtures::path(name);
        let (root_s, out) = (root.to_str().unwrap().to_owned(), out_dir_in(&root));
        bencher.bench_local(|| {
            skeptic::rt::clear_caches(&root_s);
            skeptic::rt::compile_test(&root_s, &out, triple(), black_box(SNIPPET));
        });
    }
}
