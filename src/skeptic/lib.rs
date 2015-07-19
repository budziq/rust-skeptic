extern crate pulldown_cmark as cmark;

use std::env;
use std::fs::File;
use std::io::Error as IoError;
use std::io::{Read, Write};
use std::path::{PathBuf, Path};
use cmark::{Parser, Event, Tag};

pub fn generate_doc_tests(docs: &[&str]) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let mut out_file = PathBuf::from(out_dir);
    out_file.push("skeptic-tests.rs");
    
    let config = Config {
        root_dir: PathBuf::from(cargo_manifest_dir),
        out_file: out_file,
        docs: docs.iter().map(|s| s.to_string()).collect()
    };

    run(config);
}

struct Config {
    root_dir: PathBuf,
    out_file: PathBuf,
    docs: Vec<String>
}

fn run(ref config: Config) {
    let tests = extract_tests(config).unwrap();
    emit_tests(config, tests).unwrap();
}

struct Test {
    name: String,
    text: String
}

fn extract_tests(config: &Config) -> Result<Vec<Test>, IoError> {
    let mut tests = Vec::new();
    for doc in &config.docs {
        let ref mut path = config.root_dir.clone();
        path.push(doc);
        let new_tests = try!(extract_tests_from_file(path));
        tests.extend(new_tests.into_iter());
    }
    return Ok(tests);
}

fn extract_tests_from_file(path: &Path) -> Result<Vec<Test>, IoError> {
    let mut tests = Vec::new();

    let mut file = try!(File::open(path));
    let ref mut s = String::new();
    try!(file.read_to_string(s));
    let parser = Parser::new(s);

    let mut test_name_gen = TestNameGen::new(path);
    let mut save_next_test = false;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(_meta)) => {
                save_next_test = true;
            }
            Event::Text(text) => {
                if save_next_test {
                    save_next_test = false;
                    tests.push(Test {
                        name: test_name_gen.advance(),
                        text: text.to_string()
                    });
                }
            }
            _ => ()
        }
    }
    return Ok(tests);
}

struct TestNameGen {
    root: String,
    count: i32
}

impl TestNameGen {
    fn new(path: &Path) -> TestNameGen {
        let ref file_stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        TestNameGen {
            root: sanitize_test_name(file_stem),
            count: 0
        }
    }

    fn advance(&mut self) -> String {
        let count = self.count;
        self.count += 1;
        format!("{}_{}", self.root, count)
    }
}

fn sanitize_test_name(s: &str) -> String {
    s.chars().map(|c| {
        if c.is_alphanumeric() {
            c
        } else {
            '_'
        }
    }).collect()
}

fn emit_tests(config: &Config, tests: Vec<Test>) -> Result<(), IoError> {
    let mut file = try!(File::create(&config.out_file));
    for test in tests {
        try!(writeln!(file, "#[test] fn {}() {{", test.name));
        try!(writeln!(file, "{}", test.text));
        try!(writeln!(file, "}}"));
        try!(writeln!(file, ""));
    }

    Ok(())
}
