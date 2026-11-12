//! Tree YAML: block mappings with the structure of the JSON output.

use rustine::exec::{execute_tree, serialize_tree, RuntimeFormat};
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

fn yaml(gel: &str, input: &str) -> String {
    let mut doc = parse_gel_document(&lex(gel).unwrap()).unwrap();
    doc.preserve_case = true;
    doc.compile_regexes();
    serialize_tree(&execute_tree(&doc, "input", input).unwrap(), RuntimeFormat::Yaml).unwrap()
}

const PAIRS: &str = "grammar input:\n    match /([^\\t\\n]*)\\t([^\\n]*)\\n/:\n        out.add('$1', '$2')\n";

#[test]
fn sequences_and_empty_nodes() {
    let gel = "grammar input:\n    match /e\\n/:\n        out.create('list/empty')\n    match /n (\\w+) (\\w+)\\n/:\n        out.open('nested?id=\"$1\"')\n        out.create('item', '$1')\n        out.create('item', '$2')\n";
    assert_eq!(
        yaml(gel, "e\ne\nn a b\nn c d\n"),
        concat!(
            "list:\n  empty:\n  - {}\n  - {}\n",
            "nested:\n",
            "- '@id': a\n  item:\n  - '#text': a\n  - '#text': b\n",
            "- '@id': c\n  item:\n  - '#text': c\n  - '#text': d\n",
        )
    );
}

#[test]
fn empty_tree() {
    assert_eq!(yaml("grammar input:\n    skip /.*/\n", "x"), "{}\n");
}

#[test]
fn quoting() {
    let input = "true\tyes\n042\t1.5\nplain\tit's fine\nk\t a b \nq\t'x' and \"y\"\nt\ta\tb\nx\x01\tv\u{2028}\n";
    assert_eq!(
        yaml(PAIRS, input),
        concat!(
            "'true':\n  '#text': 'yes'\n",
            "'042':\n  '#text': '1.5'\n",
            "plain:\n  '#text': it's fine\n",
            "k:\n  '#text': ' a b '\n",
            "q:\n  '#text': '''x'' and \"y\"'\n",
            "t:\n  '#text': \"a\\tb\"\n",
            "\"x\\x01\":\n  '#text': \"v\\u2028\"\n",
        )
    );
}

#[test]
fn attribute_keys_are_quoted() {
    let gel = "grammar input:\n    match /(.*)\\n/:\n        out.add('node?$1=\"v\"')\n";
    assert_eq!(yaml(gel, "it's\n"), "node:\n  '@it''s': v\n");
}

#[test]
fn long_keys_are_explicit() {
    let key = "k".repeat(300);
    assert_eq!(
        yaml(PAIRS, &format!("{key}\tv\n")),
        format!("? {key}\n:\n  '#text': v\n")
    );
}
