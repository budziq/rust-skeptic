# Skeptic Template Example

This is an example of [Rust Skeptic
Templates](README.md#skeptic-templates). See
[README.md](README.md) for the main documentation.

Templates allow you to explicitly perform some of the automatic
transformations that rustdoc does on code examples.

In this document we will automatically inject `extern crate skeptic;`,
turn off unused imports, and wrap the example in main:

<code>```rust,skeptic-template</code>
```rust,skeptic-template
#![allow(unused_imports)]
extern crate skeptic;

{{test}}

```
<code>```</code>

Note that this is a [Rust format
specifier](http://doc.rust-lang.org/std/fmt/index.html), so braces are
treated specially, and need to be escaped with double-braces.

Now, examples we write here can take some shortcuts:

```rust
use skeptic::generate_doc_tests;

let _cant_do_this_without_main = 0;
```
