use parser_vm::Regex;

fn main() {
    // The Regex API mirrors the regex crate surface, but the engine matches the
    // ENTIRE input (no substring search yet).
    let re = Regex::new("test").unwrap();

    assert!(re.is_match("test"), "exact match must succeed");
    assert!(!re.is_match("abc"), "non-matching input must fail");
    assert!(!re.is_match("a test b"), "whole-input semantics: substring must not match");
    assert!(!re.is_match("testing"), "whole-input semantics: longer input must not match");

    let m = re.find("test").expect("find must locate the match");
    assert_eq!((m.start, m.end), (0, 4), "match spans the whole input");
    assert!(re.find("abc").is_none(), "find must return None on non-match");

    let caps = re.captures("test").expect("captures must succeed on match");
    assert_eq!(caps.matches.len(), 1, "one match capture");
    assert!(re.captures("abc").is_none(), "captures must return None on non-match");

    println!("regex_demo: 7 assertions passed");
}
