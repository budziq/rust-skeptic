extern crate skeptic;

fn main() {
    skeptic::generate_doc_tests(&["README.md", "template-example.md"]);
}
