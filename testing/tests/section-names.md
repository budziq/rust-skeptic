## Test Case  Names   With    weird     spacing       are        generated      without        error.

```rust
struct Person<'a>(&'a str);
fn main() {
  let _ = Person("bors");
}
```

## !@#$ Test Cases )(() with {}[] non alphanumeric characters ^$23 characters are "`#`" generated correctly @#$@#$  22.

```rust
struct Person<'a>(&'a str);
fn main() {
  let _ = Person("bors");
}
```

## Test cases with non ASCII ö_老虎_é characters are generated correctly.

```rust
struct Person<'a>(&'a str);
fn main() {
  let _ = Person("bors");
}
```
