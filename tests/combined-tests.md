Define a function once:

```rust,sk-part-of-example-1,sk-part-of-example-2
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
```

And use it like this:

```rust,sk-part-of-example-1
fn main() {
    assert_eq!(greet("Alice"), "Hello, Alice!");
}
```

Or even like this:

```rust,sk-part-of-example-2
fn main() {
    assert_eq!(greet("Bob"), "Hello, Bob!");
}
```
