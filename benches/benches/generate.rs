//! Build-script phase: parse markdown and emit test runners. Everything here
//! runs in-process, so it is what CodSpeed's simulation mode measures.
//!
//! By default a synthetic corpus is used. Set `COOKBOOK_DIR` to a checked-out
//! rust-cookbook to also benchmark its real pages.

mod common;

use std::path::{Path, PathBuf};

use common::{cookbook, markdown, triple};
use divan::{black_box, Bencher};

fn main() {
    divan::main();
}

/// `generate_doc_tests` reads `OUT_DIR`, `CARGO_MANIFEST_DIR` and `TARGET`
/// like a build script would.
fn bench_generate(bencher: Bencher, manifest_dir: &Path, docs: &[PathBuf]) {
    let out = tempfile::tempdir().unwrap();
    std::env::set_var("OUT_DIR", out.path());
    std::env::set_var("CARGO_MANIFEST_DIR", manifest_dir);
    std::env::set_var("TARGET", triple());
    bencher.bench_local(|| {
        // Remove previous output so the write path is measured too.
        let _ = std::fs::remove_file(out.path().join("skeptic-tests.rs"));
        skeptic::generate_doc_tests(black_box(docs));
    });
}

#[divan::bench(args = [10, 100, 1000])]
fn generate(bencher: Bencher, blocks: usize) {
    let dir = tempfile::tempdir().unwrap();
    let doc = dir.path().join("doc.md");
    std::fs::write(&doc, markdown(blocks)).unwrap();
    bench_generate(bencher, dir.path(), &[doc]);
}

/// Whole cookbook `src/` tree (no-op unless `COOKBOOK_DIR` is set).
#[divan::bench(sample_count = 10)]
fn generate_cookbook(bencher: Bencher) {
    let Some(dir) = cookbook() else {
        return bencher.bench_local(|| ()); // keep the bench registered
    };
    let docs = skeptic::markdown_files_of_directory(dir.join("src").to_str().unwrap());
    bench_generate(bencher, &dir, &docs);
}
