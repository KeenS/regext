use RegExt::{Captures, CfgParser, Match};

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

// =============================================================================
// find: leftmost match, longest at the leftmost start
// =============================================================================

#[test]
fn test_cfg_find_whole_and_mid_text() {
    let parser = CfgParser::new(r#"S -> "test""#).unwrap();
    assert_eq!(parser.find("test"), Some(Match::new(0, 4)));
    assert_eq!(parser.find("a test b"), Some(Match::new(2, 6)));
    assert_eq!(parser.find("abc"), None);
    assert_eq!(parser.find(""), None);
}

#[test]
fn test_cfg_find_leftmost() {
    let parser = CfgParser::new(r#"S -> "o""#).unwrap();
    assert_eq!(parser.find("hello"), Some(Match::new(4, 5)));
    // Leftmost start wins even though a later one also matches.
    assert_eq!(parser.find("helloo"), Some(Match::new(4, 5)));
}

#[test]
fn test_cfg_find_star() {
    let parser = CfgParser::new("S -> \"a\" S | epsilon").unwrap();
    // Empty match at the leftmost position.
    assert_eq!(parser.find("bbb"), Some(Match::new(0, 0)));
    // Longest at the leftmost start.
    assert_eq!(parser.find("aaab"), Some(Match::new(0, 3)));
    // Leftmost (empty) beats a longer match further right.
    assert_eq!(parser.find("baaab"), Some(Match::new(0, 0)));
}

#[test]
fn test_cfg_find_plus() {
    let parser = CfgParser::new(r#"S -> "a" S | "a""#).unwrap();
    assert_eq!(parser.find("baab"), Some(Match::new(1, 3)));
    assert_eq!(parser.find("b"), None);
}

#[test]
fn test_cfg_find_multibyte_offsets() {
    let parser = CfgParser::new(r#"S -> "é""#).unwrap();
    assert_eq!(parser.find("aéb"), Some(Match::new(1, 3)));
}

// =============================================================================
// captures: whole match + spans of the requested nonterminals
// =============================================================================

#[test]
fn test_cfg_captures_group_span() {
    let parser = CfgParser::new(r#"S -> "x" G "y"; G -> "a" | "b""#).unwrap();
    assert_eq!(parser.find("xbbxay"), Some(Match::new(3, 6)));
    assert_eq!(
        parser.captures("xbbxay", &["G"]),
        Some(Captures {
            matches: vec![Some(Match::new(3, 6)), Some(Match::new(4, 5))]
        })
    );
}

#[test]
fn test_cfg_captures_unrequested_group_is_none() {
    let parser = CfgParser::new(r#"S -> "xy""#).unwrap();
    assert_eq!(
        parser.captures("zxy", &["G"]),
        Some(Captures {
            matches: vec![Some(Match::new(1, 3)), None]
        })
    );
}

#[test]
fn test_cfg_captures_repetition_group_outermost() {
    let parser = CfgParser::new(r#"S -> "x" G+ "y"; G+ -> "a" G+ | "a""#).unwrap();
    assert_eq!(
        parser.captures("xaaay", &["G+"]),
        Some(Captures {
            matches: vec![Some(Match::new(0, 5)), Some(Match::new(1, 4))]
        })
    );
}

#[test]
fn test_cfg_captures_epsilon_group() {
    let parser = CfgParser::new(r#"S -> "p" A; A -> B "c"; B -> "a" | epsilon"#).unwrap();
    assert_eq!(parser.find("p ac"), Some(Match::new(0, 4)));
    assert_eq!(
        parser.captures("p ac", &["B"]),
        Some(Captures {
            matches: vec![Some(Match::new(0, 4)), Some(Match::new(2, 3))]
        })
    );
    // B derives epsilon: a zero-width capture at the parse frontier.
    assert_eq!(
        parser.captures("p c", &["B"]),
        Some(Captures {
            matches: vec![Some(Match::new(0, 3)), Some(Match::new(2, 2))]
        })
    );
}

#[test]
fn test_cfg_captures_none_without_match() {
    let parser = CfgParser::new(r#"S -> "a""#).unwrap();
    assert_eq!(parser.captures("b", &["A"]), None);
    assert_eq!(parser.find("b"), None);
}
