//! Regex API tests: the pattern dialect, anchored `is_match`, leftmost
//! `find`, `captures` with group spans, and the honest rejection of
//! patterns whose CFG encoding is not LALR(1).

use RegExt::{Captures, Match, Regex, RegexError};

fn caps(matches: Vec<Option<Match>>) -> Captures {
    Captures { matches }
}

/// `Regex` owns an LALR table (not `Debug`), so surface compile errors via
/// this helper instead of `unwrap_err`.
fn err_of(pattern: &str) -> RegexError {
    match Regex::new(pattern) {
        Ok(_) => panic!("pattern {pattern:?} compiled but must be rejected"),
        Err(e) => e,
    }
}

// =============================================================================
// Pattern dialect
// =============================================================================

#[test]
fn test_regex_literal() {
    let re = Regex::new("a").unwrap();
    assert!(re.is_match("a"));
    assert!(!re.is_match("b"));
}

#[test]
fn test_regex_alternation() {
    let re = Regex::new("a|b").unwrap();
    assert!(re.is_match("a"));
    assert!(re.is_match("b"));
    assert!(!re.is_match("c"));
}

#[test]
fn test_regex_kleene_star() {
    let re = Regex::new("a*").unwrap();
    assert!(re.is_match(""));
    assert!(re.is_match("a"));
    assert!(re.is_match("aa"));
}

#[test]
fn test_regex_plus() {
    let re = Regex::new("a+").unwrap();
    assert!(!re.is_match(""));
    assert!(re.is_match("a"));
    assert!(re.is_match("aa"));
}

#[test]
fn test_regex_optional() {
    let re = Regex::new("ab?").unwrap();
    assert!(re.is_match("a"));
    assert!(re.is_match("ab"));
    assert!(!re.is_match("b"));
}

#[test]
fn test_regex_escape() {
    let re = Regex::new(r"a\*b").unwrap();
    assert!(re.is_match("a*b"));
    assert!(!re.is_match("ab"));
}

#[test]
fn test_regex_quantifier_binds_last_char() {
    // `ab*` is `a` followed by `b*`, not `(ab)*`.
    let re = Regex::new("ab*").unwrap();
    assert!(re.is_match("a"));
    assert!(re.is_match("ab"));
    assert!(re.is_match("abb"));
    assert!(!re.is_match("abab"));
}

#[test]
fn test_regex_grouped_repetition() {
    let re = Regex::new("(a+)(b+)").unwrap();
    assert!(re.is_match("ab"), "one of each satisfies a+ then b+");
    assert!(re.is_match("aabb"));
    assert!(!re.is_match("bbaa"), "b before a must not match");
    assert!(!re.is_match("aabba"), "trailing a must not match");
    assert!(!re.is_match(""), "both repetitions require at least one");
}

// =============================================================================
// Rejected patterns
// =============================================================================

#[test]
fn test_regex_rejects_dot() {
    for pattern in ["h.llo", ".", "a.*b"] {
        let err = err_of(pattern);
        assert!(
            matches!(err, RegexError::InvalidPattern(_)),
            "[{pattern}] must be InvalidPattern, got {err}"
        );
    }
}

#[test]
fn test_regex_rejects_empty_alternative() {
    for pattern in ["a|", "|b", "|"] {
        let err = err_of(pattern);
        assert!(
            matches!(err, RegexError::InvalidPattern(_)),
            "[{pattern}] must be InvalidPattern, got {err}"
        );
    }
}

#[test]
fn test_regex_rejects_unbalanced_parens() {
    for pattern in ["(a", "(a)(b", "a(b", "((a)"] {
        assert!(
            matches!(err_of(pattern), RegexError::InvalidPattern(_)),
            "[{pattern}] must be rejected"
        );
    }
}

#[test]
fn test_regex_rejects_trailing_junk() {
    assert!(
        matches!(err_of("a)"), RegexError::InvalidPattern(_)),
        "trailing ')' must be rejected"
    );
}

#[test]
fn test_regex_rejects_dangling_escape() {
    assert!(
        matches!(err_of(r"a\"), RegexError::InvalidPattern(_)),
        "dangling escape must be rejected"
    );
}

#[test]
fn test_regex_rejects_quantifier_without_operand() {
    for pattern in ["*", "**", "+", "?"] {
        assert!(
            matches!(err_of(pattern), RegexError::InvalidPattern(_)),
            "[{pattern}] must be rejected"
        );
    }
}

#[test]
fn test_regex_rejects_non_lalr_encoding() {
    // `(a)+a` needs the parser to guess how far `a+` reaches before the
    // trailing literal — a genuine shift/reduce conflict. The compiler is
    // honest about it instead of silently picking a parse.
    let err = err_of("(a)+a");
    assert!(
        matches!(err, RegexError::CompileError(_)),
        "(a)+a must be a CompileError, got {err}"
    );
}

// =============================================================================
// is_match stays anchored (whole input)
// =============================================================================

#[test]
fn test_is_match_is_anchored() {
    let re = Regex::new("test").unwrap();
    assert!(re.is_match("test"));
    assert!(!re.is_match("a test b"));
    assert!(!re.is_match("testing"));
    assert!(!re.is_match(""));
}

// =============================================================================
// find: leftmost match at any position
// =============================================================================

#[test]
fn test_find_whole_input() {
    let re = Regex::new("test").unwrap();
    assert_eq!(re.find("test"), Some(Match::new(0, 4)));
    assert_eq!(re.find("abc"), None);
}

#[test]
fn test_find_mid_text() {
    let re = Regex::new("test").unwrap();
    assert_eq!(re.find("a test b"), Some(Match::new(2, 6)));
}

#[test]
fn test_find_leftmost_position_wins() {
    let re = Regex::new("a+").unwrap();
    // Two possible matches (1..3) and (3..4); the leftmost one wins.
    assert_eq!(re.find("baab"), Some(Match::new(1, 3)));
}

#[test]
fn test_find_longest_at_leftmost() {
    let re = Regex::new("a+").unwrap();
    assert_eq!(re.find("aaab"), Some(Match::new(0, 3)));
}

#[test]
fn test_find_empty_match() {
    let re = Regex::new("a*").unwrap();
    assert_eq!(re.find("bbb"), Some(Match::new(0, 0)));
}

#[test]
fn test_find_leftmost_beats_longer_match() {
    let re = Regex::new("a*").unwrap();
    // A longer match at 1..4 must lose to the empty match at 0.
    assert_eq!(re.find("baaab"), Some(Match::new(0, 0)));
}

#[test]
fn test_find_multibyte_offsets() {
    let re = Regex::new("é").unwrap();
    // 'é' is two UTF-8 bytes, so the span must be byte offsets 1..3.
    assert_eq!(re.find("aéb"), Some(Match::new(1, 3)));
}

#[test]
fn test_find_maximal_munch_alternation() {
    // Overlapping literal terminals tokenize longest-first: the input `ab`
    // becomes one `ab` token, so the `ab` branch wins.
    let re = Regex::new("a|ab").unwrap();
    assert_eq!(re.find("a"), Some(Match::new(0, 1)));
    assert_eq!(re.find("ab"), Some(Match::new(0, 2)));
    assert_eq!(re.find("bab"), Some(Match::new(1, 3)));
}

// =============================================================================
// captures: whole match + group spans (in pattern order)
// =============================================================================

#[test]
fn test_captures_without_groups() {
    let re = Regex::new("hello").unwrap();
    assert_eq!(re.group_count(), 0);
    assert_eq!(
        re.captures("hello"),
        Some(caps(vec![Some(Match::new(0, 5))]))
    );
    assert_eq!(
        re.captures("xhellox"),
        Some(caps(vec![Some(Match::new(1, 6))]))
    );
}

#[test]
fn test_captures_single_group() {
    let re = Regex::new("a(b)c").unwrap();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("xabcy").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(1, 4)), Some(Match::new(2, 3))]
    );
}

#[test]
fn test_captures_multiple_groups() {
    let re = Regex::new("(a)(b)(c)").unwrap();
    assert_eq!(re.group_count(), 3);
    let caps = re.captures("xabcy").unwrap();
    assert_eq!(
        caps.matches,
        vec![
            Some(Match::new(1, 4)),
            Some(Match::new(1, 2)),
            Some(Match::new(2, 3)),
            Some(Match::new(3, 4))
        ]
    );
}

#[test]
fn test_captures_groups_in_repetitions() {
    // Each group is the LAST reduction of its nonterminal: for a left
    // recursive repetition that is the outermost (full) span.
    let re = Regex::new("(a+)(b+)").unwrap();
    let caps = re.captures("xaabb").unwrap();
    assert_eq!(
        caps.matches,
        vec![
            Some(Match::new(1, 5)),
            Some(Match::new(1, 3)),
            Some(Match::new(3, 5))
        ]
    );
}

#[test]
fn test_captures_unmatched_group_is_none() {
    let re = Regex::new("a|(b)").unwrap();
    assert_eq!(
        re.captures("a"),
        Some(caps(vec![Some(Match::new(0, 1)), None]))
    );
    assert_eq!(
        re.captures("b"),
        Some(caps(vec![Some(Match::new(0, 1)), Some(Match::new(0, 1))]))
    );
}

#[test]
fn test_captures_group_records_last_iteration() {
    // `(a)+` = a group repeated: the group is the last `a`, not the run.
    let re = Regex::new("(a)+b").unwrap();
    let caps = re.captures("aab").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 3)), Some(Match::new(1, 2))]
    );
}

#[test]
fn test_captures_nested_group_numbering() {
    // Groups are numbered by opening parenthesis: outer = 1, inner = 2.
    // The outer group spans the whole repetition; the inner one is its
    // last iteration.
    let re = Regex::new("((a)+)b").unwrap();
    assert_eq!(re.group_count(), 2);
    let caps = re.captures("aab").unwrap();
    assert_eq!(
        caps.matches,
        vec![
            Some(Match::new(0, 3)),
            Some(Match::new(0, 2)),
            Some(Match::new(1, 2))
        ]
    );
}

#[test]
fn test_captures_optional_group() {
    // `a(b)?c`: the group sits inside the option, so it does not
    // participate (None) when the option takes the empty branch.
    let re = Regex::new("a(b)?c").unwrap();
    assert_eq!(
        re.captures("ac"),
        Some(caps(vec![Some(Match::new(0, 2)), None]))
    );
    assert_eq!(
        re.captures("abc"),
        Some(caps(vec![Some(Match::new(0, 3)), Some(Match::new(1, 2))]))
    );
}

#[test]
fn test_captures_star_group() {
    // `x(y*)(z)`: the group wraps the star, so it still participates with
    // a zero-width span when the star matches empty.
    let re = Regex::new("x(y*)(z)").unwrap();
    assert_eq!(re.group_count(), 2);
    assert_eq!(
        re.captures("xzz"),
        Some(caps(vec![
            Some(Match::new(0, 2)),
            Some(Match::new(1, 1)),
            Some(Match::new(1, 2))
        ]))
    );
    assert_eq!(
        re.captures("xyzz"),
        Some(caps(vec![
            Some(Match::new(0, 3)),
            Some(Match::new(1, 2)),
            Some(Match::new(2, 3))
        ]))
    );
}

#[test]
fn test_captures_none_without_match() {
    let re = Regex::new("(a)").unwrap();
    assert_eq!(re.captures("b"), None);
    assert_eq!(re.find("b"), None);
}

// =============================================================================
// API surface
// =============================================================================

#[test]
fn test_regex_pattern_and_group_count() {
    let re = Regex::new("x(y*)(z)").unwrap();
    assert_eq!(re.pattern(), "x(y*)(z)");
    assert_eq!(re.group_count(), 2);
}
