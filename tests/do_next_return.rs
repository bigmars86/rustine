use rustine::parse_and_run;

// Grammar: first match consumes foo and triggers out.create then do.next to skip remaining actions in block and restart loop
// Second match consumes remaining bar then do.return stops further scanning
#[test]
fn test_do_next_and_do_return() {
    let grammar = "grammar g:\n    match /foo/:\n        out.create(\"root\")\n        do.next()\n        out.add(\"root/should_not_appear\")\n    match /bar/:\n        out.add(\"root/bar\")\n        do.return()\n        out.add(\"root/after_return\")\n";
    let input = "foobarzzz"; // after return, trailing zzz should not be processed
    let json = parse_and_run(grammar, "g", input).expect("run");

    // root created, bar added, but should_not_appear and after_return absent
    assert!(json.contains("\"root\""), "expected root node: {json}");
    assert!(json.contains("\"bar\""), "expected bar node: {json}");
    assert!(
        !json.contains("should_not_appear"),
        "should_not_appear must be absent: {json}"
    );
    assert!(!json.contains("after_return"), "after_return must be absent: {json}");
    // consumed should be length of 'foo' + 'bar' = 6
    assert!(json.contains("\"consumed\": 6"), "consumed length mismatch: {json}");
}
