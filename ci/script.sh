# This script takes care of testing your crate

set -ex

main() {
    # remove clean once fixed https://github.com/budziq/rust-skeptic/issues/57
    cargo clean
    cargo build
    cargo build --release

    if [ ! -z $DISABLE_TESTS ]; then
        return
    fi

    cargo test
    cargo test --release

    cd src/skeptic
    cargo test
    cargo test --release
}

# we don't run the "test phase" when doing deploys
if [ -z $TRAVIS_TAG ]; then
    main
fi
