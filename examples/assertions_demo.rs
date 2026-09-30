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
    // Zero-width assertions on RegExt's LALR(1) engine. A lookahead needs its
    // body to match a PREFIX of the remainder at the assertion position; a
    // lookbehind needs its body to match a SUFFIX of the consumed text ending
    // there. Assertions consume nothing, so the match span is exactly the
    // tokens the main grammar consumes.

    // -- positive lookahead (?=) -------------------------------------------
    let la = Regex::new("a(?=b)").unwrap();
    find_is(
        &la,
        "ab",
        Some(Match::new(0, 1)),
        "lookahead matches the `a`, not the `b`",
    );
    find_is(&la, "ac", None, "no `b` after `a` -> no match");
    find_is(
        &la,
        "xab",
        Some(Match::new(1, 2)),
        "leftmost satisfying `a`",
    );
    match_is(
        &la,
        "ab",
        false,
        "anchored: only `a` is consumed, `b` is not",
    );

    // -- lookahead with a greedy body ---------------------------------------
    let star = Regex::new("a*(?=b)").unwrap();
    find_is(
        &star,
        "xaab",
        Some(Match::new(1, 3)),
        "greedy run, asserting the `b`",
    );
    find_is(&star, "xaa", None, "no `b` to assert against");

    // -- negative lookahead (?! ) -------------------------------------------
    let nla = Regex::new("a(?!b)c").unwrap();
    match_is(&nla, "ac", true, "`b` does not follow, so `c` is taken");
    match_is(&nla, "abc", false, "`b` does follow, so rejected");

    // -- positive lookbehind (?<=) ------------------------------------------
    let lb = Regex::new("(?<=a)b").unwrap();
    find_is(
        &lb,
        "xab",
        Some(Match::new(2, 3)),
        "match is the `b` after an `a`",
    );
    find_is(&lb, "xbb", None, "no preceding `a`");

    // -- negative lookbehind (?<! ) -----------------------------------------
    let nlb = Regex::new("(?<!a)b").unwrap();
    find_is(
        &nlb,
        "xbb",
        Some(Match::new(1, 2)),
        "first `b` not after an `a`",
    );
    match_is(
        &nlb,
        "b",
        true,
        "standalone `b` satisfies the negative lookbehind",
    );

    // -- lookbehind with a greedy body --------------------------------------
    let lbg = Regex::new("(?<=a+)b").unwrap();
    find_is(
        &lbg,
        "xaab",
        Some(Match::new(3, 4)),
        "lookbehind reaches the full run",
    );

    // -- capture groups inside assertion bodies -----------------------------
    let grp = Regex::new("(?<=(a+))b").unwrap();
    caps_is(
        &grp,
        "xaab",
        c(vec![m(3, 4), m(1, 3)]),
        "inner group captures the run",
    );

    // -- a skipped assertion branch is vacuous ------------------------------
    let alt = Regex::new("a|(?=b)(c)").unwrap();
    caps_is(
        &alt,
        "a",
        c(vec![m(0, 1), None]),
        "skipped branch: group is None",
    );

    // -- multibyte offsets stay byte-based ----------------------------------
    let mb = Regex::new("a(?=é)").unwrap();
    find_is(
        &mb,
        "aéb",
        Some(Match::new(0, 1)),
        "byte offsets around multibyte body",
    );

    // -- honest rejections --------------------------------------------------
    compile_err("a(?=b(?=c))", "nested assertion is not supported");
    compile_err("(?=(a)+a)", "a non-LALR(1) assertion body is refused");

    println!(
        "assertions_demo: {} assertions passed",
        PASS.load(std::sync::atomic::Ordering::Relaxed)
    );
}
