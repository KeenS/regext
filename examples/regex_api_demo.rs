use parser_vm::{CfgParser, Regex};

fn main() {
    // 1. Regex literals (whole-input matching)
    let re = Regex::new("test").unwrap();
    assert!(re.is_match("test"), "exact match");
    assert!(!re.is_match("a test b"), "whole-input semantics: substring must not match");
    assert!(!re.is_match("abc"), "non-match");
    assert!(re.find("a test b").is_none(), "find returns None without a full match");
    assert_eq!(re.find("test").map(|m| (m.start, m.end)), Some((0, 4)), "find span");

    // 2. Context-free mode: quoted terminals, non-terminals bare
    let parser = CfgParser::new(r#"E -> E "+" NUM | NUM; NUM -> "digit""#).unwrap();
    assert!(parser.is_match("digit"), "single NUM");
    assert!(parser.is_match("digit + digit"), "recursive E");
    assert!(!parser.is_match("digit +"), "trailing operator must be rejected");

    // 3. Captures
    let re = Regex::new("hello").unwrap();
    let caps = re.captures("hello").expect("captures on match");
    assert_eq!(caps.matches.len(), 1, "capture count");
    assert!(re.captures("hello world").is_none(), "whole-input semantics: no capture on partial");

    println!("regex_api_demo: 8 assertions passed");
}
