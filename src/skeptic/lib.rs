extern crate pulldown_cmark as cmark;
extern crate tempdir;

use std::env;
use std::fs::File;
use std::io::Error as IoError;
use std::io::{Read, Write};
use std::path::{PathBuf, Path};
use cmark::{Parser, Event, Tag};

pub fn generate_doc_tests(docs: &[&str]) {
    // This shortcut is specifically so examples in skeptic's on
    // readme can call this function in non-build.rs contexts, without
    // panicking below.
    if docs.is_empty() {
        return;
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let mut out_file = PathBuf::from(out_dir.clone());
    out_file.push("skeptic-tests.rs");

    let config = Config {
        out_dir: PathBuf::from(out_dir),
        root_dir: PathBuf::from(cargo_manifest_dir),
        out_file: out_file,
        docs: docs.iter().map(|s| s.to_string()).collect(),
    };

    run(config);
}

struct Config {
    out_dir: PathBuf,
    root_dir: PathBuf,
    out_file: PathBuf,
    docs: Vec<String>,
}

fn run(ref config: Config) {
    let tests = extract_tests(config).unwrap();
    emit_tests(config, tests).unwrap();
}

struct Test {
    name: String,
    text: Vec<String>,
    ignore: bool,
    no_run: bool,
    should_panic: bool,
}

struct DocTestSuite {
    doc_tests: Vec<DocTest>,
}

struct DocTest {
    template: Option<String>,
    tests: Vec<Test>,
}

fn extract_tests(config: &Config) -> Result<DocTestSuite, IoError> {
    let mut doc_tests = Vec::new();
    for doc in &config.docs {
        let ref mut path = config.root_dir.clone();
        path.push(doc);
        let new_tests = try!(extract_tests_from_file(path));
        doc_tests.push(new_tests);
    }
    return Ok(DocTestSuite { doc_tests: doc_tests });
}

fn extract_tests_from_file(path: &Path) -> Result<DocTest, IoError> {
    let mut tests = Vec::new();
    let mut template = None;

    let mut file = try!(File::open(path));
    let ref mut s = String::new();
    try!(file.read_to_string(s));
    let parser = Parser::new(s);

    let mut test_name_gen = TestNameGen::new(path);
    let mut code_buffer = None;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(ref info)) => {
                let code_block_info = parse_code_block_info(info);
                if code_block_info.is_rust {
                    code_buffer = Some(Vec::new());
                }
            }
            Event::Text(text) => {
                if let Some(ref mut buf) = code_buffer {
                    buf.push(text.to_string());
                }
            }
            Event::End(Tag::CodeBlock(ref info)) => {
                let code_block_info = parse_code_block_info(info);
                if let Some(buf) = code_buffer.take() {
                    if code_block_info.is_template {
                        template = Some(join_strings(buf))
                    } else {
                        tests.push(Test {
                            name: test_name_gen.advance(),
                            text: buf,
                            ignore: code_block_info.ignore,
                            no_run: code_block_info.no_run,
                            should_panic: code_block_info.should_panic,
                        });
                    }
                }
            }
            _ => (),
        }
    }

    Ok(DocTest {
        template: template,
        tests: tests,
    })
}

fn join_strings(ss: Vec<String>) -> String {
    let mut s_ = String::new();
    for s in ss {
        s_.push_str(&s)
    }

    s_
}

struct TestNameGen {
    root: String,
    count: i32,
}

impl TestNameGen {
    fn new(path: &Path) -> TestNameGen {
        let ref file_stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        TestNameGen {
            root: sanitize_test_name(file_stem),
            count: 0,
        }
    }

    fn advance(&mut self) -> String {
        let count = self.count;
        self.count += 1;
        format!("{}_{}", self.root, count)
    }
}

fn sanitize_test_name(s: &str) -> String {
    to_lowercase(s)
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// Only converting test names to lowercase to avoid style lints
// against test functions.
fn to_lowercase(s: &str) -> String {
    use std::ascii::AsciiExt;
    // FIXME: unicode
    s.to_ascii_lowercase()
}

fn parse_code_block_info(info: &str) -> CodeBlockInfo {
    // Same as rustdoc
    let tokens = info.split(|c: char| !(c == '_' || c == '-' || c.is_alphanumeric()));

    let mut seen_rust_tags = false;
    let mut seen_other_tags = false;
    let mut info = CodeBlockInfo {
        is_rust: false,
        should_panic: false,
        ignore: false,
        no_run: false,
        is_template: false,
    };

    for token in tokens {
        match token {
            "" => {}
            "rust" => {
                info.is_rust = true;
                seen_rust_tags = true
            }
            "should_panic" => {
                info.should_panic = true;
                seen_rust_tags = true
            }
            "ignore" => {
                info.ignore = true;
                seen_rust_tags = true
            }
            "no_run" => {
                info.no_run = true;
                seen_rust_tags = true;
            }
            "skeptic-template" => {
                info.is_template = true;
                seen_rust_tags = true
            }
            _ => seen_other_tags = true,
        }
    }

    info.is_rust &= !seen_other_tags || seen_rust_tags;

    info
}

struct CodeBlockInfo {
    is_rust: bool,
    should_panic: bool,
    ignore: bool,
    no_run: bool,
    is_template: bool,
}

fn emit_tests(config: &Config, suite: DocTestSuite) -> Result<(), IoError> {
    let mut file = try!(File::create(&config.out_file));

    // Test cases use the api from skeptic::rt
    try!(writeln!(file, "extern crate skeptic;\n"));

    for doc_test in suite.doc_tests {
        for test in &doc_test.tests {
            let test_string = try!(create_test_string(config, &doc_test.template, test));
            try!(writeln!(file, "{}", test_string));
        }
    }

    Ok(())
}

fn create_test_string(config: &Config,
                      template: &Option<String>,
                      test: &Test)
                      -> Result<String, IoError> {

    let template = template.clone().unwrap_or_else(|| String::from("{}"));
    let test_text = test.text.iter().fold(String::new(), |a, b| format!("{}{}", a, b));

    let mut s: Vec<u8> = Vec::new();
    if test.ignore {
        try!(writeln!(s, "#[ignore]"));
    }
    if test.should_panic {
        try!(writeln!(s, "#[should_panic]"));
    }

    try!(writeln!(s, "#[test] fn {}() {{", test.name));
    try!(writeln!(s,
                  "    let ref s = format!(\"{}\", r####\"{}\"####);",
                  template,
                  test_text));

    // if we are not running, just compile the test without running it
    if test.no_run {
        try!(writeln!(s,
            "    skeptic::rt::compile_test(r#\"{}\"#, s);",
            config.out_dir.to_str().unwrap()));
    } else {
        try!(writeln!(s,
            "    skeptic::rt::run_test(r#\"{}\"#, s);",
            config.out_dir.to_str().unwrap()));
    }

    try!(writeln!(s, "}}"));
    try!(writeln!(s, ""));

    Ok(String::from_utf8(s).unwrap())
}

pub mod rt {
    use std::env;
    use std::fs::File;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use tempdir::TempDir;

    pub fn compile_test(out_dir: &str, test_text: &str) {
        let ref rustc = env::var("RUSTC").unwrap_or(String::from("rustc"));
        let ref outdir = TempDir::new("rust-skeptic").unwrap();
        let ref testcase_path = outdir.path().join("test.rs");
        let ref binary_path = outdir.path().join("out.exe");

        write_test_case(testcase_path, test_text);
        compile_test_case(testcase_path, binary_path, rustc, out_dir);
    }

    pub fn run_test(out_dir: &str, test_text: &str) {
        let ref rustc = env::var("RUSTC").unwrap_or(String::from("rustc"));
        let ref outdir = TempDir::new("rust-skeptic").unwrap();
        let ref testcase_path = outdir.path().join("test.rs");
        let ref binary_path = outdir.path().join("out.exe");

        write_test_case(testcase_path, test_text);
        compile_test_case(testcase_path, binary_path, rustc, out_dir);
        run_test_case(binary_path);
    }

    fn write_test_case(path: &Path, test_text: &str) {
        let mut file = File::create(path).unwrap();
        file.write_all(test_text.as_bytes()).unwrap();
    }

    fn compile_test_case(in_path: &Path, out_path: &Path, rustc: &str, out_dir: &str) {

        // FIXME: Hack. Because the test runner uses rustc to build
        // tests and those tests expect access to the crate this
        // project builds and its deps, we need to find the directory
        // containing Cargo's deps to pass as a `-L` flag to
        // rustc. Cargo does not give us this directly, but we know
        // relative to OUT_DIR where to look.
        let mut target_dir = PathBuf::from(out_dir);
        target_dir.pop();
        target_dir.pop();
        target_dir.pop();
        let mut deps_dir = target_dir.clone();
        deps_dir.push("deps");

        interpret_output(Command::new(rustc)
                             .arg(in_path)
                             .arg("-o")
                             .arg(out_path)
                             .arg("--crate-type=bin")
                             .arg("-L")
                             .arg(target_dir)
                             .arg("-L")
                             .arg(deps_dir)
                             .output()
                             .unwrap());
    }
    fn run_test_case(out_path: &Path) {
        interpret_output(Command::new(out_path)
                             .output()
                             .unwrap());
    }

    fn interpret_output(output: Output) {
        write!(io::stdout(),
               "{}",
               String::from_utf8(output.stdout).unwrap())
            .unwrap();
        write!(io::stderr(),
               "{}",
               String::from_utf8(output.stderr).unwrap())
            .unwrap();
        if !output.status.success() {
            panic!("command failed");
        }
    }
}
