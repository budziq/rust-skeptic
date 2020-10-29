# 0.13.5

* [Fixed problems with changed target directory layout](https://github.com/budziq/rust-skeptic/pull/121)
* [Bumped minimal Rust version to 1.32](https://github.com/budziq/rust-skeptic/pull/114)

Contributors: icefoxen, Michał Budzyński, Andrew Gauger, Dirkjan Ochtman

# 0.13.4
* [Add support for rust editions](https://github.com/budziq/rust-skeptic/pull/91)
* [Established minimal rust version as 1.24](https://github.com/budziq/rust-skeptic/pull/86)
* [Fancier markdown escaping](https://github.com/budziq/rust-skeptic/pull/84)

Contributors: Phlosioneer, Behnam Esfahbod, Michał Budzyński, Kornel

# 0.13.3

* [Added integration tests with rust-cookbook to Travis CI](https://github.com/budziq/rust-skeptic/commit/178276c9a5d2149bc0012afe1e3c807df2a2885e)
* [Make skeptic usable with Rust 1.16](https://github.com/budziq/rust-skeptic/commit/cecd4574a7264a7636f7201f8b930ea41f3ccfdb)
* [Fix problem with linking duplicate *.so deps](https://github.com/budziq/rust-skeptic/commit/23b738c5ca16697b5497a9fdadfaacffa71a8504)
* [Don't check for changes in skt.md files that don't exist](https://github.com/budziq/rust-skeptic/commit/8b3ba1aece727ad7596a65812fbe23b200297c60)

Contributors: Michał Budzyński, Matt Brubeck, Ryman, llogiq, Andreas Jonson

# 0.13.2

* [Fixed testfails on cargo beta due to missing root in Cargo.lock](https://github.com/budziq/rust-skeptic/pull/66)
* [Fixed linking problems when workspace members were rebuild without clean](https://github.com/budziq/rust-skeptic/pull/66)

Contributors: Michał Budzyński

# 0.13.1

* [Prevented pulldown-cmark from pulling getopt dependency](https://github.com/budziq/rust-skeptic/pull/64)

Contributors: Michał Budzyński

# 0.13.0

* [Fixed test line numbers](https://github.com/budziq/rust-skeptic/commit/5fce0912ad2538b48ff47bfd07530c16288519e0)
* [Refactored test extraction logic](https://github.com/budziq/rust-skeptic/commit/75b6ca56811f9c6383c5e1813c4571abb9c455ab)
* [Fixed failing test under windows](https://github.com/budziq/rust-skeptic/commit/8d0ee743a72920705f88474cb64b0af05ec4713a)
* [Fixed path and vcs dependency resolution](https://github.com/budziq/rust-skeptic/commit/8bfbebace429ef15679ffe4e7da0d289066728cb)
* [Improved speed of `no_run` tests](https://github.com/budziq/rust-skeptic/commit/9de430dc1f51cc1cc1afdd8ff9a019ce355ad711)

Contributors: Michał Budzyński, Simon Baptista

# 0.12.3

* [Improved auto-generated test case name sanitization](https://github.com/budziq/rust-skeptic/commit/3e384a6bd6c55ac7013cccbb22bb8c49c2dc6be0)
* [Clarified build.rs example documentation](https://github.com/budziq/rust-skeptic/commit/9dd2087403c28a49f7149f7b9594cdad65ebc3a7)

Contributors: Michał Budzyński, Frank McSherry

# 0.12.2

* [Fix problem with missing "Cargo.lock" when in workspace subproject](https://github.com/budziq/rust-skeptic/commit/f1be38eb8baa8c2267eb572eac9bb43706b29d8c)

Contributors: Michał Budzyński

# 0.12.1

* [Fix warnings caused by new function naming scheme](https://github.com/budziq/rust-skeptic/commit/fa1dcb87505dab899e4abdbf30e27b55620c1f3d)
* [Fix regressions in `#` handling](https://github.com/budziq/rust-skeptic/commit/54841cf789ad787ba3b638267fdc851cea5f7f65)

Contributors: Michał Budzyński

# 0.12.0

* [Generate test names using section names and line numbers](https://github.com/budziq/rust-skeptic/pull/41/files)
* [Make handling of `#` more like rustdoc](https://github.com/budziq/rust-skeptic/pull/40)
* [Add support for listing files under a directory](https://github.com/budziq/rust-skeptic/pull/31)

Contributors: Brian Anderson, Chris Butler, Michael Howell, Victor
Polevoy

# 0.11.0

* [Update Fix problem with duplicate dependency resolution](https://github.com/budziq/rust-skeptic/pull/36)

Contributors: Brian Anderson, Michał Budzyński

# 0.10.1

* [Update pulldown-cmark and bump version](https://github.com/budziq/rust-skeptic/pull/32)
* [Corrected test errors with windows line endings on '#' hidden sections](https://github.com/budziq/rust-skeptic/pull/35)

Contributors: Brian Anderson, Michał Budzyński

# 0.10.0

* [Force skeptic tests to be located in temporary directory](https://github.com/budziq/rust-skeptic/pull/26)

Contributors: Brian Anderson, Michał Budzyński

# 0.9.0

* [Allow omitted lines like rustdoc tests do](https://github.com/budziq/rust-skeptic/pull/21)

Contributors: Brian Anderson, David Tolnay

# 0.8.0

* [Introduce more flexible templates](https://github.com/budziq/rust-skeptic/pull/20)

# 0.6.1

* [Only overwrite the generated test file when it is not modified](https://github.com/budziq/rust-skeptic/pull/10)
* [Pass --extern to rustc for all crates](https://github.com/budziq/rust-skeptic/pull/11)

Contributors: Brian Anderson, Markus, Oliver Schneider

# 0.6.0

* [Fix `no_run`](https://github.com/budziq/rust-skeptic/pull/7)

# 0.5.0

* [Allow Rust code with hashtags to be tested](https://github.com/budziq/rust-skeptic/pull/2).
* [Add support for no_run info string](https://github.com/budziq/rust-skeptic/pull/5).
