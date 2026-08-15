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
    let parser = CfgParser::new("E -> E plus T | T; T -> t").unwrap();
    assert!(parser.is_match("t"));
}

#[test]
fn test_cfg_nested() {
    let parser = CfgParser::new("SExp -> lparen SExpList rparen | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar").unwrap();
    assert!(parser.is_match("foo"));
    assert!(parser.is_match("lparen foo rparen"));
    assert!(parser.is_match("lparen foo bar rparen"));
    assert!(!parser.is_match("lparen foo"));
}

#[test]
fn test_cfg_arithmetic() {
    let parser = CfgParser::new("E -> E plus T | T; T -> T star F | F; F -> lparen E rparen | NUM; NUM -> 1 | 2").unwrap();
    assert!(parser.is_match("1"));
    assert!(parser.is_match("1 plus 2"));
    assert!(parser.is_match("1 star 2"));
    assert!(parser.is_match("lparen 1 plus 2 rparen"));
    assert!(!parser.is_match("1 plus"));
}
