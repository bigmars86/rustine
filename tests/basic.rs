#![cfg(feature = "python")]
use rustine::bridge::parse_gel_to_json;

#[test]
fn json_parse_dummy() {
    let out = parse_gel_to_json("define ws /\\s+/").unwrap();
    assert!(out.contains("defines"));
    assert!(out.contains("ws"));
}
