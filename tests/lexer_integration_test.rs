use parser_vm::CfgParser;

#[test]
fn test_lexer_with_terminals() {
    let parser = CfgParser::new("E -> E + T | T; T -> T * F | F; F -> ( E ) | num").unwrap();
    assert!(parser.is_match("num"));
    assert!(parser.is_match("num + num"));
    assert!(parser.is_match("( num )"));
    assert!(!parser.is_match("num +"));
}

#[test]
fn test_lexer_character_matching() {
    let parser = CfgParser::new("S -> ( S ) | a").unwrap();
    assert!(parser.is_match("a"));
    assert!(parser.is_match("( a )"));
    assert!(parser.is_match("( ( a ) )"));
}

#[test]
fn test_lexer_with_operators() {
    let parser = CfgParser::new("S -> S + S | num").unwrap();
    assert!(parser.is_match("num"));
    assert!(parser.is_match("num + num"));
    assert!(parser.is_match("num + num + num"));
}
