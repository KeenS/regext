use parser_vm::{Captures, CfgParser, Match, Regex};

fn main() {
    // 1. Regex literals (whole-input matching)
    let re = Regex::new("test").unwrap();
    assert!(re.is_match("test"), "exact match");
    assert!(
        !re.is_match("a test b"),
        "whole-input semantics: substring must not match"
    );
    assert!(!re.is_match("abc"), "non-match");
    assert_eq!(
        re.find("a test b").map(|m| (m.start, m.end)),
        Some((2, 6)),
        "find the leftmost match"
    );
    assert_eq!(
        re.find("test").map(|m| (m.start, m.end)),
        Some((0, 4)),
        "find span"
    );

    // 2. Context-free mode: quoted terminals, bare nonterminals
    let parser = CfgParser::new(r#"E -> E "+" NUM | NUM; NUM -> "digit""#).unwrap();
    assert!(parser.is_match("digit"), "single NUM");
    assert!(parser.is_match("digit + digit"), "recursive E");
    assert!(
        !parser.is_match("digit +"),
        "trailing operator must be rejected"
    );

    // 3. Captures: the whole match plus, at the leftmost start, a partial
    //    match still captures (unlike the anchored `is_match`).
    let re = Regex::new("hello").unwrap();
    let caps = re.captures("hello").expect("captures on a match");
    assert_eq!(caps.matches, vec![Some(Match::new(0, 5))], "capture count");
    assert_eq!(
        re.captures("hello world"),
        Some(Captures {
            matches: vec![Some(Match::new(0, 5))]
        }),
        "leftmost capture on a longer input"
    );

    // 4. Groups: in `(ab)+` the group is the repeated unit, so it records the
    //    last iteration while the whole match spans the run.
    let re = Regex::new("(ab)+").unwrap();
    let caps = re.captures("abab").expect("captures on a group match");
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 4)), Some(Match::new(2, 4))],
        "last iteration of (ab)"
    );

    // 5. The compiler is honest about encodings that are not LALR(1).
    assert!(
        Regex::new("(a)+a").is_err(),
        "non-LALR(1) pattern must be rejected"
    );
    println!("regex_api_demo: 12 assertions passed");
}
