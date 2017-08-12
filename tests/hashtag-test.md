Rust code that includes a "`#`" should be tested by skeptic without error.

```rust,no_run
#[macro_use]
extern crate error_chain;
extern crate toml;

use toml::Value;

error_chain! {
    foreign_links {
        Toml(toml::de::Error);
    }
}

fn run() -> Result<()> {
    let toml_content = r#"
          [package]
          name = "your_package"
          version = "0.1.0"
          authors = ["You! <you@example.org>"]

          [dependencies]
          serde = "1.0"
          "#;

    let package_info: Value = toml::from_str(toml_content)?;

    assert_eq!(package_info["dependencies"]["serde"].as_str(), Some("1.0"));
    assert_eq!(package_info["package"]["name"].as_str(),
               Some("your_package"));

    Ok(())
}

quick_main!(run);
```

Rust code that includes lines with single "`#`" should be tested by skeptic without error.

```rust
extern crate skeptic;
#
struct Person<'a>(&'a str);
#
fn main() {
  let _ = Person("#bors");
}
```

Rust code with hidden parts "`# `" should be tested by skeptic without error.

```rust
extern crate toml;
# struct Person<'a>(&'a str);

fn main() {
  let _ = Person("bors");
}
```

Rust code that uses attributes `"#["` should be tested by skeptic without error.

```rust
struct Person<'a>(&'a str);

#[allow(unused_variables)]
fn main() {
  let p = Person("bors");
}
```

Rust code that uses crate-level attributes `"#!"` should be tested by skeptic without error.

```rust
#![allow(unused_variables)]
struct Person<'a>(&'a str);
fn main() {
  let p = Person("bors");
}
```
