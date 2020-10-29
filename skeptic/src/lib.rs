#[macro_use]
extern crate error_chain;
extern crate bytecount;
extern crate glob;
extern crate pulldown_cmark as cmark;
extern crate tempfile;

use cmark::{Event, Parser, Tag};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, Error as IoError, Read, Write};
use std::mem;
use std::path::{Path, PathBuf};

pub mod rt;

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
pub fn markdown_files_of_directory(dir: &str) -> Vec<PathBuf> {
    use glob::{glob_with, MatchOptions};

    let opts = MatchOptions {
        case_sensitive: false,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    let mut out = Vec::new();

    for path in glob_with(&format!("{}/**/*.md", dir), opts)
        .expect("Failed to read glob pattern")
        .filter_map(Result::ok)
    {
        out.push(path.to_str().unwrap().into());
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
///     mdbook_files.push("README.md".into());
///     generate_doc_tests(&mdbook_files);
/// }
/// ```
pub fn generate_doc_tests<T: Clone>(docs: &[T])
where
    T: AsRef<Path>,
{
    // This shortcut is specifically so examples in skeptic's on
    // readme can call this function in non-build.rs contexts, without
    // panicking below.
    if docs.is_empty() {
        return;
    }

    let docs = docs
        .iter()
        .cloned()
        .map(|path| path.as_ref().to_str().unwrap().to_owned())
        .filter(|d| !d.ends_with(".skt.md"))
        .collect::<Vec<_>>();

    // Inform cargo that it needs to rerun the build script if one of the skeptic files are
    // modified
    for doc in &docs {
        println!("cargo:rerun-if-changed={}", doc);

        let skt = format!("{}.skt.md", doc);
        if Path::new(&skt).exists() {
            println!("cargo:rerun-if-changed={}", skt);
        }
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let mut out_file = PathBuf::from(out_dir.clone());
    out_file.push("skeptic-tests.rs");

    let config = Config {
        out_dir: PathBuf::from(out_dir),
        root_dir: PathBuf::from(cargo_manifest_dir),
        out_file,
        target_triple: env::var("TARGET").expect("could not get target triple"),
        docs,
    };

    run(&config);
}

struct Config {
    out_dir: PathBuf,
    root_dir: PathBuf,
    out_file: PathBuf,
    target_triple: String,
    docs: Vec<String>,
}

fn run(config: &Config) {
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
        let path = &mut config.root_dir.clone();
        path.push(doc);
        let new_tests = extract_tests_from_file(path)?;
        doc_tests.push(new_tests);
    }
    Ok(DocTestSuite { doc_tests })
}

enum Buffer {
    None,
    Code(Vec<String>),
    Header(String),
}

fn extract_tests_from_file(path: &Path) -> Result<DocTest, IoError> {
    let mut file = File::open(path)?;
    let s = &mut String::new();
    file.read_to_string(s)?;

    let file_stem = &sanitize_test_name(path.file_stem().unwrap().to_str().unwrap());

    let tests = extract_tests_from_string(s, file_stem);

    let templates = load_templates(path)?;

    Ok(DocTest {
        path: path.to_owned(),
        old_template: tests.1,
        tests: tests.0,
        templates,
    })
}

fn extract_tests_from_string(s: &str, file_stem: &str) -> (Vec<Test>, Option<String>) {
    let mut tests = Vec::new();
    let mut buffer = Buffer::None;
    let mut parser = Parser::new(s);
    let mut section = None;
    let mut code_block_start = 0;
    // Oh this isn't actually a test but a legacy template
    let mut old_template = None;

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
                    if buf.is_empty() {
                        code_block_start = line_number;
                    }
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
                            format!("{}_sect_{}_line_{}", file_stem, section, code_block_start)
                        } else {
                            format!("{}_line_{}", file_stem, code_block_start)
                        };
                        tests.push(Test {
                            name,
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
    (tests, old_template)
}

fn load_templates(path: &Path) -> Result<HashMap<String, String>, IoError> {
    let file_name = format!(
        "{}.skt.md",
        path.file_name().expect("no file name").to_string_lossy()
    );
    let path = path.with_file_name(&file_name);
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let mut map = HashMap::new();

    let mut file = File::open(path)?;
    let s = &mut String::new();
    file.read_to_string(s)?;
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
    s.to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii() && ch.is_alphanumeric() {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
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
                    let template = doc_test.templates.get(t).unwrap_or_else(|| {
                        panic!("template {} not found for {}", t, doc_test.path.display())
                    });
                    create_test_runner(config, &Some(template.to_string()), test)?
                } else {
                    create_test_runner(config, &doc_test.old_template, test)?
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
fn clean_omitted_line(line: &str) -> &str {
    // XXX To silence depreciation warning of trim_left and not bump rustc
    // requirement upto 1.30 (for trim_start) we roll our own trim_left :(
    let trimmed = if let Some(pos) = line.find(|c: char| !c.is_whitespace()) {
        &line[pos..]
    } else {
        line
    };

    if trimmed.starts_with("# ") {
        &trimmed[2..]
    } else if line.trim() == "#" {
        // line consists of single "#" which might not be followed by newline on windows
        &trimmed[1..]
    } else {
        line
    }
}

/// Creates the Rust code that this test will be operating on.
fn create_test_input(lines: &[String]) -> String {
    lines
        .iter()
        .map(|s| clean_omitted_line(s).to_owned())
        .collect()
}

fn create_test_runner(
    config: &Config,
    template: &Option<String>,
    test: &Test,
) -> Result<String, IoError> {
    let template = template.clone().unwrap_or_else(|| String::from("{}"));
    let test_text = create_test_input(&test.text);

    let mut s: Vec<u8> = Vec::new();
    if test.ignore {
        writeln!(s, "#[ignore]")?;
    }
    if test.should_panic {
        writeln!(s, "#[should_panic]")?;
    }

    writeln!(s, "#[test] fn {}() {{", test.name)?;
    writeln!(
        s,
        "    let s = &format!(r####\"\n{}\"####, r####\"{}\"####);",
        template, test_text
    )?;

    // if we are not running, just compile the test without running it
    if test.no_run {
        writeln!(
            s,
            "    skeptic::rt::compile_test(r#\"{}\"#, r#\"{}\"#, r#\"{}\"#, s);",
            config.root_dir.to_str().unwrap(),
            config.out_dir.to_str().unwrap(),
            config.target_triple
        )?;
    } else {
        writeln!(
            s,
            "    skeptic::rt::run_test(r#\"{}\"#, r#\"{}\"#, r#\"{}\"#, s);",
            config.root_dir.to_str().unwrap(),
            config.out_dir.to_str().unwrap(),
            config.target_triple
        )?;
    }

    writeln!(s, "}}")?;
    writeln!(s)?;

    Ok(String::from_utf8(s).unwrap())
}

fn write_if_contents_changed(name: &Path, contents: &str) -> Result<(), IoError> {
    // Can't open in write mode now as that would modify the last changed timestamp of the file
    match File::open(name) {
        Ok(mut file) => {
            let mut current_contents = String::new();
            file.read_to_string(&mut current_contents)?;
            if current_contents == contents {
                // No change avoid writing to avoid updating the timestamp of the file
                return Ok(());
            }
        }
        Err(ref err) if err.kind() == io::ErrorKind::NotFound => (),
        Err(err) => return Err(err),
    }
    let mut file = File::create(name)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate unindent;

    use self::unindent::unindent;
    use super::*;

    #[test]
    fn test_omitted_lines() {
        let lines = unindent(
            r###"
            # use std::collections::BTreeMap as Map;
            #
            #[allow(dead_code)]
            fn main() {
                let map = Map::new();
                #
                # let _ = map;
            }"###,
        );

        let expected = unindent(
            r###"
            use std::collections::BTreeMap as Map;

            #[allow(dead_code)]
            fn main() {
                let map = Map::new();

            let _ = map;
            }
            "###,
        );

        assert_eq!(create_test_input(&get_lines(lines)), expected);
    }

    #[test]
    fn test_markdown_files_of_directory() {
        let files = vec![
            "../tests/hashtag-test.md",
            "../tests/section-names.md",
            "../tests/should-panic-test.md",
        ];
        let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        assert_eq!(markdown_files_of_directory("../tests/"), files);
    }

    #[test]
    fn test_sanitization_of_testnames() {
        assert_eq!(sanitize_test_name("My_Fun"), "my_fun");
        assert_eq!(sanitize_test_name("__my_fun_"), "my_fun");
        assert_eq!(sanitize_test_name("^$@__my@#_fun#$@"), "my_fun");
        assert_eq!(
            sanitize_test_name("my_long__fun___name___with____a_____lot______of_______spaces",),
            "my_long_fun_name_with_a_lot_of_spaces"
        );
        assert_eq!(sanitize_test_name("Löwe 老虎 Léopard"), "l_we_l_opard");
    }

    #[test]
    fn line_numbers_displayed_are_for_the_beginning_of_each_code_block() {
        let lines = unindent(
            r###"
            Rust code that should panic when running it.

            ```rust,should_panic",/
            fn main() {
                panic!(\"I should panic\");
            }
            ```

            Rust code that should panic when compiling it.

            ```rust,no_run,should_panic",//
            fn add(a: u32, b: u32) -> u32 {
                a + b
            }

            fn main() {
                add(1);
            }
            ```"###,
        );

        let tests =
            extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));

        let test_names: Vec<String> = tests
            .0
            .into_iter()
            .map(|test| get_line_number_from_test_name(test))
            .collect();

        assert_eq!(test_names, vec!["3", "11"]);
    }

    #[test]
    fn line_numbers_displayed_are_for_the_beginning_of_each_section() {
        let lines = unindent(
            r###"
            ## Test Case  Names   With    weird     spacing       are        generated      without        error.

            ```rust", /
            struct Person<'a>(&'a str);
            fn main() {
              let _ = Person(\"bors\");
            }
            ```

            ## !@#$ Test Cases )(() with {}[] non alphanumeric characters ^$23 characters are \"`#`\" generated correctly @#$@#$  22.

            ```rust", //
            struct Person<'a>(&'a str);
            fn main() {
              let _ = Person(\"bors\");
            }
            ```

            ## Test cases with non ASCII ö_老虎_é characters are generated correctly.

            ```rust",//
            struct Person<'a>(&'a str);
            fn main() {
              let _ = Person(\"bors\");
            }
            ```"###,
        );

        let tests =
            extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));

        let test_names: Vec<String> = tests
            .0
            .into_iter()
            .map(|test| get_line_number_from_test_name(test))
            .collect();

        assert_eq!(test_names, vec!["3", "12", "21"]);
    }

    #[test]
    fn old_template_is_returned_for_old_skeptic_template_format() {
        let lines = unindent(
            r###"
            ```rust,skeptic-template
            ```rust,ignore
            use std::path::PathBuf;

            fn main() {{
                {}
            }}
            ```
            ```
            "###,
        );
        let expected = unindent(
            r###"
            ```rust,ignore
            use std::path::PathBuf;

            fn main() {{
                {}
            }}
            "###,
        );
        let tests =
            extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));
        assert_eq!(tests.1, Some(expected));
    }

    #[test]
    fn old_template_is_not_returned_if_old_skeptic_template_is_not_specified() {
        let lines = unindent(
            r###"
            ```rust", /
            struct Person<'a>(&'a str);
            fn main() {
              let _ = Person(\"bors\");
            }
            ```
            "###,
        );
        let tests =
            extract_tests_from_string(&create_test_input(&get_lines(lines)), &String::from("blah"));
        assert_eq!(tests.1, None);
    }

    fn get_line_number_from_test_name(test: Test) -> String {
        String::from(
            test.name
                .split('_')
                .last()
                .expect("There were no underscores!"),
        )
    }

    fn get_lines(lines: String) -> Vec<String> {
        lines
            .split('\n')
            .map(|string_slice| format!("{}\n", string_slice)) //restore line endings since they are removed by split.
            .collect()
    }
}
