//! Composable API tests: regex patterns built from `Pattern` combinators
//! (`or` / `then` / quantifiers / groups / assertions) via
//! `Regex::from_pattern`, and CFG grammars built from `Expr` combinators
//! via `CfgParser::from_registry`. A composable construction must behave
//! exactly like its string-dialect twin.

use RegExt::{CfgParser, Expr, GrammarRegistry, Match, Pattern, Regex, RegexError};

/// `Regex` owns an LALR table (not `Debug`), so surface compile errors via
/// this helper instead of `unwrap_err`.
fn err_of(composable: Pattern) -> RegexError {
    match Regex::from_pattern(composable) {
        Ok(_) => panic!("pattern compiled but must be rejected"),
        Err(e) => e,
    }
}

fn invalid_pattern_err(e: &RegexError) -> &str {
    match e {
        RegexError::InvalidPattern(msg) => msg,
        RegexError::CompileError(msg) => panic!("expected InvalidPattern, got CompileError: {msg}"),
    }
}

/// Compare two regexes over a battery: `is_match`, `find`, and (when the
/// input matches) `captures`.
fn assert_parity(comp: &Regex, str_re: &Regex, battery: &[&str]) {
    assert_eq!(comp.group_count(), str_re.group_count());
    for t in battery {
        assert_eq!(comp.is_match(t), str_re.is_match(t), "is_match({t:?})");
        assert_eq!(comp.find(t), str_re.find(t), "find({t:?})");
        assert_eq!(comp.captures(t), str_re.captures(t), "captures({t:?})");
    }
}

// =============================================================================
// Regex: composable == string dialect
// =============================================================================

#[test]
fn composable_adjacent_literals_are_one_terminal() {
    // `ab` is one literal. `lit("a").then(lit("b"))` is the same terminal,
    // so a space between the letters is not skipped. A quantifier stops
    // the merge: `a` then `b*` stays two terminals and still skips the space.
    let ab = Regex::from_pattern(Pattern::lit("a").then(Pattern::lit("b"))).unwrap();
    let ab_str = Regex::new("ab").unwrap();
    assert_parity(&ab, &ab_str, &["", "a", "b", "ab", "a b", "ba"]);
    assert!(ab.is_match("ab"));
    assert!(!ab.is_match("a b"));

    let star = Regex::from_pattern(Pattern::lit("a").then(Pattern::lit("b").star())).unwrap();
    let star_str = Regex::new("ab*").unwrap();
    assert_parity(&star, &star_str, &["", "a", "ab", "abb", "a b", "b"]);
    assert!(star.is_match("a"));
    assert!(star.is_match("ab"));
    assert!(star.is_match("a b"));
    assert!(!star.is_match("b"));

    // Three adjacent literals stay one terminal, including a quoted pipe.
    let abc = Regex::from_pattern(
        Pattern::lit("a")
            .then(Pattern::lit("b"))
            .then(Pattern::lit("c")),
    )
    .unwrap();
    let abc_str = Regex::new("abc").unwrap();
    assert_parity(&abc, &abc_str, &["abc", "abc ", "a b c", "ab c", "a bc"]);

    let pipe = Regex::from_pattern(
        Pattern::lit("a")
            .then(Pattern::lit("|"))
            .then(Pattern::lit("b")),
    )
    .unwrap();
    let pipe_str = Regex::new(r"a\|b").unwrap();
    assert_parity(&pipe, &pipe_str, &["a|b", "ab", "a | b"]);

    // Alternation stops the merge: `(a|b)c` is two terminals.
    let alt = Regex::from_pattern(
        Pattern::lit("a")
            .or(Pattern::lit("b"))
            .then(Pattern::lit("c")),
    )
    .unwrap();
    let alt_str = Regex::new("(?:a|b)c").unwrap();
    assert_parity(
        &alt,
        &alt_str,
        &["", "a", "c", "ac", "bc", "a c", "abc", "ab"],
    );
}

#[test]
fn composable_literals_or_then() {
    let battery = ["", "a", "b", "ab", "aabb", "ba", "c"];
    let comp = Regex::from_pattern(
        Pattern::lit("a")
            .then(Pattern::lit("a").star())
            .or(Pattern::lit("b")),
    )
    .unwrap();
    let str_re = Regex::new("aa*|b").unwrap();
    assert_parity(&comp, &str_re, &battery);
    // And `(a|b)*` is a different language — composition keeps it distinct.
    let comp = Regex::from_pattern(
        Pattern::lit("a")
            .then(Pattern::lit("a").or(Pattern::lit("b")).group().star())
            .or(Pattern::lit("b")),
    )
    .unwrap();
    let str_re = Regex::new("a(a|b)*|b").unwrap();
    assert_parity(&comp, &str_re, &battery);
}

#[test]
fn composable_quantifiers() {
    let battery = ["", "a", "aa", "aaa", "aaaa", "b"];
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").star()).unwrap(),
        &Regex::new("a*").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").plus()).unwrap(),
        &Regex::new("a+").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").opt()).unwrap(),
        &Regex::new("a?").unwrap(),
        &battery,
    );
}

#[test]
fn composable_bounded_repetition() {
    let battery = ["", "a", "aa", "aaa", "aaaa"];
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").times(2)).unwrap(),
        &Regex::new("a{2}").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").at_least(2)).unwrap(),
        &Regex::new("a{2,}").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").between(1, 3)).unwrap(),
        &Regex::new("a{1,3}").unwrap(),
        &battery,
    );
    // `{0,m}` includes the empty derivation on both front ends.
    let comp = Regex::from_pattern(Pattern::lit("a").between(0, 3)).unwrap();
    let str_re = Regex::new("a{0,3}").unwrap();
    assert_parity(&comp, &str_re, &battery);
    assert!(comp.is_match(""));
}

#[test]
fn composable_classes_and_aliases() {
    let battery = ["a", "z", "q7", "9", "0", "_x", " ", "é", ""];
    assert_parity(
        &Regex::from_pattern(Pattern::class("a-z")).unwrap(),
        &Regex::new("[a-z]").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(Pattern::neg_class("0-9").star()).unwrap(),
        &Regex::new("[^0-9]*").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(Pattern::digit().plus()).unwrap(),
        &Regex::new("\\d+").unwrap(),
        &["1", "42", "", "1a", "12345"],
    );
    assert_parity(
        &Regex::from_pattern(Pattern::word().plus()).unwrap(),
        &Regex::new("\\w+").unwrap(),
        &battery,
    );
}

#[test]
fn composable_groups_numbered_in_pattern_order() {
    let battery = ["ab", "aabb", "c"];
    assert_parity(
        &Regex::from_pattern(
            Pattern::lit("a")
                .plus()
                .group()
                .then(Pattern::lit("b").plus().group()),
        )
        .unwrap(),
        &Regex::new("(a+)(b+)").unwrap(),
        &battery,
    );
    // `(a)+(b)+` groups each single iteration instead.
    assert_parity(
        &Regex::from_pattern(
            Pattern::lit("a")
                .group()
                .plus()
                .then(Pattern::lit("b").group().plus()),
        )
        .unwrap(),
        &Regex::new("(a)+(b)+").unwrap(),
        &battery,
    );
    // A non-capturing group consumes no number.
    let comp = Regex::from_pattern(
        Pattern::lit("a")
            .non_capturing()
            .plus()
            .then(Pattern::lit("b").group()),
    )
    .unwrap();
    let str_re = Regex::new("(?:a+)(b)").unwrap();
    assert_parity(&comp, &str_re, &battery);
    assert_eq!(comp.group_count(), 1);
    // `a(b)+c`: the group captures its LAST iteration, both front ends.
    assert_parity(
        &Regex::from_pattern(
            Pattern::lit("a")
                .then(Pattern::lit("b").group().plus())
                .then(Pattern::lit("c")),
        )
        .unwrap(),
        &Regex::new("a(b)+c").unwrap(),
        &["abbbc", "abbc", "abc"],
    );
    let caps = Regex::from_pattern(
        Pattern::lit("a")
            .then(Pattern::lit("b").group().plus())
            .then(Pattern::lit("c")),
    )
    .unwrap()
    .captures("abbbc")
    .unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 5)), Some(Match::new(3, 4))]
    );
}

#[test]
fn composable_assertions() {
    let battery = ["ab", "ac", "aab", "xab", "accc", ""];
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").then(Pattern::lit("b").lookahead())).unwrap(),
        &Regex::new("a(?=b)").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").then(Pattern::lit("b").neg_lookahead())).unwrap(),
        &Regex::new("a(?!b)").unwrap(),
        &battery,
    );
    let battery = ["b", "ab", "xaab", "xbb", ""];
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").lookbehind().then(Pattern::lit("b"))).unwrap(),
        &Regex::new("(?<=a)b").unwrap(),
        &battery,
    );
    // A bare lookbehind is zero-width and holds where its body precedes.
    assert_parity(
        &Regex::from_pattern(Pattern::lit("b").lookbehind()).unwrap(),
        &Regex::new("(?<=b)").unwrap(),
        &battery,
    );
    assert_parity(
        &Regex::from_pattern(
            Pattern::lit("a")
                .plus()
                .lookbehind()
                .then(Pattern::lit("b")),
        )
        .unwrap(),
        &Regex::new("(?<=a+)b").unwrap(),
        &battery,
    );
}

#[test]
fn composable_assertion_groups_capture() {
    // Group inside the assertion body first (pattern order), then the main
    // one — same numbering and spans as the string dialect.
    let battery = ["xabcd", "xabc", "abcd", ""];
    assert_parity(
        &Regex::from_pattern(
            Pattern::lit("x")
                .then(
                    Pattern::lit("a")
                        .then(Pattern::lit("b").group())
                        .lookahead(),
                )
                .then(Pattern::lit("c").then(Pattern::lit("d").group())),
        )
        .unwrap(),
        &Regex::new("x(?=a(b))c(d)").unwrap(),
        &battery,
    );
}

#[test]
fn composable_not_complement() {
    let battery = ["", "a", "aa", "b", "aab", "ab"];
    let comp = Regex::from_pattern(Pattern::lit("a").star()).unwrap().not();
    let str_re = Regex::new("a*").unwrap().not();
    for t in battery {
        assert_eq!(comp.is_match(t), str_re.is_match(t), "is_match({t:?})");
        assert_eq!(comp.find(t), str_re.find(t), "find({t:?})");
    }
}

#[test]
fn composable_rejections() {
    // Inverted or over-cap bounds: same verdicts as the string dialect.
    let err = err_of(Pattern::lit("a").between(3, 2));
    assert!(invalid_pattern_err(&err).contains("upper bound below its lower"));
    let err = err_of(Pattern::lit("a").times(101));
    assert!(invalid_pattern_err(&err).contains("101"));
    let err = err_of(Pattern::lit("a").at_least(101));
    assert!(invalid_pattern_err(&err).contains("101"));
    // A memberless class (the string dialect rejects `[` `]`).
    assert!(invalid_pattern_err(&err_of(Pattern::class(""))).contains("empty class"));
    assert!(invalid_pattern_err(&err_of(Pattern::neg_class(""))).contains("empty class"));
    // Nested assertions.
    let nested = Pattern::lit("a").then(Pattern::lit("b").lookahead().lookahead());
    let err = err_of(nested);
    assert!(invalid_pattern_err(&err).contains("assertion inside an assertion"));
    // A pattern whose encoding is not conflict-free LALR(1).
    let conflict = Pattern::lit("a").group().plus().then(Pattern::lit("a"));
    match Regex::from_pattern(conflict) {
        Ok(_) => panic!("expected CompileError"),
        Err(RegexError::CompileError(msg)) => assert!(msg.contains("conflict"), "{msg}"),
        Err(RegexError::InvalidPattern(msg)) => panic!("expected CompileError, got {msg}"),
    }
    // `a**` through the composable front end is the same non-LALR grammar.
    match Regex::from_pattern(Pattern::lit("a").star().star()) {
        Ok(_) => panic!("star().star() must be a CompileError"),
        Err(RegexError::CompileError(_)) => {}
        Err(RegexError::InvalidPattern(msg)) => panic!("expected CompileError, got {msg}"),
    }
}

// =============================================================================
// Regex: composable-only spelling of the empty pattern
// =============================================================================

#[test]
fn composable_epsilon() {
    // `a | <empty>` is the composable `a?`.
    assert_parity(
        &Regex::from_pattern(Pattern::lit("a").or(Pattern::epsilon())).unwrap(),
        &Regex::new("a?").unwrap(),
        &["", "a", "aa"],
    );
    // Quantifiers over the empty pattern are just the empty pattern.
    assert!(
        Regex::from_pattern(Pattern::epsilon().star())
            .unwrap()
            .is_match("")
    );
    assert!(
        !Regex::from_pattern(Pattern::epsilon().star())
            .unwrap()
            .is_match("a")
    );
    assert!(
        Regex::from_pattern(Pattern::epsilon().plus())
            .unwrap()
            .is_match("")
    );
    // `lit("")` normalizes to epsilon; epsilon is a sequence identity.
    let mixed = Regex::from_pattern(
        Pattern::lit("a")
            .then(Pattern::epsilon().star())
            .then(Pattern::lit("b")),
    )
    .unwrap();
    assert!(mixed.is_match("ab"));
    assert!(!mixed.is_match("a"));
    assert!(Regex::from_pattern(Pattern::lit("")).unwrap().is_match(""));
    // A capture group over epsilon lowers to `epsilon` (still captures).
    let re =
        Regex::from_pattern(Pattern::lit("a").then(Pattern::epsilon().group()).plus()).unwrap();
    assert!(re.is_match("aa"));
}

// =============================================================================
// CFG: Expr combinators
// =============================================================================

#[test]
fn expr_or_then_flatten() {
    // Flattened choice and sequence, in either association order.
    let flat = Expr::str("a").or(Expr::str("b")).or(Expr::str("c"));
    let nested = Expr::str("a").or(Expr::str("b").or(Expr::str("c")));
    let mut left = GrammarRegistry::new();
    let mut right = GrammarRegistry::new();
    flat.named("S", &mut left).unwrap();
    nested.named("S", &mut right).unwrap();
    assert_eq!(left.productions, right.productions);

    let flat = Expr::str("a").then(Expr::str("b")).then(Expr::str("c"));
    let nested = Expr::str("a").then(Expr::str("b").then(Expr::str("c")));
    let mut left = GrammarRegistry::new();
    let mut right = GrammarRegistry::new();
    flat.named("S", &mut left).unwrap();
    nested.named("S", &mut right).unwrap();
    assert_eq!(left.productions, right.productions);

    // Epsilon behaves as the identity for `then`.
    let with_eps = Expr::str("a").then(Expr::epsilon()).then(Expr::str("b"));
    let plain = Expr::str("a").then(Expr::str("b"));
    let mut left = GrammarRegistry::new();
    let mut right = GrammarRegistry::new();
    with_eps.named("S", &mut left).unwrap();
    plain.named("S", &mut right).unwrap();
    assert_eq!(left.productions, right.productions);
}

#[test]
fn from_registry_matches_grammar_string() {
    // The same arithmetic grammar, two ways: composable `Expr` and the
    // grammar-string front end.
    let e = Expr::call("E")
        .then(Expr::str("+"))
        .then(Expr::call("T"))
        .or(Expr::call("T"));
    let t = Expr::call("T")
        .then(Expr::str("*"))
        .then(Expr::call("F"))
        .or(Expr::call("F"));
    let f = Expr::str("(")
        .then(Expr::call("E"))
        .then(Expr::str(")"))
        .or(Expr::str("1"))
        .or(Expr::str("2"));
    let mut registry = GrammarRegistry::new();
    e.named("E", &mut registry).unwrap();
    t.named("T", &mut registry).unwrap();
    f.named("F", &mut registry).unwrap();
    let comp = CfgParser::from_registry(registry).unwrap();

    let str_parser =
        CfgParser::new(r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | "1" | "2""#)
            .unwrap();

    for t in [
        "1",
        "2",
        "1 + 2",
        "1 * 2",
        "1 + 2 * 1",
        "(1 + 2) * 1",
        "1 +",
        "1 *",
        "x",
        "",
        "xy 1 * 2 z",
    ] {
        assert_eq!(comp.is_match(t), str_parser.is_match(t), "is_match({t:?})");
        assert_eq!(comp.find(t), str_parser.find(t), "find({t:?})");
    }
    assert_eq!(
        comp.captures("1 + 2 * 1", &["T"]).unwrap().matches,
        str_parser.captures("1 + 2 * 1", &["T"]).unwrap().matches,
    );
}

#[test]
fn from_registry_errors_and_epsilon() {
    // An empty registry has no start symbol.
    if CfgParser::from_registry(GrammarRegistry::new()).is_ok() {
        panic!("empty registry compiled but must be rejected");
    }
    // `S -> epsilon`.
    let mut registry = GrammarRegistry::new();
    Expr::epsilon().named("S", &mut registry).unwrap();
    let parser = CfgParser::from_registry(registry).unwrap();
    assert!(parser.is_match(""));
    assert!(!parser.is_match("a"));
}

#[test]
fn named_expr_or_then() {
    // `expr(...)` string pieces combine with composable pieces.
    let mut registry = GrammarRegistry::new();
    RegExt::expr("G").named("S", &mut registry).unwrap();
    RegExt::expr(r#""a" | "b""#)
        .or(Expr::str("c"))
        .named("G", &mut registry)
        .unwrap();
    let parser = CfgParser::from_registry(registry).unwrap();
    assert!(parser.is_match("a"));
    assert!(parser.is_match("c"));
    assert!(!parser.is_match("d"));
}

// =============================================================================
// CFG: Expr quantifiers — star / plus / opt / bounds, grouped choices,
// class sugar, bound rejections (the `Pattern` surface, CFG-side)
// =============================================================================

/// Compare two CFG parsers over a battery: `is_match` and `find` (and
/// `captures` for the given group names).
fn assert_parity_cfg(comp: &CfgParser, str_parser: &CfgParser, battery: &[&str], groups: &[&str]) {
    for t in battery {
        assert_eq!(comp.is_match(t), str_parser.is_match(t), "is_match({t:?})");
        assert_eq!(comp.find(t), str_parser.find(t), "find({t:?})");
        if !groups.is_empty() {
            assert_eq!(
                comp.captures(t, groups),
                str_parser.captures(t, groups),
                "captures({t:?})"
            );
        }
    }
}

#[test]
fn expr_quantifiers_match_string_grammar() {
    let battery = ["", "a", "aa", "aaa", "b", "aab", "baa"];

    // a* — the star's fresh nonterminal is registered alongside.
    let mut reg = GrammarRegistry::new();
    Expr::str("a").star().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> "a" A | epsilon"#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // a+
    let mut reg = GrammarRegistry::new();
    Expr::str("a").plus().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> "a" A | "a""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // a?
    let mut reg = GrammarRegistry::new();
    Expr::str("a").opt().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> epsilon | "a""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // a{2}
    let mut reg = GrammarRegistry::new();
    Expr::str("a").times(2).named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> "a" "a""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // a{2,} — two copies plus a star tail.
    let mut reg = GrammarRegistry::new();
    Expr::str("a").at_least(2).named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> "a" "a" E; E -> "a" E | epsilon"#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // a{1,3} — one nonterminal with three alternatives.
    let mut reg = GrammarRegistry::new();
    Expr::str("a").between(1, 3).named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> "a" | "a" "a" | "a" "a" "a""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // a{0} is the empty symbol; {0,m} includes the empty derivation.
    let mut reg = GrammarRegistry::new();
    Expr::str("a").times(0).named("S", &mut reg).unwrap();
    assert!(CfgParser::from_registry(reg).unwrap().is_match(""));
    let mut reg = GrammarRegistry::new();
    Expr::str("a").between(0, 2).named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> epsilon | "a" | "a" "a""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);
}

#[test]
fn expr_grouped_choice_in_sequence() {
    // (a|b)c — a choice inside a sequence: used to panic on registration,
    // now lowers to a fresh nonterminal like the `Pattern` lowerer's groups.
    let battery = ["", "a", "c", "ac", "bc", "abc", "bca", "a c b"];
    let mut reg = GrammarRegistry::new();
    Expr::str("a")
        .or(Expr::str("b"))
        .then(Expr::str("c"))
        .named("S", &mut reg)
        .unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"S -> G "c"; G -> "a" | "b""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // (ab|c)+ — a choice of sequences, quantified.
    let battery = ["", "ab", "c", "abc", "cabc", "abab", "acb", "cab"];
    let mut reg = GrammarRegistry::new();
    Expr::str("a")
        .then(Expr::str("b"))
        .or(Expr::str("c"))
        .plus()
        .named("S", &mut reg)
        .unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"S -> G S | G; G -> "a" "b" | "c""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // (a|b)* — a quantified choice of literals.
    let mut reg = GrammarRegistry::new();
    Expr::str("a")
        .or(Expr::str("b"))
        .star()
        .named("S", &mut reg)
        .unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"S -> G S | epsilon; G -> "a" | "b""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    // Nested quantifiers: (a*)* = a*.
    let mut reg = GrammarRegistry::new();
    Expr::str("a").star().star().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"A -> "a" A | epsilon"#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &["", "a", "aa", "aaa", "b"], &[]);
}

#[test]
fn expr_classes_and_aliases() {
    let battery = ["", "1", "42", "1a", "12345", "x", "é", " 1 ", "1 2"];
    let mut reg = GrammarRegistry::new();
    Expr::digit().plus().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"S -> "[0-9]" S | "[0-9]""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    let battery = ["", "a", "z", "q7", "9", "0", "_x", "é", " 7 "];
    let mut reg = GrammarRegistry::new();
    Expr::neg_class("0-9").star().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"S -> "[^0-9]" S | epsilon"#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &[]);

    let mut reg = GrammarRegistry::new();
    Expr::word().opt().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"S -> "[a-zA-Z0-9_]" | epsilon"#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &["", "a", "x_", "9", "é"], &[]);
}

#[test]
fn expr_generated_nonterminal_captures() {
    // The star's fresh nonterminal (N1 — first allocation in the registry)
    // is a real production LHS: it can be captured like any named piece,
    // the CFG analog of a regex star-group.
    let battery = ["", "b", "bb", "bbbc", "xbbbb"];
    let mut reg = GrammarRegistry::new();
    Expr::str("b").plus().named("S", &mut reg).unwrap();
    let comp = CfgParser::from_registry(reg).unwrap();
    let str_parser = CfgParser::new(r#"S -> N1; N1 -> "b" N1 | "b""#).unwrap();
    assert_parity_cfg(&comp, &str_parser, &battery, &["N1"]);
}

#[test]
fn expr_bound_rejections() {
    // Inverted or over-cap bounds are refused at registration — the CFG
    // analog of `RegexError::InvalidPattern` for the `Pattern` front end.
    let mut reg = GrammarRegistry::new();
    let err = Expr::str("a").times(101).named("S", &mut reg).unwrap_err();
    assert!(err.to_string().contains("101"), "{}", err);
    let mut reg = GrammarRegistry::new();
    let err = Expr::str("a")
        .at_least(101)
        .named("S", &mut reg)
        .unwrap_err();
    assert!(err.to_string().contains("101"), "{}", err);
    let mut reg = GrammarRegistry::new();
    let err = Expr::str("a")
        .between(3, 2)
        .named("S", &mut reg)
        .unwrap_err();
    assert!(
        err.to_string().contains("upper bound below its lower"),
        "{err}"
    );
    // A refused registration leaves no production behind.
    assert!(reg.productions.is_empty());
    // The 100-copy cap itself still compiles.
    let mut reg = GrammarRegistry::new();
    Expr::str("a").times(100).named("S", &mut reg).unwrap();
    assert!(reg.productions.iter().any(|(_, rhs)| rhs.len() == 100));
    // An empty terminal is refused at registration and leaves no production.
    let mut reg = GrammarRegistry::new();
    let err = Expr::str("").named("S", &mut reg).unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
    assert!(reg.productions.is_empty());
    // An inverted class is stored as a terminal and rejected when compiled.
    let mut reg = GrammarRegistry::new();
    Expr::class("z-a").named("S", &mut reg).unwrap();
    match CfgParser::from_registry(reg) {
        Ok(_) => panic!("an inverted class must be rejected at compile"),
        Err(err) => assert!(err.to_string().contains("inverted"), "{err}"),
    }
}

#[test]
fn named_expr_quantifiers() {
    // `expr(...)` string pieces take the quantifier combinators too.
    let mut reg = GrammarRegistry::new();
    RegExt::expr(r#""a" | "b""#)
        .star()
        .named("S", &mut reg)
        .unwrap();
    let parser = CfgParser::from_registry(reg).unwrap();
    assert!(parser.is_match(""));
    assert!(parser.is_match("abba"));
    assert!(!parser.is_match("ac"));
    // The empty match at 0 beats the run at 1 (leftmost start wins).
    assert_eq!(parser.find("xabb"), Some(Match::new(0, 0)));
    assert_eq!(parser.find("aabb"), Some(Match::new(0, 4)));

    let mut reg = GrammarRegistry::new();
    RegExt::expr(r#""a" "b""#)
        .times(2)
        .or(RegExt::expr(r#""c""#))
        .named("S", &mut reg)
        .unwrap();
    let parser = CfgParser::from_registry(reg).unwrap();
    assert!(parser.is_match("abab"));
    assert!(parser.is_match("c"));
    assert!(!parser.is_match("ab"));
}

#[test]
fn cfg_expr_parity_with_pattern() {
    // The same expression spelled in both composable dialects is the same
    // language — `Pattern` (regex front end) and `Expr` (CFG front end).
    let battery = ["", "a", "aa", "aaa", "b", "ab", "ba", "c"];
    let cases: Vec<(Expr, Pattern)> = vec![
        (Expr::str("a").star(), Pattern::lit("a").star()),
        (Expr::str("a").plus(), Pattern::lit("a").plus()),
        (Expr::str("a").opt(), Pattern::lit("a").opt()),
        (Expr::str("a").times(2), Pattern::lit("a").times(2)),
        (Expr::str("a").at_least(2), Pattern::lit("a").at_least(2)),
        (
            Expr::str("a").between(1, 3),
            Pattern::lit("a").between(1, 3),
        ),
        (
            Expr::str("a").or(Expr::str("b")).star(),
            Pattern::lit("a").or(Pattern::lit("b")).star(),
        ),
        (
            Expr::str("a").or(Expr::str("b")).then(Expr::str("c")),
            Pattern::lit("a")
                .or(Pattern::lit("b"))
                .then(Pattern::lit("c")),
        ),
    ];
    for (e, p) in cases {
        let mut reg = GrammarRegistry::new();
        e.named("S", &mut reg).unwrap();
        let comp = CfgParser::from_registry(reg).unwrap();
        let re = Regex::from_pattern(p).unwrap();
        for t in battery {
            assert_eq!(comp.is_match(t), re.is_match(t), "is_match({t:?})");
            assert_eq!(comp.find(t), re.find(t), "find({t:?})");
        }
    }
}
