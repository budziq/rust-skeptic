# This script takes care of testing your crate

set -ex

main_tests() {
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

test_rust_cookbook() {
    # clone and checkout an arbitrary commit that we know to be ok but complex
    echo "Rust Cookbook integration tests!"
    cd ..
    rm -rf rust-cookbook || true
    git clone https://github.com/budziq/rust-cookbook.git
    cd rust-cookbook
    git checkout bddd3ad3d44dc7cca869fec935509f76066aac07
    cargo test
}

# we don't run the "test phase" when doing deploys
if [ -z $TRAVIS_TAG ]; then

    if [[ "${INTEGRATION_TESTS:-}" == 1 ]]; then
        test_rust_cookbook
    else
        main_tests
    fi

fi
