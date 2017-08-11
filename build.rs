extern crate skeptic;

fn main() {
    skeptic::generate_doc_tests(
        &[
            "README.md",
            "template-example.md",
            "tests/hashtag-test.md",
            "tests/should-panic-test.md",
            "tests/section-names.md",
        ],
    );
    // skeptic::generate_doc_tests(&skeptic::markdown_files_of_directory("."));
}
