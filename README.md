# Be a Rust Skeptic

Test your Rust Markdown documentation via Cargo.

# Getting started

Put this in `Cargo.toml` to add the `skeptic` dependency:

```toml
[build-dependencies]
skeptic = "*"
```

Also in `Cargo.toml`, to the `[package]` section add:

```toml
build = "build.rs"
```

That adds a [build script](http://doc.crates.io/build-script.html)
through which you will tell Skeptic to build test cases from a set
of Markdown files.

In `build.rs` write this to test all code blocks in `README.md`:

```rust
extern crate skeptic;

fn main() {
    skeptic::generate_doc_tests(&["README.md"]);
}
```

Finally, in `tests/skeptic.rs` put the following macros to tie the
generated test cases to `cargo test`:

```rust
include!(concat!(env!("OUT_DIR"), "/skeptic-tests.rs"));
```

Now any code blocks in `README.md` will be tested during `cargo test`.

[This `README.md` file itself is tested by Rust Skeptic](https://github.com/brson/rust-skeptic/blob/master/build.rs).

# Details

Not the same as rustdoc. TODO. TBD.

# License

MIT/Apache-2.0
