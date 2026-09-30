use RegExt::{Captures, Match, Regex};

// Each `check_*` helper runs exactly one assertion and increments a shared
// counter, so the printed total is guaranteed to equal the checks performed.
static PASS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn tick() {
    PASS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn find_is(re: &Regex, text: &str, expected: Option<Match>, note: &str) {
    assert_eq!(re.find(text), expected, "{note}");
    tick();
}

fn match_is(re: &Regex, text: &str, expected: bool, note: &str) {
    assert_eq!(re.is_match(text), expected, "{note}");
    tick();
}

fn caps_is(re: &Regex, text: &str, expected: Option<Captures>, note: &str) {
    assert_eq!(re.captures(text), expected, "{note}");
    tick();
}

fn compile_err(pattern: &str, note: &str) {
    assert!(Regex::new(pattern).is_err(), "{note}");
    tick();
}

fn c(matches: Vec<Option<Match>>) -> Option<Captures> {
    Some(Captures { matches })
}

fn m(start: usize, end: usize) -> Option<Match> {
    Some(Match::new(start, end))
}

fn main() {
    // Character classes, aliases, bounded repetition, non-capturing and
    // possessive forms — all finite, all regular, all LALR(1) lowerings.

    // -- character classes ---------------------------------------------------
    let cls = Regex::new("[abc]").unwrap();
    match_is(&cls, "b", true, "member matches");
    match_is(&cls, "d", false, "non-member does not");

    let range = Regex::new("[a-z]+").unwrap();
    match_is(&range, "abc", true, "range run");
    find_is(
        &range,
        "1Xa2",
        Some(Match::new(2, 3)),
        "leftmost lowercase letter after a digit",
    );

    let neg = Regex::new("[^a-z]*").unwrap();
    match_is(&neg, "1 9", true, "non-letters only");
    match_is(&neg, "abc", false, "a letter breaks the negated class");

    let pipe = Regex::new("[a|b]").unwrap();
    match_is(&pipe, "|", true, "`|` inside a class is an ordinary member");

    let multi = Regex::new("[é]").unwrap();
    find_is(
        &multi,
        "aéb",
        Some(Match::new(1, 3)),
        "single-member class is char-exact",
    );

    // -- predefined aliases ----------------------------------------------------
    let digit = Regex::new(r"\d+").unwrap();
    match_is(&digit, "123", true, "`\\d` = [0-9]");
    match_is(&digit, "1a", false, "a letter is not a digit");

    let word = Regex::new(r"\w+").unwrap();
    match_is(&word, "aB_3", true, "`\\w` = [a-zA-Z0-9_]");
    match_is(&word, "a.b", false, "a dot is not a word character");

    // -- bounded repetition ----------------------------------------------------
    let exact = Regex::new("a{2}").unwrap();
    match_is(&exact, "aa", true, "exactly two");
    match_is(&exact, "aaa", false, "three is too many");

    let atleast = Regex::new("a{2,}").unwrap();
    match_is(&atleast, "aa", true, "at least two: exactly two is fine");
    match_is(&atleast, "aaaa", true, "at least two: more is fine");
    match_is(&atleast, "a", false, "at least two: one is not");

    let span = Regex::new("a{2,3}").unwrap();
    match_is(&span, "aaa", true, "range: three");
    match_is(&span, "aaaa", false, "range: four is outside");

    let binds = Regex::new("ab{2}").unwrap();
    match_is(&binds, "abb", true, "the bound binds to the last character");
    match_is(&binds, "ab", false, "one `b` is not two");

    let grp = Regex::new("(a){2}").unwrap();
    assert_eq!(grp.group_count(), 1, "one group despite two copies");
    caps_is(
        &grp,
        "baa",
        c(vec![m(1, 3), m(2, 3)]),
        "the group capture is the last copy",
    );

    // -- non-capturing groups ---------------------------------------------------
    let nocap = Regex::new("(?:a)(b)").unwrap();
    assert_eq!(nocap.group_count(), 1, "(?:...) does not consume a number");
    caps_is(
        &nocap,
        "ab",
        c(vec![m(0, 2), m(1, 2)]),
        "`(b)` is still group 1",
    );

    let chunk = Regex::new("(?:ab)+").unwrap();
    match_is(&chunk, "abab", true, "repeated non-capturing chunk");
    match_is(&chunk, "a", false, "half a chunk is not enough");

    // -- possessive sugar and atomic groups -------------------------------------
    let pstar = Regex::new("a*+").unwrap();
    match_is(
        &pstar,
        "aa",
        true,
        "possessive star behaves like a plain star",
    );

    let pplus = Regex::new("a++").unwrap();
    match_is(
        &pplus,
        "a",
        true,
        "possessive plus behaves like a plain plus",
    );

    let popt = Regex::new("a?+b").unwrap();
    match_is(
        &popt,
        "ab",
        true,
        "possessive optional behaves like a plain optional",
    );
    match_is(&popt, "a", false, "the `b` is still required");

    let atomic = Regex::new("(?>ab)+").unwrap();
    match_is(
        &atomic,
        "abab",
        true,
        "atomic group is sugar for non-capturing",
    );

    // -- honest rejections -------------------------------------------------------
    compile_err(
        r"\s",
        "whitespace is the token separator, so \\s is rejected",
    );
    compile_err(".", "any-character is rejected");
    compile_err("[a[b]", "a nested `[` is rejected");
    compile_err("a{2,1}", "an inverted bound is rejected");
    compile_err("a{101}", "a bound above the cap of 100 is rejected");
    compile_err("a{2}+", "a quantifier after a bound is rejected");
    compile_err("a*++", "a double possessive marker is rejected");
    compile_err("(?:)", "an empty non-capturing body is rejected");

    println!(
        "classes_demo: {} assertions passed",
        PASS.load(std::sync::atomic::Ordering::Relaxed)
    );
}
