Rust code that includes a "`#`" should be tested by skeptic without error.

```rust
struct Person<'a>(&'a str);
let _ = Person("#bors");
```

Rust code that includes lines with single "`#`" should be tested by skeptic without error.

```rust
#
struct Person<'a>(&'a str);
#
let _ = Person("#bors");
```

Rust code with hidden parts "`# `" should be tested by skeptic without error.

```rust
# struct Person<'a>(&'a str);

let _ = Person("bors");
```

Rust code that uses attributes `"#["` should be tested by skeptic without error.

```rust

struct Person<'a>(&'a str);

#[allow(unused_variables)]
let p = Person("bors");
```

Rust code that uses crate-level attributes `"#!"` should be tested by skeptic without error.

```rust
#![allow(unused_variables)]

struct Person<'a>(&'a str);
let p = Person("bors");
```
