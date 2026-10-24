use std::collections::HashSet;

use RegExt::lexer::{Lexer, Token};
use RegExt::{CfgParser, Match};

fn terminal_set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_lexer_with_terminals() {
    let parser =
        CfgParser::new(r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | "num""#).unwrap();
    assert!(parser.is_match("num"));
    assert!(parser.is_match("num + num"));
    assert!(parser.is_match("( num )"));
    assert!(!parser.is_match("num +"));
}

#[test]
fn test_lexer_character_matching() {
    let parser = CfgParser::new(r#"S -> "(" S ")" | "a""#).unwrap();
    assert!(parser.is_match("a"));
    assert!(parser.is_match("( a )"));
    assert!(parser.is_match("( ( a ) )"));
}

#[test]
fn test_lexer_with_operators() {
    let parser = CfgParser::new(r#"S -> S "+" S | "num""#).unwrap();
    assert!(parser.is_match("num"));
    assert!(parser.is_match("num + num"));
    assert!(parser.is_match("num + num + num"));
}

#[test]
fn test_tokenizer_multi_char_terminals() {
    let lexer = Lexer::new(terminal_set(&["num", "+", "*"]));
    let tokens = lexer.tokenize("num + num * num");
    assert_eq!(
        tokens,
        vec![
            Token::Terminal("num".to_string()),
            Token::Terminal("+".to_string()),
            Token::Terminal("num".to_string()),
            Token::Terminal("*".to_string()),
            Token::Terminal("num".to_string()),
            Token::EOF,
        ]
    );
}

#[test]
fn test_tokenizer_skips_whitespace() {
    let lexer = Lexer::new(terminal_set(&["a", "b"]));
    let tokens = lexer.tokenize("  a b  ");
    assert_eq!(
        tokens,
        vec![
            Token::Terminal("a".to_string()),
            Token::Terminal("b".to_string()),
            Token::EOF,
        ]
    );
}

#[test]
fn test_tokenizer_prefers_longest_terminal() {
    let lexer = Lexer::new(terminal_set(&["num", "num2"]));
    let tokens = lexer.tokenize("num2 num");
    assert_eq!(
        tokens,
        vec![
            Token::Terminal("num2".to_string()),
            Token::Terminal("num".to_string()),
            Token::EOF,
        ]
    );
}

#[test]
fn test_tokenizer_char_class() {
    let lexer = Lexer::new(terminal_set(&["[1-5]"]));
    let tokens = lexer.tokenize("135");
    let mut expected = vec![Token::Terminal("[1-5]".to_string()); 3];
    expected.push(Token::EOF);
    assert_eq!(tokens, expected);
}

#[test]
fn test_tokenizer_char_class_range_excludes_hyphen() {
    let lexer = Lexer::new(terminal_set(&["[1-5]"]));
    let tokens = lexer.tokenize("-");
    assert_eq!(tokens, vec![Token::Terminal("-".to_string()), Token::EOF]);
}

#[test]
fn test_tokenizer_unknown_char_emitted_raw() {
    let lexer = Lexer::new(terminal_set(&["a"]));
    let tokens = lexer.tokenize("z");
    assert_eq!(tokens, vec![Token::Terminal("z".to_string()), Token::EOF]);

    // With an OTHER terminal present, an unmatched character uses that name.
    let lexer = Lexer::new(terminal_set(&["\u{0}other", "a"]));
    assert_eq!(
        lexer.tokenize("az"),
        vec![
            Token::Terminal("a".to_string()),
            Token::Terminal("\u{0}other".to_string()),
            Token::EOF,
        ]
    );
}

#[test]
fn test_parser_with_char_class() {
    let parser = CfgParser::new(r#"S -> "[1-5]""#).unwrap();
    assert!(parser.is_match("3"));
    assert!(parser.is_match(" 3 "));
    assert!(!parser.is_match("-"));
    assert!(!parser.is_match("0"));
    assert!(!parser.is_match("33"));
    assert!(!parser.is_match("3 4"));
    assert_eq!(parser.find("x3y"), Some(Match::new(1, 2)));
}

#[test]
fn test_parser_literal_preferred_over_char_class() {
    let parser = CfgParser::new(r#"S -> "ab" | "[a-c]""#).unwrap();
    assert!(parser.is_match("ab"));
    assert!(parser.is_match("a"));
    assert!(parser.is_match("b"));
    assert!(parser.is_match("c"));
    assert!(!parser.is_match("d"));
    assert!(!parser.is_match("abc"));
    assert_eq!(parser.find("xab"), Some(Match::new(1, 3)));
    assert_eq!(parser.find("xc"), Some(Match::new(1, 2)));
}

#[test]
fn test_tokenizer_class_run_maximal_munch() {
    let lexer = Lexer::new(terminal_set(&["[0-9]+"]));
    let tokens = lexer.tokenize("123 45");
    assert_eq!(
        tokens,
        vec![
            Token::Terminal("[0-9]+".to_string()),
            Token::Terminal("[0-9]+".to_string()),
            Token::EOF,
        ]
    );
}

#[test]
fn test_tokenizer_class_run_stops_at_non_member() {
    let lexer = Lexer::new(terminal_set(&["[a-z]+", "[0-9]+"]));
    let tokens = lexer.tokenize("abc123");
    assert_eq!(
        tokens,
        vec![
            Token::Terminal("[a-z]+".to_string()),
            Token::Terminal("[0-9]+".to_string()),
            Token::EOF,
        ]
    );
}

#[test]
fn test_tokenizer_quoted_string_keeps_structural_chars() {
    let lexer = Lexer::new(terminal_set(&["any"]));
    let tokens = lexer.tokenize(r#""hello world, {yes} [no]""#);
    assert_eq!(tokens, vec![Token::Terminal("any".to_string()), Token::EOF]);
}

#[test]
fn test_tokenizer_quoted_string_escapes() {
    let lexer = Lexer::new(terminal_set(&["any"]));
    let tokens = lexer.tokenize(r#""a \"quoted\" word""#);
    assert_eq!(tokens, vec![Token::Terminal("any".to_string()), Token::EOF]);
}

#[test]
fn test_tokenizer_longest_match_across_kinds() {
    let lexer = Lexer::new(terminal_set(&["hello!", "[a-z]+"]));
    assert_eq!(
        lexer.tokenize("hello!"),
        vec![Token::Terminal("hello!".to_string()), Token::EOF]
    );
    assert_eq!(
        lexer.tokenize("hello"),
        vec![Token::Terminal("[a-z]+".to_string()), Token::EOF]
    );

    let lexer = Lexer::new(terminal_set(&["[a-z]+", "[a-c]+"]));
    assert_eq!(
        lexer.tokenize("abcd"),
        vec![Token::Terminal("[a-z]+".to_string()), Token::EOF,]
    );
    assert_eq!(
        lexer.tokenize("abc"),
        vec![Token::Terminal("[a-c]+".to_string()), Token::EOF]
    );

    // Equal length: a literal outranks a class run. A longer run still wins.
    let lexer = Lexer::new(terminal_set(&["ab", "[a-z]+"]));
    assert_eq!(
        lexer.tokenize("ab"),
        vec![Token::Terminal("ab".to_string()), Token::EOF]
    );
    assert_eq!(
        lexer.tokenize("abc"),
        vec![Token::Terminal("[a-z]+".to_string()), Token::EOF]
    );
}

#[test]
fn test_tokenizer_empty_literal_does_not_stick() {
    let lexer = Lexer::new(terminal_set(&["", "a"]));
    assert_eq!(lexer.tokenize(""), vec![Token::EOF]);
    assert_eq!(
        lexer.tokenize("a"),
        vec![Token::Terminal("a".to_string()), Token::EOF]
    );
    assert_eq!(
        lexer.tokenize("aa"),
        vec![
            Token::Terminal("a".to_string()),
            Token::Terminal("a".to_string()),
            Token::EOF,
        ]
    );
}

#[test]
fn test_tokenizer_unclosed_quote_is_not_a_string() {
    let lexer = Lexer::new(terminal_set(&["any", "a"]));
    let tokens = lexer.tokenize(r#""u"#);
    assert_eq!(
        tokens,
        vec![
            Token::Terminal("\"".to_string()),
            Token::Terminal("u".to_string()),
            Token::EOF,
        ]
    );
}
