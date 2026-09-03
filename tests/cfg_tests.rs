use parser_vm::CfgParser;

#[test]
fn test_cfg_basic() {
    let parser = CfgParser::new(r#"S -> "a""#).unwrap();
    assert!(parser.is_match("a"));
    assert!(!parser.is_match("b"));
}

#[test]
fn test_cfg_alternation() {
    let parser = CfgParser::new(r#"S -> "a" | "b" | "c""#).unwrap();
    assert!(parser.is_match("a"));
    assert!(parser.is_match("b"));
    assert!(parser.is_match("c"));
    assert!(!parser.is_match("d"));
}

#[test]
fn test_cfg_sequence() {
    let parser = CfgParser::new(r#"S -> "a" "b" "c""#).unwrap();
    assert!(parser.is_match("a b c"));
    assert!(!parser.is_match("a b"));
}

#[test]
fn test_cfg_multiple_productions() {
    let parser = CfgParser::new(r#"E -> E "+" T | T; T -> "t""#).unwrap();
    assert!(parser.is_match("t"));
}

#[test]
fn test_cfg_nested_with_parens() {
    let parser = CfgParser::new(
        r#"SExp -> "(" SExpList ")" | Atom; SExpList -> SExp SExpList | SExp; Atom -> "foo" | "bar""#,
    ).unwrap();
    assert!(parser.is_match("foo"));
    assert!(parser.is_match("( foo )"));
    assert!(parser.is_match("( foo bar )"));
    assert!(parser.is_match("( foo ( bar ) )"));
    assert!(!parser.is_match("( foo"));
}

#[test]
fn test_cfg_arithmetic_with_parens() {
    let parser = CfgParser::new(
        r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | NUM; NUM -> "1" | "2""#,
    )
    .unwrap();
    assert!(parser.is_match("1"));
    assert!(parser.is_match("1 + 2"));
    assert!(parser.is_match("1 * 2"));
    assert!(parser.is_match("( 1 + 2 )"));
    assert!(!parser.is_match("1 +"));
}

#[test]
fn test_cfg_epsilon() {
    let parser = CfgParser::new("S -> epsilon").unwrap();
    assert!(parser.is_match(""));
    assert!(parser.is_match("   "));
    assert!(!parser.is_match("a"));
}

#[test]
fn test_cfg_unquoted_is_nonterminal() {
    // An unquoted symbol is a non-terminal: `a` on the RHS is undefined, so the
    // input `a` (a bare token) must not match.
    let parser = CfgParser::new("S -> a").unwrap();
    assert!(!parser.is_match("a"));
}

#[test]
fn test_parser_word_and_number_runs() {
    let parser = CfgParser::new(r#"S -> Word Num; Word -> "[a-z]+" ; Num -> "[0-9]+""#).unwrap();
    assert!(parser.is_match("abc42"));
    assert!(!parser.is_match("abc"));
    assert!(!parser.is_match("42abc"));
}

#[test]
fn test_parser_any_string_token() {
    let parser = CfgParser::new(r#"S -> "any""#).unwrap();
    assert!(parser.is_match(r#""any content at all""#));
    assert!(!parser.is_match("abc"));
    assert!(!parser.is_match(r#""unclosed"#));
}

#[test]
fn test_cfg_new_errors() {
    assert!(CfgParser::new("").is_err(), "empty grammar must error");
    assert!(
        CfgParser::new("S a").is_err(),
        "production without -> must error"
    );
}
