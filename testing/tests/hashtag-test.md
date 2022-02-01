Rust code that includes a "`#`" should be tested by skeptic without error.

```rust
#[derive(derive_more::From)]
struct Person<'a>(&'a str);
fn main() {
  let _ = Person::from("#bors");
}
```

Rust code that includes lines with single "`#`" should be tested by skeptic without error.

```rust
#
struct Person<'a>(&'a str);
#
fn main() {
  let _ = Person("#bors");
}
```

Rust code with hidden parts "`# `" should be tested by skeptic without error.

```rust
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
