//! Helpers shared by the bench targets.

#![allow(dead_code)]

use std::fmt::Write;
use std::path::PathBuf;

/// The cookbook checkout, canonicalized: cargo reports manifest paths in
/// canonical form and skeptic compares them as strings.
pub fn cookbook() -> Option<PathBuf> {
    let dir = std::env::var_os("COOKBOOK_DIR")?;
    Some(std::fs::canonicalize(&dir).unwrap_or_else(|e| panic!("COOKBOOK_DIR {:?}: {}", dir, e)))
}

pub fn triple() -> &'static str {
    option_env!("TARGET").unwrap_or(HOST)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const HOST: &str = "aarch64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const HOST: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const HOST: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const HOST: &str = "aarch64-unknown-linux-gnu";
#[cfg(not(any(
    all(
        target_os = "macos",
        any(target_arch = "aarch64", target_arch = "x86_64")
    ),
    all(
        target_os = "linux",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
)))]
const HOST: &str = "";

/// A page resembling a cookbook chapter with `blocks` rust snippets.
pub fn markdown(blocks: usize) -> String {
    let mut s = String::from("# Page\n\nIntro text.\n\n");
    for i in 0..blocks {
        writeln!(
            s,
            "## Section {i}\n\nSome *prose* with `code` and a [link](x).\n"
        )
        .unwrap();
        writeln!(
            s,
            "```rust\n# use std::collections::HashMap;\nfn main() {{\n    let mut m = HashMap::new();\n    m.insert({i}, {i});\n    assert_eq!(m[&{i}], {i});\n}}\n```\n"
        )
        .unwrap();
    }
    s
}
