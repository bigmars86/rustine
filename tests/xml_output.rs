//! XML output: escaping that keeps every character, and errors for names and
//! characters that XML 1.0 cannot represent.

use rustine::exec::{execute_tree, serialize_tree, RuntimeFormat};
use rustine::parser::{lexer::lex, syntax::parse_gel_document};

const PAIRS: &str = "grammar input:\n    match /([^\\t\\n]*)\\t([^\\n]*)\\n/:\n        out.add('$1', '$2')\n";
const ATTR: &str = "grammar input:\n    match /([^\\t\\n]*)\\t([^\\n]*)\\n/:\n        out.add('node?$1=\"$2\"')\n";

fn xml(gel: &str, input: &str) -> rustine::Result<String> {
    let mut doc = parse_gel_document(&lex(gel).unwrap()).unwrap();
    doc.preserve_case = true;
    doc.compile_regexes();
    let exec = execute_tree(&doc, "input", input).unwrap();
    assert_eq!(exec.error, None);
    assert!(
        serialize_tree(&exec, RuntimeFormat::Json).is_ok(),
        "JSON takes any name"
    );
    serialize_tree(&exec, RuntimeFormat::Xml)
}

fn error(gel: &str, input: &str) -> String {
    xml(gel, input).unwrap_err().to_string()
}

#[test]
fn escapes_keep_every_character() {
    assert_eq!(
        xml(PAIRS, "t\t<a> & \"q\" 'x'\r\tz\n").unwrap(),
        "<xml>\n  <t>&lt;a&gt; &amp; \"q\" 'x'&#13;\tz</t>\n</xml>\n"
    );
    assert_eq!(
        xml(ATTR, "k\t<a> & \"q\" 'x'\r\t\n").unwrap(),
        "<xml>\n  <node k=\"&lt;a&gt; &amp; &quot;q&quot; 'x'&#13;&#9;\"/>\n</xml>\n"
    );
}

#[test]
fn names_must_be_xml_names() {
    assert!(xml(PAIRS, "gr\u{f6}\u{df}e_1.x-y\tv\n").is_ok());
    assert!(error(PAIRS, "042\tv\n").contains("not an XML element name: \"042\""));
    assert!(error(PAIRS, "a:b\tv\n").contains("not an XML element name: \"a:b\""));
    assert!(error(ATTR, "1k\tv\n").contains("not an XML attribute name: \"1k\" (element <node>)"));
}

#[test]
fn characters_outside_xml_1_0_are_errors() {
    assert!(error(PAIRS, "t\ta\u{1}b\n").contains("XML 1.0 cannot represent U+0001 in the text of <t>"));
    assert!(error(PAIRS, "t\ta\u{ffff}\n").contains("U+FFFF in the text of <t>"));
    assert!(error(ATTR, "k\tv\u{1b}\n").contains("U+001B in attribute \"k\" of <node>"));
}

#[test]
fn a_repeated_attribute_keeps_its_first_position_and_last_value() {
    let gel = "grammar input:\n    match /x\\n/:\n        out.add_attribute('.', 'k', '1')\n        out.add_attribute('.', 'x', '2')\n        out.add_attribute('.', 'k', '3')\n";
    assert_eq!(xml(gel, "x\n").unwrap(), "<xml k=\"3\" x=\"2\">\n</xml>\n");
}
