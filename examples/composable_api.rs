//! The composable CFG API: `Expr` pieces combine with `.or(...)` (choice)
//! and `.then(...)` (juxtaposition); the first piece registered is the
//! start symbol; and `CfgParser::from_registry` turns the composed grammar
//! into a full `CfgParser` (`is_match` / `find` / `captures`) — the same
//! engine as `CfgParser::new`, built from pieces instead of one string.
//!
//! The grammar:
//!
//! ```text
//! E -> E "+" T | T
//! T -> T "*" F | F
//! F -> "(" E ")" | "1" | "2" | "3"
//! ```

use RegExt::{CfgParser, Expr, GrammarRegistry, Match};

fn main() {
    // One composable piece per nonterminal: `or` = `|`, `then` = sequence.
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
        .or(Expr::str("2"))
        .or(Expr::str("3"));

    // First registration is the start symbol (E).
    let mut registry = GrammarRegistry::new();
    e.named("E", &mut registry).unwrap();
    t.named("T", &mut registry).unwrap();
    f.named("F", &mut registry).unwrap();

    let parser = CfgParser::from_registry(registry).unwrap();

    // Anchored, whole input.
    assert!(parser.is_match("1 + 2 * 3"), "addition and multiplication");
    assert!(parser.is_match("(1 + 2) * 1"), "parentheses");
    assert!(!parser.is_match("1 +"), "dangling operator");
    assert!(!parser.is_match("4"), "4 is not a terminal");

    // Leftmost match anywhere.
    assert_eq!(
        parser.find("xy 1 * 2 z"),
        Some(Match::new(3, 8)),
        "find 1 * 2"
    );

    // Captures: the last `T` reduction in the winning derivation.
    let caps = parser.captures("1 + 2 * 3", &["T"]).unwrap();
    assert_eq!(
        caps.matches,
        vec![Some(Match::new(0, 9)), Some(Match::new(4, 9))],
        "T captures 2 * 3"
    );

    // Parity with the grammar-string front end.
    let str_parser =
        CfgParser::new(r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | "1" | "2" | "3""#)
            .unwrap();
    for t in [
        "1",
        "3",
        "1 + 2",
        "1 * 2 * 3",
        "(1 + 2) * 1",
        "1 +",
        "1 *",
        "x 1 z",
    ] {
        assert_eq!(
            parser.is_match(t),
            str_parser.is_match(t),
            "is_match({t:?})"
        );
        assert_eq!(parser.find(t), str_parser.find(t), "find({t:?})");
    }

    println!("composable_api: 22 assertions passed");
}
