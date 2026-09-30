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

// =============================================================================
// Zero-width assertions: lookahead (?=) (?!), lookbehind (?<=) (?<!)
//
// An assertion is evaluated at the position where its placeholder sits in the
// winning derivation. A lookahead needs its body to match a PREFIX of the
// remainder from that position; a lookbehind needs its body to match a
// SUFFIX of the consumed text ending at that position (both as far as
// possible). Assertions consume nothing, so the match span is exactly the
// tokens the main grammar consumes.
// =============================================================================

#[test]
fn test_lookahead_positive() {
    // `a(?=b)`: consume `a` only, asserting `b` follows. The match is the
    // single `a`; `b` is looked at, not consumed.
    let re = Regex::new("a(?=b)").unwrap();
    assert_eq!(re.group_count(), 0);
    // Anchored: the whole input is `ab`, but only `a` is consumed.
    assert!(!re.is_match("ab"));
    assert!(!re.is_match("a"));
    // Leftmost match is the `a` at each `ab` boundary.
    assert_eq!(re.find("ab"), Some(Match::new(0, 1)));
    assert_eq!(re.find("ac"), None); // no `b` after `a`
    assert_eq!(re.find("xab"), Some(Match::new(1, 2)));
    assert_eq!(re.find("bab"), Some(Match::new(1, 2)));
    // At the first `a`, the remainder is `ab` which does not start with `b`;
    // the leftmost satisfying `a` is the second one.
    assert_eq!(re.find("aab"), Some(Match::new(1, 2)));
}

#[test]
fn test_lookahead_with_quantifier() {
    // `a*(?=b)`: greedily consume the run of `a`s, asserting a `b` follows.
    let re = Regex::new("a*(?=b)").unwrap();
    assert_eq!(re.find("xaab"), Some(Match::new(1, 3)));
    assert_eq!(re.find("xaabx"), Some(Match::new(1, 3)));
    assert_eq!(re.find("xaa"), None); // no `b` to assert against
}

#[test]
fn test_lookahead_negative() {
    // `a(?!b)c`: consume `a`, assert `b` does NOT follow, then consume `c`.
    let re = Regex::new("a(?!b)c").unwrap();
    assert!(re.is_match("ac"));
    assert!(!re.is_match("abc"));
    assert_eq!(re.find("xabc"), None);
    assert_eq!(re.find("xacc"), Some(Match::new(1, 3)));
}

#[test]
fn test_lookbehind_positive() {
    // `(?<=a)b`: assert `a` precedes, then consume `b`.
    let re = Regex::new("(?<=a)b").unwrap();
    assert!(!re.is_match("ab")); // only `b` is consumed; `a` is looked at
    assert!(!re.is_match("b"));
    assert_eq!(re.find("xab"), Some(Match::new(2, 3)));
    assert_eq!(re.find("xbb"), None);
    assert_eq!(re.find("aab"), Some(Match::new(2, 3)));
}

#[test]
fn test_lookbehind_negative() {
    // `(?<!a)b`: assert `a` does NOT precede, then consume `b`.
    let re = Regex::new("(?<!a)b").unwrap();
    assert!(re.is_match("b"));
    assert_eq!(re.find("xab"), None);
    assert_eq!(re.find("xbb"), Some(Match::new(1, 2)));
    assert_eq!(re.find("b"), Some(Match::new(0, 1)));
}

#[test]
fn test_lookbehind_with_quantifier() {
    // `(?<=a+)b`: the lookbehind body `a+` matches the SUFFIX of the consumed
    // text, reaching as far back as possible (greedy), still not consuming.
    let re = Regex::new("(?<=a+)b").unwrap();
    assert_eq!(re.group_count(), 0);
    assert_eq!(re.find("xaab"), Some(Match::new(3, 4)));
    assert_eq!(
        re.captures("xaab"),
        Some(caps(vec![Some(Match::new(3, 4))]))
    );
    // `(?<!a+)b`: no run of `a`s may precede.
    let re = Regex::new("(?<!a+)b").unwrap();
    assert!(re.is_match("b"));
    assert_eq!(re.find("xaab"), None);
}

#[test]
fn test_assertion_group_captures() {
    // A capture group inside an assertion body is numbered globally and
    // captures when its assertion is in the winning derivation.
    let re = Regex::new("(?<=(a+))b").unwrap();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("xaab").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(3, 4)), Some(Match::new(1, 3))]
    );
}

#[test]
fn test_assertion_in_skipped_branch_is_vacuous() {
    // `a|(?=b)(c)`: on `a` the assertion branch is not taken, so its group
    // does not participate (None) and the assertion is vacuously satisfied.
    let re = Regex::new("a|(?=b)(c)").unwrap();
    assert_eq!(re.group_count(), 1);
    assert_eq!(
        re.captures("a"),
        Some(caps(vec![Some(Match::new(0, 1)), None]))
    );
    // `bc`: the only derivable main slice is the `c`; at that position the
    // positive lookahead `b` fails, so no match at all.
    assert_eq!(re.find("bc"), None);
}

#[test]
fn test_assertion_self_contradictory_never_matches() {
    // `x(?=a(b))c(d)` asks, after `x`, for the remainder to both start with
    // `ab` (lookahead) and continue with `cd` (main pattern) at the SAME
    // position — impossible, so it never matches. Group count is still 2.
    let re = Regex::new("x(?=a(b))c(d)").unwrap();
    assert_eq!(re.group_count(), 2);
    assert_eq!(re.find("xabcd"), None);
    assert_eq!(re.captures("xabcd"), None);
}

#[test]
fn test_assertion_multibyte_offsets() {
    // Offsets are byte-based even with multibyte literals in the body.
    let re = Regex::new("a(?=é)").unwrap();
    assert_eq!(re.find("aéb"), Some(Match::new(0, 1)));
    let re = Regex::new("(?<=é)b").unwrap();
    // 'é' is two bytes, so `b` sits at byte offset 3.
    assert_eq!(re.find("aéb"), Some(Match::new(3, 4)));
}

#[test]
fn test_rejects_empty_assertion_body() {
    for pattern in ["(?=)", "(?<=)b", "(?!)x"] {
        let err = err_of(pattern);
        assert!(
            matches!(err, RegexError::InvalidPattern(_)),
            "[{pattern}] must be InvalidPattern, got {err}"
        );
    }
}

#[test]
fn test_rejects_bad_assertion_intro() {
    let cases: [(&str, &str); 4] = [
        ("(?)", "unsupported construct after '(?'"),
        ("(?~a)", "unsupported construct after '(?'"),
        ("(?<a)", "malformed lookbehind: expected '(?<=' or '(?<!'"),
        ("(?!a)(?=a", "unbalanced '(' in assertion"),
    ];
    for (pattern, msg) in cases {
        let err = err_of(pattern);
        let text = format!("{err}");
        assert!(
            text.contains(msg),
            "[{pattern}] expected message to contain {msg:?}, got {text}"
        );
    }
}

#[test]
fn test_rejects_nested_assertions() {
    let err = err_of("a(?=b(?=c))");
    let text = format!("{err}");
    assert!(
        matches!(err, RegexError::InvalidPattern(_))
            && text.contains("an assertion inside an assertion is not supported"),
        "nested assertion must be rejected, got {text}"
    );
}

#[test]
fn test_rejects_non_lalr_assertion_body() {
    // The assertion body itself must be conflict-free LALR(1); `(a)+a` is not.
    let err = err_of("(?=(a)+a)");
    let text = format!("{err}");
    assert!(
        matches!(err, RegexError::CompileError(_)) && text.contains("1 LALR(1) conflict"),
        "non-LALR assertion body must be a CompileError, got {text}"
    );
}

// =============================================================================
// Character classes: [..], [^..], \d, \w
// =============================================================================

#[test]
fn test_class_literal_members() {
    let re = Regex::new("[abc]").unwrap();
    assert!(re.is_match("a"));
    assert!(re.is_match("b"));
    assert!(re.is_match("c"));
    assert!(!re.is_match("d"));
}

#[test]
fn test_class_ranges() {
    let re = Regex::new("[a-z]+").unwrap();
    assert!(re.is_match("abc"));
    assert!(re.is_match("z"));
    assert!(!re.is_match("a1"));
    // `X` is outside the class, so the leftmost match starts at `a`.
    assert_eq!(re.find("Xa12"), Some(Match::new(1, 2)));
}

#[test]
fn test_class_negated() {
    let re = Regex::new("[^abc]").unwrap();
    assert!(re.is_match("d"));
    assert!(!re.is_match("a"));
    let re = Regex::new("[^a-z]*").unwrap();
    assert!(re.is_match("!!"));
    assert!(!re.is_match("abc"));
    assert_eq!(re.find("abc"), Some(Match::new(0, 0)));
    let re = Regex::new("[^0-9]*").unwrap();
    assert!(re.is_match("ab"));
    assert!(re.is_match(""));
    assert!(!re.is_match("1"));
    assert_eq!(re.find("a1b2"), Some(Match::new(0, 1)));
}

#[test]
fn test_class_special_members() {
    // A pipe inside a class is an ordinary member, not alternation.
    let re = Regex::new("[a|b]").unwrap();
    assert!(re.is_match("|"));
    assert!(re.is_match("a"));
    assert!(!re.is_match("c"));
}

#[test]
fn test_class_quantifiers() {
    let re = Regex::new("[a-z]{2}").unwrap();
    assert!(re.is_match("ab"));
    assert!(!re.is_match("a"));
    assert!(!re.is_match("abc"));
    // `X` is outside the class, so the two-class-char match starts at `a`.
    assert_eq!(re.find("Xabc"), Some(Match::new(1, 3)));
    let re = Regex::new("[a-z]{2,}").unwrap();
    assert!(re.is_match("ab"));
    assert!(re.is_match("abc"));
    assert!(!re.is_match("a"));
}

#[test]
fn test_class_in_alternation_and_group() {
    let re = Regex::new("a|[b-d]").unwrap();
    assert!(re.is_match("a"));
    assert!(re.is_match("c"));
    assert!(!re.is_match("e"));
    let re = Regex::new("([0-9]+)?x").unwrap();
    assert!(re.is_match("x"));
    assert!(re.is_match("42x"));
    assert!(!re.is_match("42y"));
}

#[test]
fn test_class_multibyte_members() {
    // A single-member class is char-exact: `[é]` matches exactly `é`
    // (write the character directly in the pattern; the pattern text is
    // not Rust-escape-processed).
    let re = Regex::new("[é]").unwrap();
    assert_eq!(re.find("aéb"), Some(Match::new(1, 3)));
    assert!(!re.is_match("e"));
    let re = Regex::new("[^é]").unwrap();
    assert!(re.is_match("a"));
    assert!(!re.is_match("é"));
}

#[test]
fn test_alias_digit() {
    let re = Regex::new(r"\d+").unwrap();
    assert!(re.is_match("123"));
    assert!(!re.is_match("1a"));
    // `x` and `a` are not digits; the digit run is `12`.
    assert_eq!(re.find("xa12"), Some(Match::new(2, 4)));
    let re = Regex::new(r"a\d*b").unwrap();
    assert!(re.is_match("ab"));
    assert!(re.is_match("a123b"));
    assert!(!re.is_match("axb"));
    let re = Regex::new(r"\d{3}").unwrap();
    assert!(re.is_match("123"));
    assert!(!re.is_match("12"));
}

#[test]
fn test_alias_word() {
    let re = Regex::new(r"\w+").unwrap();
    assert!(re.is_match("aB_3"));
    assert!(!re.is_match("a.b"));
    assert_eq!(re.find("a.b_"), Some(Match::new(0, 1)));
}

#[test]
fn test_rejects_class_errors() {
    for pattern in ["[", "[]", "[a[b]", "[a"] {
        let err = err_of(pattern);
        assert!(
            matches!(err, RegexError::InvalidPattern(_)),
            "[{pattern}] must be rejected, got {err}"
        );
    }
    // `\s` is rejected with an explanation: whitespace is the token
    // separator, so no terminal can match it.
    let err = err_of(r"\s");
    let text = format!("{err}");
    assert!(
        matches!(err, RegexError::InvalidPattern(_)) && text.contains("token separator"),
        "\\s rejection must explain why, got {text}"
    );
}

#[test]
fn test_assertion_with_class_body() {
    let re = Regex::new(r"x(?=1)(\d)").unwrap();
    assert!(re.is_match("x1"));
    let caps = re.captures("x1").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 2)), Some(Match::new(1, 2))]
    );
    let re = Regex::new(r"(?<=\d)x").unwrap();
    assert_eq!(re.find("1x"), Some(Match::new(1, 2)));
    assert_eq!(re.find("ax"), None);
}

// =============================================================================
// Bounded repetition: {n}, {n,}, {n,m}
// =============================================================================

#[test]
fn test_bound_exact() {
    let re = Regex::new("a{2}").unwrap();
    assert!(re.is_match("aa"));
    assert!(!re.is_match("a"));
    assert!(!re.is_match("aaa"));
    assert_eq!(re.find("xaaay"), Some(Match::new(1, 3)));
}

#[test]
fn test_bound_at_least() {
    let re = Regex::new("a{2,}").unwrap();
    assert!(re.is_match("aa"));
    assert!(re.is_match("aaaa"));
    assert!(!re.is_match("a"));
    assert_eq!(re.find("xaaaay"), Some(Match::new(1, 5)));
}

#[test]
fn test_bound_range() {
    let re = Regex::new("a{2,3}").unwrap();
    assert!(re.is_match("aa"));
    assert!(re.is_match("aaa"));
    assert!(!re.is_match("a"));
    assert!(!re.is_match("aaaa"));
}

#[test]
fn test_bound_binds_last_char() {
    // `ab{2}` is `a` then exactly two `b`s — the bound binds to the last
    // character of the literal run, like `*` `+` `?`.
    let re = Regex::new("ab{2}").unwrap();
    assert!(re.is_match("abb"));
    assert!(!re.is_match("ab"));
    assert!(!re.is_match("abbb"));
}

#[test]
fn test_bound_group_captures_last_copy() {
    let re = Regex::new("(a){2}").unwrap();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("baa").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(1, 3)), Some(Match::new(2, 3))]
    );
}

#[test]
fn test_bound_range_prefers_longest() {
    let re = Regex::new("(a){2,3}").unwrap();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("aaa").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 3)), Some(Match::new(2, 3))]
    );
}

#[test]
fn test_bound_on_group_run() {
    let re = Regex::new("(ab){2}").unwrap();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("abab").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 4)), Some(Match::new(2, 4))]
    );
}

#[test]
fn test_bound_zero_and_cap() {
    let re = Regex::new("a{0}").unwrap();
    assert!(re.is_match(""));
    assert_eq!(re.find("x"), Some(Match::new(0, 0)));
    // 100 is the documented cap: it compiles.
    assert!(Regex::new("a{100}").is_ok());
}

#[test]
fn test_bound_alternation() {
    let re = Regex::new("a{2}|b").unwrap();
    assert!(re.is_match("aa"));
    assert!(re.is_match("b"));
    assert!(!re.is_match("a"));
}

#[test]
fn test_rejects_bad_bounds() {
    for pattern in [
        "a{}", "a{,3}", "a{x}", "a{3,2}", "a{101}", "a{2}+", "a{2}{3}", "a*++",
    ] {
        let err = err_of(pattern);
        assert!(
            matches!(err, RegexError::InvalidPattern(_)),
            "[{pattern}] must be InvalidPattern, got {err}"
        );
    }
}

#[test]
fn test_bound_non_lalr_is_rejected() {
    // `a{2,3}a`: after two `a`s the parser cannot know from one token
    // whether the 2-copy alternative is finished or a third `a` belongs to
    // the 3-copy one — a genuine LALR(1) conflict, refused honestly.
    let err = err_of("a{2,3}a");
    assert!(
        matches!(err, RegexError::CompileError(_)),
        "a{{2,3}}a must be a CompileError, got {err}"
    );
}

// =============================================================================
// Non-capturing groups: (?:...)
// =============================================================================

#[test]
fn test_noncapturing_group_numbering() {
    let re = Regex::new("(?:a)(b)").unwrap();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("ab").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 2)), Some(Match::new(1, 2))]
    );
}

#[test]
fn test_noncapturing_group_matching() {
    let re = Regex::new("(?:ab)+").unwrap();
    assert!(re.is_match("abab"));
    assert!(re.is_match("ab"));
    assert!(!re.is_match("a"));
    let re = Regex::new("(?:a|b)+").unwrap();
    assert!(re.is_match("abba"));
    assert!(!re.is_match("abca"));
    let re = Regex::new("(?:\\d+)?x").unwrap();
    assert!(re.is_match("x"));
    assert!(re.is_match("42x"));
}

#[test]
fn test_noncapturing_with_bound() {
    let re = Regex::new("(?:ab){2,3}").unwrap();
    assert!(re.is_match("abab"));
    assert!(re.is_match("ababab"));
    assert!(!re.is_match("ab"));
    assert!(!re.is_match("abababa"));
}

#[test]
fn test_rejects_empty_noncapturing() {
    assert!(
        matches!(err_of("(?:)"), RegexError::InvalidPattern(_)),
        "(?:) must be rejected"
    );
}

// =============================================================================
// Possessive quantifiers (*+ ++ ?+) and atomic groups (?>...)
//
// This engine never backtracks, so possessive quantifiers are
// language-identical to the plain ones, and an atomic group is structural
// sugar for a non-capturing group.
// =============================================================================

#[test]
fn test_possessive_quantifiers() {
    let star = Regex::new("a*+").unwrap();
    assert!(star.is_match(""));
    assert!(star.is_match("aa"));
    let plus = Regex::new("a++").unwrap();
    assert!(plus.is_match("a"));
    assert!(plus.is_match("aa"));
    assert!(!plus.is_match(""));
    let opt = Regex::new("a?+b").unwrap();
    assert!(opt.is_match("ab"));
    assert!(opt.is_match("b"));
    assert!(!opt.is_match("a"));
    let class = Regex::new("x[a-z]*+").unwrap();
    assert!(class.is_match("xabc"));
    assert!(class.is_match("x"));
}

#[test]
fn test_rejects_double_possessive() {
    let err = err_of("a*++");
    assert!(
        matches!(err, RegexError::InvalidPattern(_)),
        "a*++ must be rejected, got {err}"
    );
}

#[test]
fn test_atomic_group() {
    let re = Regex::new("(?>ab)+").unwrap();
    assert!(re.is_match("abab"));
    assert!(!re.is_match("a"));
    let re = Regex::new("(?>a)(b)").unwrap();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("ab").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 2)), Some(Match::new(1, 2))]
    );
}

// =============================================================================
// Complement: Regex::not() — the complement of a regular language is regular
// =============================================================================

#[test]
fn test_not_is_match() {
    let re = Regex::new("a").unwrap().not();
    assert!(re.is_match("b"));
    assert!(re.is_match("aa"));
    assert!(re.is_match("ab"));
    assert!(re.is_match(""));
    assert!(!re.is_match("a"));
}

#[test]
fn test_not_star_language() {
    // L(a*) = all-a strings (incl. empty); the complement is everything
    // containing a non-a.
    let re = Regex::new("a*").unwrap().not();
    assert!(!re.is_match(""));
    assert!(!re.is_match("aaa"));
    assert!(re.is_match("b"));
    assert!(re.is_match("ab"));
    assert!(re.is_match("ba"));
    assert!(re.is_match("aab"));
}

#[test]
fn test_not_class_and_alias() {
    let re = Regex::new("[a-z]+").unwrap().not();
    assert!(!re.is_match("abc"));
    assert!(!re.is_match("z"));
    assert!(re.is_match(""));
    assert!(re.is_match("1"));
    assert!(re.is_match("ab1"));
    let digits = Regex::new(r"\d+").unwrap().not();
    assert!(!digits.is_match("123"));
    assert!(digits.is_match("12x"));
    assert!(digits.is_match("x"));
}

#[test]
fn test_not_find() {
    // find on the complement: leftmost slice the pattern rejects, longest
    // at that start.
    let re = Regex::new("a").unwrap().not();
    assert_eq!(re.find("b"), Some(Match::new(0, 1)));
    assert_eq!(re.find("ba"), Some(Match::new(0, 2)));
    assert_eq!(re.find("aaa"), Some(Match::new(0, 3)));
    assert_eq!(re.find("a"), Some(Match::new(0, 0))); // empty is not in L(a)
    let ab = Regex::new("ab").unwrap().not();
    // `ab` tokenizes as ONE terminal, so the only slices are the whole
    // token (in L) and the empty slice (not in L).
    assert_eq!(ab.find("ab"), Some(Match::new(0, 0)));
    assert_eq!(ab.find("xab"), Some(Match::new(0, 3)));
}

#[test]
fn test_not_find_rejects_nowhere() {
    // Every slice of an all-`a` text is in L(a*), so the complement finds
    // nothing at all.
    let re = Regex::new("a*").unwrap().not();
    assert_eq!(re.find("aaa"), None);
    assert_eq!(re.find(""), None);
    assert!(re.captures("aaa").is_none());
}

#[test]
fn test_not_captures() {
    // A complement slice has no winning derivation, so no group
    // participates — but the group count (and the matches invariant) hold.
    let re = Regex::new("a(b)+c").unwrap().not();
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("ab").unwrap();
    assert_eq!(caps.matches, vec![Some(Match::new(0, 2)), None]);
    assert_eq!(caps.matches.len(), 1 + re.group_count());
    // The longest slice `abbc` rejects, starting at 0, is `abb`.
    let caps = re.captures("abbc").unwrap();
    assert_eq!(caps.matches, vec![Some(Match::new(0, 3)), None]);
}

#[test]
fn test_not_double_negation_round_trip() {
    let original = Regex::new("a(b)+c").unwrap();
    let round_trip = Regex::new("a(b)+c").unwrap().not().not();
    for text in ["abbc", "abc", "ab", "b", "c", ""] {
        assert_eq!(
            round_trip.is_match(text),
            original.is_match(text),
            "is_match({text})"
        );
    }
    assert_eq!(round_trip.find("xabbcz"), Some(Match::new(1, 5)));
    let caps = round_trip.captures("abbc").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 4)), Some(Match::new(2, 3))]
    );
    assert_eq!(round_trip.group_count(), 1);
}

#[test]
fn test_not_pattern_string() {
    let re = Regex::new("a(b)+c").unwrap().not();
    assert_eq!(re.pattern(), "not(a(b)+c)");
    let double = Regex::new("a").unwrap().not().not();
    assert_eq!(double.pattern(), "not(not(a))");
}

// =============================================================================
// Intersection: Regex::and() — the intersection of two regular languages is
// regular (product automaton), realized at the engine level like `not`
// =============================================================================

#[test]
fn test_and_is_match() {
    // (a|b) ∩ (a|c) = {a}.
    let re = Regex::new("a|b").unwrap().and(Regex::new("a|c").unwrap());
    assert!(re.is_match("a"));
    assert!(!re.is_match("b"));
    assert!(!re.is_match("c"));
    assert!(!re.is_match("ab"));
    assert!(!re.is_match(""));

    // a* ∩ b* = {""};  a+ ∩ a* = a+.
    let star_star = Regex::new("a*").unwrap().and(Regex::new("b*").unwrap());
    assert!(star_star.is_match(""));
    assert!(!star_star.is_match("a"));
    assert!(!star_star.is_match("b"));
    assert!(!star_star.is_match("ab"));
    let plus_star = Regex::new("a+").unwrap().and(Regex::new("a*").unwrap());
    assert!(plus_star.is_match("a"));
    assert!(plus_star.is_match("aaa"));
    assert!(!plus_star.is_match(""));

    // (ab)+ ⊆ (a|b)*, so the intersection is (ab)+ itself.
    let ab = Regex::new("(ab)+")
        .unwrap()
        .and(Regex::new("(a|b)*").unwrap());
    assert!(ab.is_match("ab"));
    assert!(ab.is_match("abab"));
    assert!(!ab.is_match("a"));
    assert!(!ab.is_match("aba"));
}

#[test]
fn test_and_differs_by_tokenization() {
    // `ab` is ONE terminal of "ab" but two tokens of "a": the intersection
    // must be decided on the language, not on aligned token slices.
    let empty = Regex::new("ab").unwrap().and(Regex::new("a").unwrap());
    assert!(!empty.is_match("ab"));
    assert!(!empty.is_match("aab"));
    assert_eq!(empty.find("aab"), None);
    assert!(empty.captures("aab").is_none());

    let same = Regex::new("ab").unwrap().and(Regex::new("ab").unwrap());
    assert!(same.is_match("ab"));
    assert_eq!(same.find("xab"), Some(Match::new(1, 3)));

    // a|aa ∩ aa = {aa} — even where one side's longest-munch token
    // straddles the other side's token boundary.
    let aa = Regex::new("a|aa").unwrap().and(Regex::new("aa").unwrap());
    assert!(aa.is_match("aa"));
    assert!(!aa.is_match("a"));
    assert_eq!(aa.find("xaa"), Some(Match::new(1, 3)));
}

#[test]
fn test_and_find_leftmost_longest() {
    // (a|ab)+ ∩ a* = non-empty runs of a.
    let re = Regex::new("(a|ab)+")
        .unwrap()
        .and(Regex::new("a*").unwrap());
    assert_eq!(re.find("baaax"), Some(Match::new(1, 4)));
    // An empty match at 0 beats any longer match further right.
    let star = Regex::new("a*").unwrap().and(Regex::new("a*").unwrap());
    assert_eq!(star.find("bbb"), Some(Match::new(0, 0)));
    // Nothing to find when the languages are disjoint.
    let disjoint = Regex::new("a").unwrap().and(Regex::new("b").unwrap());
    assert_eq!(disjoint.find("ab"), None);
}

#[test]
fn test_and_captures() {
    // (a)(b) ∩ a(b): left side's groups first, then the right side's —
    // each side's winning derivation supplies its own spans.
    let re = Regex::new("(a)(b)")
        .unwrap()
        .and(Regex::new("a(b)").unwrap());
    assert_eq!(re.group_count(), 3);
    let caps = re.captures("xab").unwrap();
    assert_eq!(
        caps.matches,
        vec![
            Some(Match::new(1, 3)),
            Some(Match::new(1, 2)),
            Some(Match::new(2, 3)),
            Some(Match::new(2, 3))
        ]
    );

    // A group that does not participate in the winning derivation is None.
    let re = Regex::new("a(b)?c").unwrap().and(Regex::new("ac").unwrap());
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("xac").unwrap();
    assert_eq!(caps.matches, vec![Some(Match::new(1, 3)), None]);

    // One side with no groups contributes nothing.
    let re = Regex::new("ab").unwrap().and(Regex::new("a(b)").unwrap());
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("ab").unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 2)), Some(Match::new(1, 2))]
    );
}

#[test]
fn test_and_with_assertions() {
    // a(?=b) ∩ a: the assertion is checked against the FULL text at the
    // slice's position, on each side's own derivation.
    let re = Regex::new("a(?=b)").unwrap().and(Regex::new("a").unwrap());
    assert_eq!(re.find("xab"), Some(Match::new(1, 2)));
    assert_eq!(re.find("xax"), None);
    // Anchored, the lookahead sees no following `b`, so `a` is rejected.
    assert!(!re.is_match("a"));
    assert!(!re.is_match("ab"));
}

#[test]
fn test_and_with_not() {
    // not(a ∩ b): a ∩ b = ∅, so the complement is everything.
    let all = Regex::new("a").unwrap().and(Regex::new("b").unwrap()).not();
    assert!(all.is_match(""));
    assert!(all.is_match("x"));
    assert!(all.is_match("ab"));
    assert_eq!(all.find("ab"), Some(Match::new(0, 2)));

    // not(a) ∩ b = b.
    let re = Regex::new("a").unwrap().not().and(Regex::new("b").unwrap());
    assert!(re.is_match("b"));
    assert!(!re.is_match("a"));
    assert!(!re.is_match("ab"));
    assert!(!re.is_match(""));

    // A complemented side has no winning derivation: its groups are None.
    let re = Regex::new("a(b)")
        .unwrap()
        .not()
        .and(Regex::new("c").unwrap());
    assert_eq!(re.group_count(), 1);
    let caps = re.captures("xc").unwrap();
    assert_eq!(caps.matches, vec![Some(Match::new(1, 2)), None]);

    // Double negation cancels exactly, intersection included.
    let base = Regex::new("a").unwrap().and(Regex::new("b").unwrap());
    let round_trip = Regex::new("a")
        .unwrap()
        .and(Regex::new("b").unwrap())
        .not()
        .not();
    for text in ["", "a", "b", "ab", "x"] {
        assert_eq!(
            round_trip.is_match(text),
            base.is_match(text),
            "is_match({text})"
        );
    }
    assert_eq!(round_trip.find("cab"), None);
}

#[test]
fn test_and_epsilon() {
    let eps1 = Regex::from_pattern(RegExt::Pattern::epsilon()).unwrap();
    let eps2 = Regex::from_pattern(RegExt::Pattern::epsilon()).unwrap();
    let both = eps1.and(eps2);
    assert!(both.is_match(""));
    assert_eq!(both.find("x"), Some(Match::new(0, 0)));
    assert_eq!(both.find(""), Some(Match::new(0, 0)));
    let eps3 = Regex::from_pattern(RegExt::Pattern::epsilon()).unwrap();
    let mixed = eps3.and(Regex::new("a").unwrap());
    assert!(!mixed.is_match(""));
    assert!(!mixed.is_match("a"));
    assert_eq!(mixed.find("a"), None);
}

#[test]
fn test_and_pattern_string() {
    let re = Regex::new("a").unwrap().and(Regex::new("b").unwrap());
    assert_eq!(re.pattern(), "and(a, b)");
    let nested = Regex::new("a").unwrap().not().and(Regex::new("b").unwrap());
    assert_eq!(nested.pattern(), "and(not(a), b)");
}
