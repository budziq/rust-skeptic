# 0.12.1

* [Fix warnings caused by new function naming scheme](https://github.com/brson/rust-skeptic/commit/fa1dcb87505dab899e4abdbf30e27b55620c1f3d)
* [Fix regressions in `#` handling](https://github.com/brson/rust-skeptic/commit/54841cf789ad787ba3b638267fdc851cea5f7f65)

Contributors: Michal Budzynski

# 0.12.0

* [Generate test names using section names and line numbers](https://github.com/brson/rust-skeptic/pull/41/files)
* [Make handling of `#` more like rustdoc](https://github.com/brson/rust-skeptic/pull/40)
* [Add support for listing files under a directory](https://github.com/brson/rust-skeptic/pull/31)

Contributors: Brian Anderson, Chris Butler, Michael Howell, Victor
Polevoy

# 0.11.0

* [Update Fix problem with duplicate dependency resolution](https://github.com/brson/rust-skeptic/pull/36)

Contributors: Brian Anderson, Michal Budzynski

# 0.10.1

* [Update pulldown-cmark and bump version](https://github.com/brson/rust-skeptic/pull/32)
* [Corrected test errors with windows line endings on '#' hidden sections](https://github.com/brson/rust-skeptic/pull/35)

Contributors: Brian Anderson, Michal Budzynski

# 0.10.0

* [Force skeptic tests to be located in temporary directory](https://github.com/brson/rust-skeptic/pull/26)

Contributors: Brian Anderson, Michal Budzynski

# 0.9.0

* [Allow omitted lines like rustdoc tests do](https://github.com/brson/rust-skeptic/pull/21)

Contributors: Brian Anderson, David Tolnay

# 0.8.0

* [Introduce more flexible templates](https://github.com/brson/rust-skeptic/pull/20)

# 0.6.1

* [Only overwrite the generated test file when it is not modified](https://github.com/brson/rust-skeptic/pull/10)
* [Pass --extern to rustc for all crates](https://github.com/brson/rust-skeptic/pull/11)

Contributors: Brian Anderson, Markus, Oliver Schneider

# 0.6.0

* [Fix `no_run`](https://github.com/brson/rust-skeptic/pull/7)

# 0.5.0

* [Allow Rust code with hashtags to be tested](https://github.com/brson/rust-skeptic/pull/2).
* [Add support for no_run info string](https://github.com/brson/rust-skeptic/pull/5).
