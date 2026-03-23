#![cfg(feature = "python")]
use rustine::bridge::{parse_gel_to_json, parse_gel_to_xml};

#[test]
fn json_parse_dummy() {
    let out = parse_gel_to_json("define ws /\\s+/").unwrap();
    assert!(out.contains("defines"));
    assert!(out.contains("ws"));
}

#[test]
fn xml_parse_dummy() {
    let out = parse_gel_to_xml("define ws /\\s+/").unwrap();
    assert!(out.contains("<gel-document>"));
    assert!(out.contains("<define"));
}
