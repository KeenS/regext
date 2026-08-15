use parser_vm::Regex;

#[test]
fn test_regex_new_api() {
    let re = Regex::new(r"\d+").unwrap();
    // Just test it doesn't panic
    assert!(true);
}

#[test]
fn test_regex_is_match() {
    let re = Regex::new(r"test").unwrap();
    assert!(re.is_match("test"));
    assert!(re.is_match("a test b"));
    assert!(!re.is_match("abc"));
}

#[test]
fn test_regex_find() {
    let re = Regex::new(r"test").unwrap();
    let m = re.find("test");
    assert!(m.is_some());
}

#[test]
fn test_regex_captures() {
    let re = Regex::new(r"hello").unwrap();
    let caps = re.captures("hello world");
    assert!(caps.is_some());
}

#[test]
fn test_regex_simple_pattern() {
    let re = Regex::new("hello").unwrap();
    assert!(re.is_match("hello"));
    assert!(re.is_match("hello world"));
    assert!(!re.is_match("world"));
}
