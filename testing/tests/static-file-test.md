Files from skeptic-static should be copied over before compiling and running tests

```rust
fn main() {
    // The contents of file.txt is "Hello world!"
    let str_from_file = include_str!("file.txt");
    assert_eq!(str_from_file, "Hello world!");
}
```
