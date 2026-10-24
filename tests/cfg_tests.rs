use RegExt::{Captures, CfgParser, Match};

#[test]
fn test_cfg_basic() {
    let parser = CfgParser::new(r#"S -> "a""#).unwrap();
    assert!(parser.is_match("a"));
    assert!(parser.is_match(" a "));
    assert!(!parser.is_match("b"));
    assert!(!parser.is_match(""));
    assert!(!parser.is_match("aa"));
    assert_eq!(parser.find(" a "), Some(Match::new(1, 2)));
    assert_eq!(parser.find("ba"), Some(Match::new(1, 2)));
}

#[test]
fn test_cfg_alternation() {
    let parser = CfgParser::new(r#"S -> "a" | "b" | "c""#).unwrap();
    assert!(parser.is_match("a"));
    assert!(parser.is_match("b"));
    assert!(parser.is_match("c"));
    assert!(parser.is_match(" a "));
    assert!(!parser.is_match("d"));
    assert!(!parser.is_match("ab"));
    assert_eq!(parser.find("xby"), Some(Match::new(1, 2)));
}

#[test]
fn test_cfg_sequence() {
    let parser = CfgParser::new(r#"S -> "a" "b" "c""#).unwrap();
    assert!(parser.is_match("abc"));
    assert!(parser.is_match("a b c"));
    assert!(parser.is_match("a  b   c"));
    assert!(!parser.is_match("a b"));
    assert!(!parser.is_match("cba"));
    // The space sits between tokens, and those tokens cover the whole slice.
    assert_eq!(parser.find("a b c"), Some(Match::new(0, 5)));
    assert_eq!(parser.find("xa b cz"), Some(Match::new(1, 6)));
}

#[test]
fn test_cfg_multiple_productions() {
    let parser = CfgParser::new(r#"E -> E "+" T | T; T -> "t""#).unwrap();
    assert!(parser.is_match("t"));
    assert!(parser.is_match("t+t"));
    assert!(parser.is_match("t + t"));
    assert!(!parser.is_match("t +"));
    assert!(!parser.is_match(""));
}

#[test]
fn test_cfg_nested_with_parens() {
    let parser = CfgParser::new(
        r#"SExp -> "(" SExpList ")" | Atom; SExpList -> SExp SExpList | SExp; Atom -> "foo" | "bar""#,
    ).unwrap();
    assert!(parser.is_match("foo"));
    assert!(parser.is_match("( foo )"));
    assert!(parser.is_match("(foo)"));
    assert!(parser.is_match("( foo bar )"));
    assert!(parser.is_match("( foo ( bar ) )"));
    assert!(parser.is_match("(foo(bar))"));
    assert!(!parser.is_match("( foo"));
    assert!(!parser.is_match(""));
}

#[test]
fn test_cfg_arithmetic_with_parens() {
    let parser = CfgParser::new(
        r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | NUM; NUM -> "1" | "2""#,
    )
    .unwrap();
    assert!(parser.is_match("1"));
    assert!(parser.is_match("1 + 2"));
    assert!(parser.is_match("1+2"));
    assert!(parser.is_match("1 * 2"));
    assert!(parser.is_match("( 1 + 2 )"));
    assert!(parser.is_match("(1+2)*2"));
    assert!(!parser.is_match("1 +"));
    assert!(!parser.is_match("1++2"));
}

#[test]
fn test_cfg_epsilon() {
    let parser = CfgParser::new("S -> epsilon").unwrap();
    assert!(parser.is_match(""));
    assert!(parser.is_match("   "));
    assert!(parser.is_match("\n\t"));
    assert!(!parser.is_match("a"));
    // A whitespace-only slice has no tokens, so it is not a `find` span.
    // The empty slice is in the language, so it is the leftmost match.
    assert_eq!(parser.find(""), Some(Match::new(0, 0)));
    assert_eq!(parser.find("   "), Some(Match::new(0, 0)));
    assert_eq!(parser.find("a"), Some(Match::new(0, 0)));
}

#[test]
fn test_cfg_unquoted_is_nonterminal() {
    // An unquoted symbol is a non-terminal: `a` on the RHS is undefined, so the
    // input `a` (a bare token) must not match.
    let parser = CfgParser::new("S -> a").unwrap();
    assert!(!parser.is_match("a"));
    assert!(!parser.is_match(" a "));
    assert_eq!(parser.find("a"), None);
}

#[test]
fn test_parser_word_and_number_runs() {
    let parser = CfgParser::new(r#"S -> Word Num; Word -> "[a-z]+" ; Num -> "[0-9]+""#).unwrap();
    assert!(parser.is_match("abc42"));
    assert!(parser.is_match("abc 42"));
    assert!(!parser.is_match("abc"));
    assert!(!parser.is_match("42abc"));
    assert!(!parser.is_match("abc42!"));
    assert_eq!(parser.find("abc 42"), Some(Match::new(0, 6)));
    assert_eq!(parser.find("!abc42"), Some(Match::new(1, 6)));
}

#[test]
fn test_parser_any_string_token() {
    let parser = CfgParser::new(r#"S -> "any""#).unwrap();
    assert!(parser.is_match(r#""any content at all""#));
    assert!(parser.is_match(r#""a \"quoted\" word""#));
    assert!(!parser.is_match("abc"));
    assert!(!parser.is_match(r#""unclosed"#));
    assert_eq!(parser.find(r#"xx"hi"yy"#), Some(Match::new(2, 6)));
}

#[test]
fn test_cfg_new_errors() {
    assert!(CfgParser::new("").is_err(), "empty grammar must error");
    assert!(
        CfgParser::new("S a").is_err(),
        "production without -> must error"
    );
    assert!(
        CfgParser::new(r#"S -> "a\"#).is_err(),
        "a dangling escape must error"
    );
    assert!(
        CfgParser::new(r#"S -> "unterminated"#).is_err(),
        "an unclosed quote must error"
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
    assert!(parser.is_match(" test "));
    assert_eq!(parser.find(" test "), Some(Match::new(1, 5)));
    assert_eq!(parser.find("test "), Some(Match::new(0, 4)));
    assert_eq!(
        parser.captures("a test b", &["Nope"]),
        Some(Captures {
            matches: vec![Some(Match::new(2, 6)), None]
        })
    );
}

#[test]
fn test_cfg_find_leftmost() {
    let parser = CfgParser::new(r#"S -> "o""#).unwrap();
    assert_eq!(parser.find("hello"), Some(Match::new(4, 5)));
    // Leftmost start wins even though a later one also matches.
    assert_eq!(parser.find("helloo"), Some(Match::new(4, 5)));
    assert_eq!(parser.find("x o y"), Some(Match::new(2, 3)));
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
    assert!(parser.is_match("   "));
    assert_eq!(parser.find("   "), Some(Match::new(0, 0)));
    // Internal spaces stay inside a span whose tokens cover it.
    assert_eq!(parser.find("a a"), Some(Match::new(0, 3)));
}

#[test]
fn test_cfg_find_plus() {
    let parser = CfgParser::new(r#"S -> "a" S | "a""#).unwrap();
    assert_eq!(parser.find("baab"), Some(Match::new(1, 3)));
    assert_eq!(parser.find("b"), None);
    assert!(parser.is_match("a a"));
    assert_eq!(parser.find("a a"), Some(Match::new(0, 3)));
    // " ba " is space, b, a, space: the `a` is at byte 2.
    assert_eq!(parser.find(" ba "), Some(Match::new(2, 3)));
    // A space before that `a` is outside the span.
    assert_eq!(parser.find(" b a "), Some(Match::new(3, 4)));
}

#[test]
fn test_cfg_find_multibyte_offsets() {
    let parser = CfgParser::new(r#"S -> "é""#).unwrap();
    assert_eq!(parser.find("aéb"), Some(Match::new(1, 3)));
    assert_eq!(parser.find("éé"), Some(Match::new(0, 2)));
    assert_eq!(parser.find("aéé"), Some(Match::new(1, 3)));
    assert!(parser.is_match(" é "));
    assert_eq!(parser.find(" é "), Some(Match::new(1, 3)));
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
    assert!(parser.is_match("x a y"));
    assert_eq!(
        parser.captures("x a y", &["G"]),
        Some(Captures {
            matches: vec![Some(Match::new(0, 5)), Some(Match::new(2, 3))]
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

#[test]
fn test_cfg_quote_aware_terminals() {
    let parser = CfgParser::new(r#"S -> "a|b" | "c""#).unwrap();
    assert!(parser.is_match("a|b"));
    assert!(parser.is_match("c"));
    assert!(!parser.is_match("ab"));

    let parser = CfgParser::new(r#"S -> "a;b""#).unwrap();
    assert!(parser.is_match("a;b"));
    assert!(!parser.is_match("ab"));

    let parser = CfgParser::new(r#"S -> "a\"b""#).unwrap();
    assert!(parser.is_match("a\"b"));
    assert!(!parser.is_match("ab"));

    let parser = CfgParser::new(r#"S -> "a\\""#).unwrap();
    assert!(parser.is_match("a\\"));

    let parser = CfgParser::new(r#"S -> "a b" "c""#).unwrap();
    assert!(parser.is_match("a b c"));
    assert!(!parser.is_match("abc"));

    // `->`, `|`, and `;` inside quotes are terminal text.
    let parser = CfgParser::new(r#"S -> "a->b""#).unwrap();
    assert!(parser.is_match("a->b"));
    assert!(!parser.is_match("ab"));

    let parser = CfgParser::new(r#"S -> "a|b" "c""#).unwrap();
    assert!(parser.is_match("a|bc"));
    assert!(parser.is_match("a|b c"));
    assert!(!parser.is_match("abc"));

    let parser = CfgParser::new(r#"S -> "\"""#).unwrap();
    assert!(parser.is_match("\""));
    assert!(!parser.is_match(""));

    let parser = CfgParser::new(r#"S -> "a;b" | T; T -> "c""#).unwrap();
    assert!(parser.is_match("a;b"));
    assert!(parser.is_match("c"));
    assert!(!parser.is_match("ab"));

    assert!(
        CfgParser::new(r#"S -> "a;b"#).is_err(),
        "an unclosed quote is a grammar error"
    );
    assert!(
        CfgParser::new(r#"S -> "unterminated"#).is_err(),
        "an unclosed quote is a grammar error"
    );
}

#[test]
fn test_cfg_rejects_empty_terminal() {
    assert!(
        CfgParser::new(r#"S -> """#).is_err(),
        "an empty quoted terminal must be rejected"
    );
    assert!(
        CfgParser::new(r#"S -> "a" """#).is_err(),
        "an empty quoted terminal must be rejected"
    );
    assert!(
        CfgParser::new(r#"S -> "a" | """#).is_err(),
        "an empty alternative must be rejected"
    );
}

#[test]
fn test_cfg_longest_terminal_beats_class_run() {
    let parser = CfgParser::new(r#"S -> "[a-z]+" | "hello!""#).unwrap();
    assert!(parser.is_match("hello!"));
    assert!(parser.is_match("hello"));
    assert!(parser.is_match("h"));
    assert!(!parser.is_match("hello!x"));
    assert!(!parser.is_match(""));
    assert!(!parser.is_match("!"));
    // `x` is a letter, so the leftmost match is the class run, not `hello!`.
    assert_eq!(parser.find("xhello!"), Some(Match::new(0, 6)));
    assert_eq!(parser.find("xhello"), Some(Match::new(0, 6)));
    // `!` is not a letter, so `hello!` wins over the shorter class run.
    assert_eq!(parser.find("!hello!"), Some(Match::new(1, 7)));
}

#[test]
fn test_cfg_longest_overlapping_class_run() {
    let parser = CfgParser::new(r#"S -> "[a-z]+" | "[a-c]+""#).unwrap();
    assert!(parser.is_match("abcd"));
    assert!(parser.is_match("abc"));
    assert!(parser.is_match("ab"));
    assert!(parser.is_match("a"));
    assert!(parser.is_match("d"));
    assert!(!parser.is_match("abc!"));
    assert!(!parser.is_match(""));
    // `x` is a letter, so the run starts at 0. A digit is outside the class.
    assert_eq!(parser.find("xabcd"), Some(Match::new(0, 5)));
    assert_eq!(parser.find("1abcd"), Some(Match::new(1, 5)));
    assert_eq!(parser.find("abc!"), Some(Match::new(0, 3)));
}

#[test]
fn test_cfg_overlapping_char_classes_share_letters() {
    let parser = CfgParser::new(r#"S -> "[a-z]" | C C; C -> "[a-c]""#).unwrap();
    assert_eq!(parser.conflict_count(), 0);
    assert!(parser.is_match("b"));
    assert!(parser.is_match("c"));
    assert!(parser.is_match("bb"));
    assert!(parser.is_match("b b"));
    assert!(parser.is_match("d"));
    assert!(!parser.is_match(""));
    assert!(!parser.is_match("db"));
    assert!(!parser.is_match("dd"));
    // `x` is in `[a-z]`, so it is the leftmost one-character match.
    assert_eq!(parser.find("xb"), Some(Match::new(0, 1)));
    assert_eq!(parser.find("1b"), Some(Match::new(1, 2)));
}

#[test]
fn test_cfg_rejects_inverted_class_range() {
    assert!(CfgParser::new(r#"S -> "[z-a]""#).is_err());
    assert!(CfgParser::new(r#"S -> "[a--]""#).is_err());
    assert!(CfgParser::new(r#"S -> "[9-0]""#).is_err());
    assert!(CfgParser::new(r#"S -> "[b-a-c]""#).is_err());
    assert!(CfgParser::new(r#"S -> "[z-a]+""#).is_err());
    let parser = CfgParser::new(r#"S -> "[a-a]""#).unwrap();
    assert!(parser.is_match("a"));
    assert!(!parser.is_match("b"));
    // A hyphen at either edge, or two hyphens, is a literal member.
    let edges = CfgParser::new(r#"S -> "[-a]" | "[a-]" | "[--]""#).unwrap();
    assert!(edges.is_match("-"));
    assert!(edges.is_match("a"));
    assert!(!edges.is_match("b"));
}

#[test]
fn test_cfg_end_marker_is_not_eof_terminal() {
    let parser = CfgParser::new(r#"S -> A "EOF"; A -> epsilon | "a""#).unwrap();
    assert_eq!(parser.conflict_count(), 0);
    assert!(parser.is_match("EOF"));
    assert!(parser.is_match("aEOF"));
    assert!(parser.is_match("EOF "));
    assert!(parser.is_match(" EOF"));
    assert!(!parser.is_match(""));
    assert!(!parser.is_match("a"));
    assert!(!parser.is_match("xEOF"));
    assert!(!parser.is_match("EOFx"));
    assert_eq!(parser.find(" EOF"), Some(Match::new(1, 4)));
    assert_eq!(parser.find("aEOF"), Some(Match::new(0, 4)));
    // The leading `x` is outside the slice; `EOF` itself is in the language.
    assert_eq!(parser.find("xEOF"), Some(Match::new(1, 4)));
    assert_eq!(parser.find("EOFx"), Some(Match::new(0, 3)));
}

#[test]
fn test_cfg_augmented_start_does_not_collide() {
    let parser = CfgParser::new(r#"S -> "x" S_prime; S_prime -> "a" | epsilon"#).unwrap();
    assert_eq!(parser.conflict_count(), 0);
    assert!(parser.is_match("x"));
    assert!(parser.is_match("xa"));
    assert!(parser.is_match("x a"));
    assert!(parser.is_match("xa "));
    assert!(!parser.is_match("xx"));
    assert!(!parser.is_match("x x"));
    assert!(!parser.is_match("a"));
    assert!(!parser.is_match(""));

    // `S_prime` as its own alternative is a user nonterminal, not the
    // augmented start.
    let alt = CfgParser::new(r#"S -> "x" | S_prime; S_prime -> "a""#).unwrap();
    assert_eq!(alt.conflict_count(), 0);
    assert!(alt.is_match("x"));
    assert!(alt.is_match("a"));
    assert!(!alt.is_match("xa"));
    assert!(!alt.is_match("xx"));
}

#[test]
fn test_cfg_find_shorter_alternative_inside_longer_literal() {
    let parser = CfgParser::new(r#"S -> "ab" "c" | "b""#).unwrap();
    assert_eq!(parser.find("ab"), Some(Match::new(1, 2)));
    assert_eq!(parser.find("ab "), Some(Match::new(1, 2)));
    assert_eq!(parser.find("zab"), Some(Match::new(2, 3)));
    assert_eq!(parser.find("abc"), Some(Match::new(0, 3)));
    assert_eq!(parser.find("b"), Some(Match::new(0, 1)));
    assert!(parser.is_match("ab c"));
    assert_eq!(parser.find("ab c"), Some(Match::new(0, 4)));
    assert!(!parser.is_match("ab"));
}
