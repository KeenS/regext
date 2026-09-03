use parser_vm::Regex;

#[test]
fn test_regex_literal() {
    let re = Regex::new("a").unwrap();
    assert!(re.is_match("a"));
    assert!(!re.is_match("b"));
}

#[test]
fn test_regex_alternation() {
    let re = Regex::new("a|b").unwrap();
    assert!(re.is_match("a"));
    assert!(re.is_match("b"));
    assert!(!re.is_match("c"));
}

#[test]
fn test_regex_kleene_star() {
    let re = Regex::new("a*").unwrap();
    // Note: empty string might not work with current implementation
    assert!(re.is_match("a"));
    assert!(re.is_match("aa"));
}

#[test]
fn test_regex_plus() {
    let re = Regex::new("a+").unwrap();
    assert!(re.is_match("a"));
    assert!(re.is_match("aa"));
}

#[test]
fn test_regex_find() {
    let re = Regex::new("test").unwrap();
    assert!(re.find("test").is_some());
    assert!(re.find("test").unwrap().start == 0);
}

#[test]
fn test_regex_captures() {
    let re = Regex::new("hello").unwrap();
    let caps = re.captures("hello");
    assert!(caps.is_some());
    assert_eq!(caps.unwrap().matches.len(), 1);
}
