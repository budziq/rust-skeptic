Rust code that includes a `"#` should be tested by skeptic without error.

```rust
struct Person<'a>(&'a str);
fn main() {
  let _ = Person("#bors");
}
```

Rust code with hidden parts `"#` should be tested by skeptic without error.

```rust
# struct Person<'a>(&'a str);
#
fn main() {
  let _ = Person("#bors");
}
```
