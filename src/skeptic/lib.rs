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
    text: Vec<String>,
    ignore: bool,
    should_panic: bool
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
    let mut test_buffer = None;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(ref info)) => {
                let code_block_info = parse_code_block_info(info);
                if code_block_info.is_rust {
                    test_buffer = Some(Vec::new());
                }
            }
            Event::Text(text) => {
                if let Some(ref mut buf) = test_buffer {
                    buf.push(text.to_string());
                }
            }
            Event::End(Tag::CodeBlock(ref info)) => {
                let code_block_info = parse_code_block_info(info);
                if let Some(buf) = test_buffer.take() {
                    tests.push(Test {
                        name: test_name_gen.advance(),
                        text: buf,
                        ignore: code_block_info.ignore,
                        should_panic: code_block_info.should_panic
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
    s.to_lowercase().chars().map(|c| {
        if c.is_alphanumeric() {
            c
        } else {
            '_'
        }
    }).collect()
}

fn parse_code_block_info(info: &str) -> CodeBlockInfo {
    // Same as rustdoc
    let tokens = info.split(|c: char| {
        !(c == '_' || c == '-' || c.is_alphanumeric())
    });

    let mut seen_rust_tags = false;
    let mut seen_other_tags = false;
    let mut info = CodeBlockInfo {
        is_rust: true,
        should_panic: false,
        ignore: false,
    };
    
    for token in tokens {
        match token {
            "" => {}
            "rust" => { info.is_rust = true; seen_rust_tags = true }
            "should_panic" => { info.should_panic = true; seen_rust_tags = true }
            "ignore" => { info.ignore = true; seen_rust_tags = true }
            _ => { seen_other_tags = true }
        }
    }

    info.is_rust &= !seen_other_tags || seen_rust_tags;

    info
}

struct CodeBlockInfo {
    is_rust: bool,
    should_panic: bool,
    ignore: bool
}

fn emit_tests(config: &Config, tests: Vec<Test>) -> Result<(), IoError> {
    let mut file = try!(File::create(&config.out_file));
    for test in tests {
        if test.ignore {
            try!(writeln!(file, "#[ignore]"));
        }
        if test.should_panic {
            try!(writeln!(file, "#[should_panic]"));
        }
        try!(writeln!(file, "#[test] fn {}() {{", test.name));
        if test.ignore {
            try!(writeln!(file, "/*"));
        }
        for text in &test.text {
            try!(write!(file, "    {}", text));
        }
        if test.ignore {
            try!(writeln!(file, "*/"));
        }
        try!(writeln!(file, "}}"));
        try!(writeln!(file, ""));
    }

    Ok(())
}
