# Be a Rust Documentation Skeptic

Test your Rust Markdown via Cargo.

## Getting started

Put this in `Cargo.toml` to add the `skeptic` dependency:

```toml
[build-dependencies]
skeptic = "0.12"

[dev-dependencies]
skeptic = "0.12"
```

Also in `Cargo.toml`, to the `[package]` section add:

```toml
build = "build.rs"
```

That adds a [build script](http://doc.crates.io/build-script.html)
through which you will tell Skeptic to build test cases from a set
of Markdown files.

In `build.rs` write this to test all Rust code blocks in `README.md`:

```rust
extern crate skeptic;

fn main() {
    // generates doc tests for `README.md`.
    skeptic::generate_doc_tests(&["README.md"]);
}
```

If you want to test multiple markdown files, you just need to build 
a list of filenames and supply that to `generate_doc_tests`. To help
you, the method `markdown_files_of_directory` with create such a list
enumerating the markdown files in the specified directory. You can add
more files to this list as you like:

```rust,no_run
extern crate skeptic;

use skeptic::*;

fn main() {
    // Add all markdown files in directory "book/".
    let mut mdbook_files = markdown_files_of_directory("book/");
    // Also add "README.md" to the list of files.
    mdbook_files.push("README.md".to_owned());
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
Skeptic](https://github.com/brson/rust-skeptic/blob/master/build.rs).
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

<code>```</code>
```
let this_is_not_going_to_be_compiled_and_run = @all;
It doesn't really matter what's in here.
```
<code>```</code>

To indicate Rust code, code blocks are labeled `rust`:

<code>```rust</code>
```rust
fn main() {
   println!("Calm your skepticism. This example is verified.");
}
```
<code>```</code>

Skeptic will interpret other words in the code block's 'info string'
(which should be separated by comma, `,`, to be
GitHub-compatible). These words change how the test is interpreted:
`ignore`, `no_run`, and `should_panic`.

### `ignore` Info String

The `ignore` info string causes the test to be completely ignored.  It will not
be compiled or run during testing.  This can be useful if an example is written
in Rust (and you want it highlighted as such) but it is known to be incomplete
(so it cannot compile as-is).

<code>```rust,ignore</code>
```rust,ignore
fn do_amazing_thing() -> i32 {
   // TODO: How do I do this?
   unimplemented! whatever I'm distracted, oh cookies!
```
<code>```</code>

### `no_run` Info String

The `no_run` info string causes the example code not to be run during testing.
Code marked with `no_run` will however still be compiled.  This is useful for
examples/test that may have side effects or dependencies which are not desirable
in a testing situation.

<code>```rust,no_run</code>
```rust,no_run
fn do_amazing_thing() -> i32 {
   // TODO: How do I do this?
   unimplemented!()
}

fn main() {
   do_amazing_thing();
}
```
<code>```</code>

### `should_panic` Info String

`should_panic` causes the test to only pass if it terminates because
of a `panic!()`.

<code>```rust,should_panic</code>
```rust,should_panic
fn main() {
   assert!(1 == 100);
}
```
<code>```</code>

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

<code>```rust,skt-foo</code>
```rust,skt-foo
let p = PathBuf::from("foo");
println!("{:?}", p);
```
<code>```</code>

This tells skeptic to look in the template file for another
markdown block with the same `skt-foo` annotation, and compose
them together using the standard Rust `format!` macro. Here's
what the template looks like:

<code>```rust,skt-foo</code>
```rust,ignore
use std::path::PathBuf;

fn main() {{
    {}
}}
```
<code>```</code>

Templates are [Rust format
specifiers](http://doc.rust-lang.org/std/fmt/index.html) that must
take a single argument (i.e. they need to contain the string
"{}"). See [the (old) template example](template-example.md) for more
on templates.

Note that in a template, real braces need to be doubled.

## The old-style, document-global template

Within a document, a `rust` code block tagged `skeptic-template` will
be used as the template for all examples in the doc that are not
explicitly tagged.

<code>```rust,skeptic-template</code>
```rust,ignore
use std::path::PathBuf;

fn main() {{
    {}
}}
```
<code>```</code>

## Rustdoc-style undisplayed lines with `# `

Like rustdoc, skeptic will remove preceding `# ` from any lines of
code before compiling them. Hiding such lines during display requires
custom support in the markdown renderer.

## License

MIT/Apache-2.0
