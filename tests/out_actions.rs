use rustine::parse_and_run;

#[test]
fn test_out_add_and_replace_and_attr() {
    let gel = "grammar main:\n    match 'foo':\n        out.create('a/b/c','one')\n        out.add('a/b/c','+two')\n        out.replace('a/b/c','only')\n        out.add_attribute('a/b','k','v')\n        out.set_root_name('root')\n";
    let res = parse_and_run(gel, "main", "foo").expect("run");
    // attribute present as @k
    assert!(res.contains("\"@k\": \"v\""), "attribute @k missing: {res}");
    // replaced leaf has only final text 'only'
    assert!(res.contains("\"#text\": \"only\""), "replaced text missing: {res}");
    // concatenated 'one+two' should not appear (replaced by 'only')
    assert!(!res.contains("one+two"), "old text should be replaced: {res}");
}
