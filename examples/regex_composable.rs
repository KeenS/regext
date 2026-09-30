//! A regex built with the composable API: `Pattern` expressions combine
//! with `.or(...)` / `.then(...)` (plus `star`, `plus`, `opt`, `between`,
//! `group`, assertions, ...), and `Regex::from_pattern` compiles the tree
//! through the exact same pipeline as `Regex::new` — so a composable
//! pattern and its string form behave identically, asserted below.
//!
//! | string front end                          | composable front end                          |
//! |-------------------------------------------|-----------------------------------------------|
//! | `Regex::new("[+-]?\\d{1,3}")`             | `sign.opt().then(digit.between(1, 3))`        |
//! | `Regex::new("\\d{1,3}(\\.\\d{1,3}){3}")`  | `seg.clone().then(dot.clone()).then(seg...)`  |

use RegExt::{Match, Pattern, Regex};

/// `[+-]?\d{1,3}` — the optional sign is `or` + `opt`, the bound `between`.
fn signed_int() -> Regex {
    let sign = Pattern::lit("+").or(Pattern::lit("-"));
    Regex::from_pattern(sign.opt().then(Pattern::digit().between(1, 3)))
        .expect("signed int compiles")
}

/// `\d{1,3}(\.\d{1,3}){3}` — four segments, each a capture group; the
/// shared `seg` piece is cloned across positions.
fn ipv4() -> Regex {
    let seg = Pattern::digit().between(1, 3).group();
    let dot = Pattern::lit(".");
    Regex::from_pattern(
        seg.clone()
            .then(dot.clone())
            .then(seg.clone())
            .then(dot.clone())
            .then(seg.clone())
            .then(dot.clone())
            .then(seg),
    )
    .expect("ipv4 compiles")
}

fn main() {
    let int = signed_int();
    let ip = ipv4();

    // Parity with the string front end on a battery of inputs.
    let int_str = Regex::new("[+-]?\\d{1,3}").unwrap();
    let ip_str = Regex::new("\\d{1,3}(\\.\\d{1,3}){3}").unwrap();
    for t in [
        "42",
        "+17",
        "-123",
        "1234",
        "+",
        "-",
        "",
        "x42",
        "192.168.1.1",
        "1.2.3",
    ] {
        assert_eq!(int.is_match(t), int_str.is_match(t), "int is_match({t:?})");
        assert_eq!(ip.is_match(t), ip_str.is_match(t), "ip is_match({t:?})");
    }

    // Anchored semantics (the whole input must match).
    assert!(int.is_match("42"), "42");
    assert!(int.is_match("-123"), "-123");
    assert!(!int.is_match("1234"), "1234 breaks \\d{{1,3}}");
    assert!(!int.is_match("+"), "a sign alone carries no digits");
    assert!(ip.is_match("192.168.1.1"), "192.168.1.1");
    assert!(
        !ip.is_match("1234.1.1.1"),
        "a 4-digit segment breaks \\d{{1,3}}"
    );

    // Leftmost match anywhere, like `Regex::find`.
    assert_eq!(
        int.find("xx-7yy"),
        Some(Match::new(2, 4)),
        "find -7 in xx-7yy"
    );
    assert_eq!(
        ip.find("ip 10.0.0.1/8 done"),
        Some(Match::new(3, 11)),
        "find 10.0.0.1"
    );

    // Capture groups are the `.group()` pieces, in pattern order.
    assert_eq!(ip.group_count(), 4, "one group per segment");
    let caps = ip.captures("192.168.1.1").unwrap();
    assert_eq!(
        caps.matches,
        vec![
            Some(Match::new(0, 11)),
            Some(Match::new(0, 3)),
            Some(Match::new(4, 7)),
            Some(Match::new(8, 9)),
            Some(Match::new(10, 11)),
        ],
        "segments captured in pattern order"
    );

    // `not()` composes over the composable regex too.
    let not_int = int.not();
    assert!(!not_int.is_match("42"), "complement rejects");
    assert!(not_int.is_match("x42"), "complement accepts the rejected");

    // `and()` intersects two compiled regexes at the engine level.
    // signed_int ∩ unsigned_int is the unsigned 1-to-3-digit integers.
    let unsigned = Regex::from_pattern(Pattern::digit().between(1, 3)).expect("unsigned compiles");
    let both = signed_int().and(unsigned);
    assert!(both.is_match("42"), "unsigned ints are in both languages");
    assert!(both.is_match("123"), "unsigned ints are in both languages");
    assert!(
        !both.is_match("+17"),
        "a signed int is only in the signed side"
    );
    assert!(
        !both.is_match("-123"),
        "a signed int is only in the signed side"
    );
    assert!(
        !both.is_match("1234"),
        "four digits are in neither language"
    );
    assert_eq!(
        both.find("xx42yy"),
        Some(Match::new(2, 4)),
        "leftmost common slice"
    );

    println!("regex_composable: 38 assertions passed");
}
