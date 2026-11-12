//! Node identity as in Gelatin: name plus creation attributes (in any order);
//! the most recently created match wins; `add_attribute` keeps the identity.

use rustine::exec::{execute_tree, serialize_tree, RuntimeFormat};
use rustine::parser::{lexer::lex, syntax::parse_gel_document};
use serde_json::{json, Value};

const GEL: &str = r#"grammar input:
    match /f (\w+)\n/:
        out.create('$1')
    match /u (\w+)\n/:
        out.add_attribute('user', 'role', '$1')
    match /t (\w+)\n/:
        out.add('user', '$1')
    match /e (\w+)\n/:
        out.enter('user')
        out.add('child', '$1')
    match /R (\w+)\n/:
        out.replace('user', '$1')
    match /c (\w+)\n/:
        out.create('item', '$1')
    match /a (\w+)\n/:
        out.add('item', '$1')
    match /r (\w+)\n/:
        out.replace('item', '$1')
    match /p\n/:
        out.add('n?a="1"&b="2"', 'p')
    match /q\n/:
        out.add('n?b="2"&a="1"', 'q')
    match /d\n/:
        out.add('dup?k="1"&k="2"')
"#;

/// Eight other children first, so that the parent also looks children up by hash.
const SIBLINGS: &str = "f s1\nf s2\nf s3\nf s4\nf s5\nf s6\nf s7\nf s8\n";

fn run(input: &str) -> Value {
    let mut doc = parse_gel_document(&lex(GEL).unwrap()).unwrap();
    doc.compile_regexes();
    let exec = execute_tree(&doc, "input", input).unwrap();
    assert_eq!(exec.error, None);
    serde_json::from_str(&serialize_tree(&exec, RuntimeFormat::Json)).unwrap()
}

/// `run` with and without many siblings; the result without the sibling nodes.
fn both(input: &str) -> Value {
    let few = run(input);
    let mut many = run(&format!("{SIBLINGS}{input}"));
    for i in 1..=8 {
        many.as_object_mut().unwrap().remove(&format!("s{i}"));
    }
    assert_eq!(few, many, "with eight siblings");
    few
}

#[test]
fn add_attribute_keeps_the_identity() {
    assert_eq!(
        both("u admin\nt x\n"),
        json!({"user": {"@role": "admin", "#text": "x"}})
    );
    assert_eq!(
        both("u admin\ne x\n"),
        json!({"user": {"@role": "admin", "child": {"#text": "x"}}})
    );
}

#[test]
fn the_newest_match_wins() {
    assert_eq!(
        both("c 1\nc 2\na x\n"),
        json!({"item": [{"#text": "1"}, {"#text": "2x"}]})
    );
    let many: String = (1..=9).map(|i| format!("c {i}\n")).collect();
    let items = run(&format!("{many}a x\n"))["item"].clone();
    assert_eq!(items[7], json!({"#text": "8"}));
    assert_eq!(items[8], json!({"#text": "9x"}));
}

#[test]
fn attribute_order_does_not_matter() {
    assert_eq!(both("p\nq\n"), json!({"n": {"@a": "1", "@b": "2", "#text": "pq"}}));
}

#[test]
fn replace_sets_the_text_of_the_node_add_finds() {
    assert_eq!(
        both("c 1\nc 2\nr z\n"),
        json!({"item": [{"#text": "1"}, {"#text": "z"}]})
    );
    assert_eq!(
        both("u admin\ne x\nt y\nR z\n"),
        json!({"user": {"@role": "admin", "child": {"#text": "x"}, "#text": "z"}})
    );
}

#[test]
fn a_repeated_attribute_key_keeps_its_first_value() {
    assert_eq!(both("d\n"), json!({"dup": {"@k": "1"}}));
}
