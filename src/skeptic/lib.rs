#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate serde_derive;
extern crate pulldown_cmark as cmark;
extern crate tempdir;
extern crate glob;
extern crate bytecount;

use std::env;
use std::fs::File;
use std::io::{self, Read, Write, Error as IoError};
use std::mem;
use std::path::{PathBuf, Path};
use std::collections::HashMap;
use cmark::{Parser, Event, Tag};

/// Returns a list of markdown files under a directory.
/// 
/// # Usage
/// 
/// List markdown files of `mdbook` which are under `<project dir>/book` usually:
/// 
/// ```rust
/// extern crate skeptic;
/// 
/// use skeptic::markdown_files_of_directory;
/// 
/// fn main() {
///     let _ = markdown_files_of_directory("book/");
/// }
/// ```
pub fn markdown_files_of_directory(dir: &str) -> Vec<String> {
    use glob::{ glob_with, MatchOptions };

    let opts = MatchOptions {
        case_sensitive: false,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    let mut out = Vec::new();

    for path in glob_with(&format!("{}/**/*.md", dir), &opts).expect("Failed to read glob pattern")
                                                             .filter_map(Result::ok) {
        out.push(path.to_str().unwrap().to_owned());
    }

    out
}

/// Generates tests for specified markdown files.
/// 
/// # Usage
/// 
/// Generates doc tests for the specified files.
/// 
/// ```rust,no_run
/// extern crate skeptic;
/// 
/// use skeptic::generate_doc_tests;
/// 
/// fn main() {
///     generate_doc_tests(&["README.md"]);
/// }
/// ```
/// 
/// Or in case you want to add `mdbook` files:
/// 
/// ```rust,no_run
/// extern crate skeptic;
/// 
/// use skeptic::*;
/// 
/// fn main() {
///     let mut mdbook_files = markdown_files_of_directory("book/");
///     mdbook_files.push("README.md".to_owned());
///     generate_doc_tests(&mdbook_files);
/// }
/// ```
pub fn generate_doc_tests<T: Clone>(docs: &[T]) where T : AsRef<str> {
    // This shortcut is specifically so examples in skeptic's on
    // readme can call this function in non-build.rs contexts, without
    // panicking below.
    if docs.is_empty() {
        return;
    }

    let docs = docs.iter().cloned().filter(|d| {
        !d.as_ref().ends_with(".skt.md")
    }).collect::<Vec<_>>();

    // Inform cargo that it needs to rerun the build script if one of the skeptic files are
    // modified
    for doc in &docs {
        println!("cargo:rerun-if-changed={}", doc.as_ref());
        println!("cargo:rerun-if-changed={}.skt.md", doc.as_ref());
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let mut out_file = PathBuf::from(out_dir.clone());
    out_file.push("skeptic-tests.rs");

    let config = Config {
        out_dir: PathBuf::from(out_dir),
        root_dir: PathBuf::from(cargo_manifest_dir),
        out_file: out_file,
        docs: docs.iter().map(|s| s.as_ref().to_string()).collect(),
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
    template: Option<String>,
}

struct DocTestSuite {
    doc_tests: Vec<DocTest>,
}

struct DocTest {
    path: PathBuf,
    old_template: Option<String>,
    tests: Vec<Test>,
    templates: HashMap<String, String>,
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

enum Buffer {
    None,
    Code(Vec<String>),
    Header(String),
}

fn extract_tests_from_file(path: &Path) -> Result<DocTest, IoError> {
    let mut tests = Vec::new();
    // Oh this isn't actually a test but a legacy template
    let mut old_template = None;

    let mut file = try!(File::open(path));
    let ref mut s = String::new();
    try!(file.read_to_string(s));
    let mut parser = Parser::new(s);

    let mut buffer = Buffer::None;

    let ref file_stem = sanitize_test_name(path.file_stem().unwrap().to_str().unwrap());
    let mut section = None;

    // In order to call get_offset() on the parser,
    // this loop must not hold an exclusive reference to the parser.
    loop {
        let offset = parser.get_offset();
        let line_number = bytecount::count(&s.as_bytes()[0..offset], b'\n');
        let event = if let Some(event) = parser.next() {
            event
        } else {
            break;
        };
        match event {
            Event::Start(Tag::Header(level)) if level < 3 => {
                buffer = Buffer::Header(String::new());
            }
            Event::End(Tag::Header(level)) if level < 3 => {
                let cur_buffer = mem::replace(&mut buffer, Buffer::None);
                if let Buffer::Header(sect) = cur_buffer {
                    section = Some(sanitize_test_name(&sect));
                }
            }
            Event::Start(Tag::CodeBlock(ref info)) => {
                let code_block_info = parse_code_block_info(info);
                if code_block_info.is_rust {
                    buffer = Buffer::Code(Vec::new());
                }
            }
            Event::Text(text) => {
                if let Buffer::Code(ref mut buf) = buffer {
                    buf.push(text.into_owned());
                } else if let Buffer::Header(ref mut buf) = buffer {
                    buf.push_str(&*text);
                }
            }
            Event::End(Tag::CodeBlock(ref info)) => {
                let code_block_info = parse_code_block_info(info);
                if let Buffer::Code(buf) = mem::replace(&mut buffer, Buffer::None) {
                    if code_block_info.is_old_template {
                        old_template = Some(buf.into_iter().collect())
                    } else {
                        let name = if let Some(ref section) = section {
                            format!("{}_sect_{}_line_{}", file_stem, section, line_number)
                        } else {
                            format!("{}_line_{}", file_stem, line_number)
                        };
                        tests.push(Test {
                            name: name,
                            text: buf,
                            ignore: code_block_info.ignore,
                            no_run: code_block_info.no_run,
                            should_panic: code_block_info.should_panic,
                            template: code_block_info.template,
                        });
                    }
                }
            }
            _ => (),
        }
    }

    let templates = load_templates(path)?;

    Ok(DocTest {
        path: path.to_owned(),
        old_template: old_template,
        tests: tests,
        templates: templates,
    })
}

fn load_templates(path: &Path) -> Result<HashMap<String, String>, IoError> {
    let file_name = format!("{}.skt.md", path.file_name().expect("no file name").to_string_lossy());
    let path = path.with_file_name(&file_name);
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let mut map = HashMap::new();

    let mut file = try!(File::open(path));
    let ref mut s = String::new();
    try!(file.read_to_string(s));
    let parser = Parser::new(s);

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
                    if let Some(t) = code_block_info.template {
                        map.insert(t, buf.into_iter().collect());
                    }
                }
            }
            _ => (),
        }
    }

    Ok(map)
}

fn sanitize_test_name(s: &str) -> String {
    use std::ascii::AsciiExt;
    s.to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii() && ch.is_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .split("_")
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
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
        is_old_template: false,
        template: None,
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
                info.is_old_template = true;
                seen_rust_tags = true
            }
            _ if token.starts_with("skt-") => {
                info.template = Some(token[4..].to_string());
                seen_rust_tags = true;
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
    is_old_template: bool,
    template: Option<String>,
}

fn emit_tests(config: &Config, suite: DocTestSuite) -> Result<(), IoError> {
    let mut out = String::new();

    // Test cases use the api from skeptic::rt
    out.push_str("extern crate skeptic;\n");

    for doc_test in suite.doc_tests {
        for test in &doc_test.tests {
            let test_string = {
                if let Some(ref t) = test.template {
                    let template = doc_test.templates.get(t)
                        .expect(&format!("template {} not found for {}", t, doc_test.path.display()));
                    try!(create_test_runner(config, &Some(template.to_string()), test))
                } else {
                    try!(create_test_runner(config, &doc_test.old_template, test))
                }
            };
            out.push_str(&test_string);
        }
    }
    write_if_contents_changed(&config.out_file, &out)
}

/// Just like Rustdoc, ignore a "#" sign at the beginning of a line of code.
/// These are commonly an indication to omit the line from user-facing
/// documentation but include it for the purpose of playground links or skeptic
/// testing.
fn clean_omitted_line(line: &String) -> &str {
    let trimmed = line.trim_left();

    if trimmed.starts_with("# ") {
        &trimmed[2..]
    } else if trimmed.trim_right() == "#" {
    // line consists of single "#" which might not be followed by newline on windows
        &trimmed[1..]
    } else {
        line
    }
}

/// Creates the Rust code that this test will be operating on.
fn create_test_input(lines: &[String]) -> String {
    lines.iter().map(clean_omitted_line).collect()
}

fn create_test_runner(config: &Config,
                      template: &Option<String>,
                      test: &Test)
                      -> Result<String, IoError> {

    let template = template.clone().unwrap_or_else(|| String::from("{}"));
    let test_text = create_test_input(&test.text);

    let mut s: Vec<u8> = Vec::new();
    if test.ignore {
        try!(writeln!(s, "#[ignore]"));
    }
    if test.should_panic {
        try!(writeln!(s, "#[should_panic]"));
    }

    try!(writeln!(s, "#[test] fn {}() {{", test.name));
    try!(writeln!(s,
                  "    let s = &format!(r####\"{}{}\"####, r####\"{}\"####);",
                  "\n",
                  template,
                  test_text));

    // if we are not running, just compile the test without running it
    if test.no_run {
        try!(writeln!(s,
            "    skeptic::rt::compile_test(r#\"{}\"#, r#\"{}\"#, s);",
            config.root_dir.to_str().unwrap(),
            config.out_dir.to_str().unwrap()));
    } else {
        try!(writeln!(s,
            "    skeptic::rt::run_test(r#\"{}\"#, r#\"{}\"#, s);",
            config.root_dir.to_str().unwrap(),
            config.out_dir.to_str().unwrap()));
    }

    try!(writeln!(s, "}}"));
    try!(writeln!(s, ""));

    Ok(String::from_utf8(s).unwrap())
}

fn write_if_contents_changed(name: &Path, contents: &str) -> Result<(), IoError> {
    // Can't open in write mode now as that would modify the last changed timestamp of the file
    match File::open(name) {
        Ok(mut file) => {
            let mut current_contents = String::new();
            try!(file.read_to_string(&mut current_contents));
            if current_contents == contents {
                // No change avoid writing to avoid updating the timestamp of the file
                return Ok(())
            }
        }
        Err(ref err) if err.kind() == io::ErrorKind::NotFound => (),
        Err(err) => return Err(err),
    }
    let mut file = try!(File::create(name));
    try!(file.write(contents.as_bytes()));
    Ok(())
}

pub mod rt {
    extern crate serde;
    extern crate serde_json;
    extern crate toml;
    extern crate walkdir;

    use std::collections::{HashSet, HashMap};
    use std::collections::hash_map::Entry;
    use std::time::SystemTime;

    use std::{self, env};
    use std::fs::File;
    use std::io::{self, Write, Read};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::ffi::OsStr;
    use tempdir::TempDir;

    use self::walkdir::WalkDir;
    use self::serde_json::Value;

    error_chain! {
        errors { Fingerprint }
        foreign_links {
            Io(std::io::Error);
            Toml(toml::de::Error);
            Json(serde_json::Error);
        }
    }

    // An iterator over the root dependencies in a lockfile
    #[derive(Deserialize, Debug)]
    struct CargoLock {
        root: Deps,
    }

    #[derive(Deserialize, Debug)]
    struct Deps {
        dependencies: Vec<String>,
    }

    impl Iterator for CargoLock {
        type Item = (String, String);

        fn next(&mut self) -> Option<(String, String)> {
            self.root.dependencies.pop().and_then(|val| {
                let mut it = val.split_whitespace();

                match (it.next(), it.next()) {
                    (Some(name), Some(val)) => {
                        Some((name.replace("-", "_").to_owned(), val.to_owned()))
                    }
                    _ => None,
                }
            })
        }
    }

    impl CargoLock {
        fn from_path<P: AsRef<Path>>(pth: P) -> Result<CargoLock> {
            let pth = pth.as_ref();
            let mut contents = String::new();
            File::open(pth)?.read_to_string(&mut contents)?;
            Ok(toml::from_str(&contents)?)
        }
    }

    #[derive(Debug)]
    struct Fingerprint {
        libname: String,
        version: String,
        rlib: PathBuf,
        mtime: SystemTime,
    }

    impl Fingerprint {
        fn from_path<P: AsRef<Path>>(pth: P) -> Result<Fingerprint> {
            let pth = pth.as_ref();

            let fname = pth.file_stem().and_then(OsStr::to_str).ok_or(
                ErrorKind::Fingerprint,
            )?;

            let mut captures = fname.splitn(3, '-');
            captures.next();
            match (captures.next(), captures.next(), pth.extension()) {
                (Some(libname), Some(hash), Some(ext)) if ext == "json" => {

                    let mut rlib = PathBuf::from(pth);
                    rlib.pop();
                    rlib.pop();
                    rlib.pop();
                    rlib.push(format!("deps/lib{}-{}.rlib", libname, hash));

                    let file = File::open(pth)?;
                    let mtime = file.metadata()?.modified()?;
                    let parsed: Value = serde_json::from_reader(file)?;
                    let vers = parsed["local"]["Precalculated"]
                .as_str()
                // fingerprint file sometimes has different form
                .or_else(|| parsed["local"][0]["Precalculated"].as_str())
                .ok_or(ErrorKind::Fingerprint)?
                .to_owned();

                    Ok(Fingerprint {
                        libname: libname.to_owned(),
                        version: vers,
                        rlib: rlib,
                        mtime: mtime,
                    })
                }
                _ => Err(ErrorKind::Fingerprint.into()),
            }
        }

        fn deppair(&self) -> (String, String) {
            (self.libname.clone(), self.version.clone())
        }
    }

    // Retrieve the exact dependencies for a given build by
    // cross-referencing the lockfile with the fingerprint file
    fn get_rlib_dependencies<P: AsRef<Path>>(root_dir: P, target_dir: P) -> Result<Vec<Fingerprint>> {
        let root_dir = root_dir.as_ref();
        let target_dir = target_dir.as_ref();
        let lock = CargoLock::from_path(root_dir.join("Cargo.lock")).or_else(
            |_| {
                // could not find Cargo.lock in $CARGO_MAINFEST_DIR
                // try relative to target_dir
                let mut root_dir = PathBuf::from(target_dir);
                root_dir.pop();
                root_dir.pop();
                CargoLock::from_path(root_dir.join("Cargo.lock"))
            })?;
        let fingerprint_dir = target_dir.join(".fingerprint/");

        let set = lock.collect::<HashSet<_>>();
        let mut map: HashMap<String, Fingerprint> = HashMap::new();

        for entry in WalkDir::new(fingerprint_dir)
            .into_iter()
            .filter_map(|v| v.ok())
            .filter_map(|v| Fingerprint::from_path(v.path()).ok())
        {
            if set.contains(&entry.deppair()) {
                let libname = entry.libname.clone();
                match map.entry(libname) {
                    Entry::Occupied(mut o) => {
                        if o.get().mtime < entry.mtime {
                            o.insert(entry);
                        }
                    }
                    Entry::Vacant(v) => {
                        v.insert(entry);
                    }
                }
            }
        }

        Ok(
            map.into_iter()
                .filter_map(|(_, val)| if val.rlib.exists() { Some(val) } else { None })
                .collect(),
        )
    }

    pub fn compile_test(root_dir: &str, out_dir: &str, test_text: &str) {
        let ref rustc = env::var("RUSTC").unwrap_or(String::from("rustc"));
        let ref outdir = TempDir::new("rust-skeptic").unwrap();
        let ref testcase_path = outdir.path().join("test.rs");
        let ref binary_path = outdir.path().join("out.exe");

        write_test_case(testcase_path, test_text);
        compile_test_case(testcase_path, binary_path, rustc, root_dir, out_dir);
    }

    pub fn run_test(root_dir: &str, out_dir: &str, test_text: &str) {
        let ref rustc = env::var("RUSTC").unwrap_or(String::from("rustc"));
        let ref outdir = TempDir::new("rust-skeptic").unwrap();
        let ref testcase_path = outdir.path().join("test.rs");
        let ref binary_path = outdir.path().join("out.exe");

        write_test_case(testcase_path, test_text);
        compile_test_case(testcase_path, binary_path, rustc, root_dir, out_dir);
        run_test_case(binary_path, outdir.path());
    }

    fn write_test_case(path: &Path, test_text: &str) {
        let mut file = File::create(path).unwrap();
        file.write_all(test_text.as_bytes()).unwrap();
    }

    fn compile_test_case(in_path: &Path, out_path: &Path, rustc: &str, root_dir: &str, out_dir: &str) {

        // OK, here's where a bunch of magic happens using assumptions
        // about cargo internals. We are going to use rustc to compile
        // the examples, but to do that we've got to tell it where to
        // look for the rlibs with the -L flag, and what their names
        // are with the --extern flag. This is going to involve
        // parsing fingerprints out of the lockfile and looking them
        // up in the fingerprint file.

        let root_dir = PathBuf::from(root_dir);
        let mut target_dir = PathBuf::from(out_dir);
        target_dir.pop();
        target_dir.pop();
        target_dir.pop();
        let mut deps_dir = target_dir.clone();
        deps_dir.push("deps");

        let mut cmd = Command::new(rustc);
        cmd.arg(in_path)
            .arg("--verbose")
            .arg("-o").arg(out_path)
            .arg("--crate-type=bin")
            .arg("-L").arg(&target_dir)
            .arg("-L").arg(&deps_dir);

        for dep in get_rlib_dependencies(root_dir, target_dir).expect("failed to read dependencies") {
            cmd.arg("--extern");
            cmd.arg(format!("{}={}", dep.libname, dep.rlib.to_str().expect("filename not utf8")));
        }

        interpret_output(cmd);
    }

    fn run_test_case(program_path: &Path, outdir: &Path) {
        let mut cmd = Command::new(program_path);
        cmd.current_dir(outdir);
        interpret_output(cmd);
    }

    fn interpret_output(mut command: Command) {
        let output = command.output().unwrap();
        write!(io::stdout(),
               "{}",
               String::from_utf8(output.stdout).unwrap())
            .unwrap();
        write!(io::stderr(),
               "{}",
               String::from_utf8(output.stderr).unwrap())
            .unwrap();
        if !output.status.success() {
            panic!("Command failed:\n{:?}", command);
        }
    }
}

#[test]
fn test_omitted_lines() {
    let lines = &[
        "# use std::collections::BTreeMap as Map;\n".to_owned(),
        "#\n".to_owned(),
        "#[allow(dead_code)]\n".to_owned(),
        "fn main() {\n".to_owned(),
        "    let map = Map::new();\n".to_owned(),
        "    #\n".to_owned(),
        "    # let _ = map;\n".to_owned(),
        "}\n".to_owned(),
    ];

    let expected = [
        "use std::collections::BTreeMap as Map;\n",
        "\n",
        "#[allow(dead_code)]\n",
        "fn main() {\n",
        "    let map = Map::new();\n",
        "\n",
        "let _ = map;\n",
        "}\n",
    ].concat();

    assert_eq!(create_test_input(lines), expected);
}

#[test]
fn test_markdown_files_of_directory() {
    let files = vec![
        "../../CHANGELOG.md",
        "../../README.md",
        "../../README.md.skt.md",
        "../../template-example.md",
        "../../tests/hashtag-test.md",
        "../../tests/should-panic-test.md",
    ];
    assert_eq!(markdown_files_of_directory("../../"), files);
}

#[test]
fn test_sanitization_of_testnames() {
    assert_eq!(sanitize_test_name("My_Fun"), "my_fun");
    assert_eq!(sanitize_test_name("__my_fun_"), "my_fun");
    assert_eq!(sanitize_test_name("^$@__my@#_fun#$@"), "my_fun");
    assert_eq!(sanitize_test_name("my_long__fun___name___with____a_____lot______of_______spaces"), "my_long_fun_name_with_a_lot_of_spaces");
    assert_eq!(sanitize_test_name("Löwe 老虎 Léopard"), "l_we_l_opard");
}
