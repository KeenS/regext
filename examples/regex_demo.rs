use RegExt::{Match, Regex};

fn main() {
    // The Regex API mirrors the regex-crate surface — whole-input `is_match`,
    // leftmost `find`, and `captures` with group spans — but everything runs
    // on the LALR(1) CFG engine (a pattern is a restricted, LALR(1) grammar).
    let re = Regex::new("test").unwrap();

    assert!(re.is_match("test"), "exact match must succeed");
    assert!(!re.is_match("abc"), "non-matching input must fail");
    assert!(
        !re.is_match("a test b"),
        "whole-input semantics: substring must not match"
    );
    assert!(
        !re.is_match("testing"),
        "whole-input semantics: longer input must not match"
    );

    assert_eq!(
        re.find("test").map(|m| (m.start, m.end)),
        Some((0, 4)),
        "find spans the whole input"
    );
    assert_eq!(re.find("abc"), None, "find must return None on non-match");
    assert_eq!(
        re.find("a test b").map(|m| (m.start, m.end)),
        Some((2, 6)),
        "find locates the leftmost match"
    );

    let caps = re
        .captures("test")
        .expect("captures must succeed on a match");
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 4))],
        "one match capture"
    );
    assert_eq!(
        re.captures("abc"),
        None,
        "captures must return None on non-match"
    );

    // Capture groups: in `(a)+` the group is the repeated unit, so it records
    // the LAST iteration, while the whole match spans the run.
    let rep = Regex::new("(a)+b").unwrap();
    let caps = rep.captures("xaab").expect("captures on a group match");
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(1, 4)), Some(Match::new(2, 3))],
        "group span is the last iteration"
    );

    // The compiler is honest: a pattern whose encoding is not LALR(1) is
    // rejected instead of silently picking a parse.
    assert!(
        Regex::new("(a)+a").is_err(),
        "a non-LALR(1) encoding must be rejected"
    );

    println!("regex_demo: 11 assertions passed");
}
