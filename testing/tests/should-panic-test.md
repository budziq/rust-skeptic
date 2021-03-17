Rust code that should panic when running it.

```rust,should_panic
fn main() {
  panic!("I should panic");
}
```

Rust code that should panic when compiling it.

```rust,no_run,should_panic
fn add(a: u32, b: u32) -> u32 {
    a + b
}

fn main() {
  add(1);
}
```
