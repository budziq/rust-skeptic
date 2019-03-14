# Be a Rust Documentation Skeptic

[![Unix build status](https://travis-ci.org/budziq/rust-skeptic.svg?branch=master)](https://travis-ci.org/budziq/rust-skeptic)
[![Windows build status](https://ci.appveyor.com/api/projects/status/l1f74hon37wt2vce/branch/master?svg=true)](https://ci.appveyor.com/project/budziq/rust-skeptic/branch/master)
[![crates.io](https://img.shields.io/crates/v/skeptic.svg)](https://crates.io/crates/skeptic)
[![Documentation](https://docs.rs/skeptic/badge.svg)](https://docs.rs/skeptic)

Test your Rust Markdown via Cargo.

## Getting started

Put this in `Cargo.toml` to add the `skeptic` dependency:

```toml
[build-dependencies]
skeptic = "0.13"

[dev-dependencies]
skeptic = "0.13"
```

Also in `Cargo.toml`, to the `[package]` section add:

```toml
build = "build.rs"
```

That adds a [build script](https://doc.crates.io/build-script.html)
through which you will tell Skeptic to build test cases from a set
of Markdown files.

In `build.rs` write this to test all Rust code blocks in `README.md`:

```rust,no_run
extern crate skeptic;

fn main() {
    // generates doc tests for `README.md`.
    skeptic::generate_doc_tests(&["README.md"]);
}
```

If you want to test multiple markdown files, you just need to build 
a list of filenames and supply that to `generate_doc_tests`. To help
you, the method `markdown_files_of_directory` will create such a list,
enumerating the markdown files in the specified directory. You can add
more files to this list as you like:

```rust,no_run
extern crate skeptic;

use skeptic::*;

fn main() {
    // Add all markdown files in directory "book/".
    let mut mdbook_files = markdown_files_of_directory("book/");
    // Also add "README.md" to the list of files.
    mdbook_files.push("README.md".into());
    generate_doc_tests(&mdbook_files);
}
```


Finally, in `tests/skeptic.rs` put the following macros to tie the
generated test cases to `cargo test`:

```rust,ignore
include!(concat!(env!("OUT_DIR"), "/skeptic-tests.rs"));
```

Now any Rust code blocks in `README.md` will be tested during `cargo
test`.

## Users' Guide

Rust Skeptic is not based on rustdoc. It behaves similarly in many
cases, but not all. Here's the lowdown on the Skeptic system.

*Note: [this `README.md` file itself is tested by Rust
Skeptic](https://github.com/budziq/rust-skeptic/blob/master/build.rs).
Because it is illustrating how to use markdown syntax, the markup on
this document itself is funky, and so is the output below,
particularly when illustrating Markdown's code fences
(<code>```rust</code>).*

*You must ask for `rust` code blocks explicitly to get Rust testing*,
with <code>```rust</code>. This is different from rustdoc, which
assumes code blocks are Rust. The reason for this is that common
Markdown parsers, like that used on GitHub, also do not assume Rust by
default: you either get both Rust syntax highlighting and testing, or
no Rust syntax highlighting and testing.

So the below is not tested by Skeptic.

````
```
let this_is_not_going_to_be_compiled_and_run = @all;
It doesn't really matter what's in here.
```
````

To indicate Rust code, code blocks are labeled `rust`:

````rust,ignore
```rust
fn main() {
   println!("Calm your skepticism. This example is verified.");
}
```
````

Skeptic will interpret other words in the code block's 'info string'
(which should be separated by comma, `,`, to be
GitHub-compatible). These words change how the test is interpreted:
`ignore`, `no_run`, and `should_panic`.

### `ignore` Info String

The `ignore` info string causes the test to be completely ignored.  It will not
be compiled or run during testing.  This can be useful if an example is written
in Rust (and you want it highlighted as such) but it is known to be incomplete
(so it cannot compile as-is).

````rust,ignore
```rust,ignore
fn do_amazing_thing() -> i32 {
   // TODO: How do I do this?
   unimplemented! whatever I'm distracted, oh cookies!
```
````

### `no_run` Info String

The `no_run` info string causes the example code not to be run during testing.
Code marked with `no_run` will however still be compiled.  This is useful for
examples/test that may have side effects or dependencies which are not desirable
in a testing situation.

````rust,ignore
```rust,no_run
fn do_amazing_thing() -> i32 {
   // TODO: How do I do this?
   unimplemented!()
}

fn main() {
   do_amazing_thing();
}
```
````

### `should_panic` Info String

`should_panic` causes the test to only pass if it terminates because
of a `panic!()`.

````rust,ignore
```rust,should_panic
fn main() {
   assert!(1 == 100);
}
```
````

## Skeptic Templates

Unlike rustdoc, *Skeptic does not modify examples before testing by
default*. Skeptic examples are placed in a '.rs' file, compiled, then
run.

This means that - *by default* - Skeptic examples require a `main`
function, as in all the examples above. Implicit wrapping of examples
in `main`, and custom injection of `extern crate` statements and crate
attributes are controlled through templates.

Templates for a document are located in a separate file, that lives
next to the document on the filesystem, and has the same full name as
the document file, but with an additional ".skt.md" template.

So for example, this file, `README.md`, stores its templates
in `README.md.skt.md`.

This scheme allows the markdown to be displayed naturally by stock
Markdown renderers without displaying the template itself. The weird
file extension is similarly so that the templates themselves are
interpreted as valid markdown.

Consider this example:

```rust,skt-foo
let p = PathBuf::from("foo");
println!("{:?}", p);
```

This example won't compile without defining `main` and importing
`PathBuf`, but the example itself does not contain that
boilerplate. Instead it is annotated `skt-foo`, for _skeptic template
foo_, like so:

````rust,ignore
```rust,skt-foo
let p = PathBuf::from("foo");
println!("{:?}", p);
```
````

This tells skeptic to look in the template file for another
markdown block with the same `skt-foo` annotation, and compose
them together using the standard Rust `format!` macro. Here's
what the template looks like:

````rust,ignore
```rust,skt-foo
use std::path::PathBuf;

fn main() {{
    {}
}}
```
````

Templates are [Rust format
specifiers](https://doc.rust-lang.org/std/fmt/index.html) that must
take a single argument (i.e. they need to contain the string
"{}"). See [the (old) template example](template-example.md) for more
on templates.

Note that in a template, real braces need to be doubled.

## The old-style, document-global template

Within a document, a `rust` code block tagged `skeptic-template` will
be used as the template for all examples in the doc that are not
explicitly tagged.

````rust,ignore
```rust,skeptic-template
use std::path::PathBuf;

fn main() {{
    {}
}}
```
````

## Rustdoc-style undisplayed lines with `# `

Like rustdoc, skeptic will remove preceding `# ` from any lines of
code before compiling them. Hiding such lines during display requires
custom support in the markdown renderer.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or https://apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
