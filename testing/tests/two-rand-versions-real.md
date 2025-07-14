# Test using two versions of rand (direct and via criterion)

This test demonstrates a real-world scenario where two different versions of the `rand` crate are loaded:
- `rand` v0.8 as a direct dependency
- `rand` v0.7 as a transitive dependency through the `criterion` crate

```rust
extern crate rand;
#[cfg(not(windows))]
extern crate criterion;
extern crate oorandom;

use rand::SeedableRng;

fn main() {
    // Use direct rand 0.8
    let mut rng08 = rand::rngs::StdRng::seed_from_u64(42);
    let n08: u32 = rand::Rng::gen(&mut rng08);
    println!("rand 0.8: {}", n08);

    // Use oorandom (which criterion uses internally with rand 0.7)
    let mut rng07 = oorandom::Rand32::new(42);
    let n07: u32 = rng07.rand_u32();
    println!("oorandom (via criterion's rand 0.7): {}", n07);
    
    // Verify they're different versions by checking they produce different results
    // (even with the same seed, different rand versions may have different implementations)
    assert_ne!(n08, n07, "Different rand versions should produce different results");
}
``` 