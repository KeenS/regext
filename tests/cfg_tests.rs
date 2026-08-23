use parser_vm::CfgParser;

#[test]
fn test_cfg_basic() {
    let parser = CfgParser::new("S -> a").unwrap();
    assert!(parser.is_match("a"));
    assert!(!parser.is_match("b"));
}

#[test]
fn test_cfg_alternation() {
    let parser = CfgParser::new("S -> a | b | c").unwrap();
    assert!(parser.is_match("a"));
    assert!(parser.is_match("b"));
    assert!(parser.is_match("c"));
    assert!(!parser.is_match("d"));
}

#[test]
fn test_cfg_sequence() {
    let parser = CfgParser::new("S -> a b c").unwrap();
    assert!(parser.is_match("a b c"));
    assert!(!parser.is_match("a b"));
}

#[test]
fn test_cfg_multiple_productions() {
    let parser = CfgParser::new("E -> E + T | T; T -> t").unwrap();
    assert!(parser.is_match("t"));
}

#[test]
fn test_cfg_nested_with_parens() {
    let parser = CfgParser::new("SExp -> ( SExpList ) | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar").unwrap();
    assert!(parser.is_match("foo"));
    assert!(parser.is_match("( foo )"));
    assert!(parser.is_match("( foo bar )"));
    assert!(parser.is_match("( foo ( bar ) )"));
    assert!(!parser.is_match("( foo"));
}

#[test]
fn test_cfg_arithmetic_with_parens() {
    let parser = CfgParser::new("E -> E + T | T; T -> T * F | F; F -> ( E ) | NUM; NUM -> 1 | 2").unwrap();
    assert!(parser.is_match("1"));
    assert!(parser.is_match("1 + 2"));
    assert!(parser.is_match("1 * 2"));
    assert!(parser.is_match("( 1 + 2 )"));
    assert!(!parser.is_match("1 +"));
}
