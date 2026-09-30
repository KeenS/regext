//! Extract the text of the innermost `<div>...</div>` blocks from nested
//! markup — the task naive regexes usually get wrong.
//!
//! The key observation: an *innermost* div is precisely a div whose
//! content contains no `<` at all — any `<` would open a nested tag.
//! "A div with no nested divs" is a REGULAR language, so the regex
//! dialect can find one; what is not regular is matching *across* the
//! nesting, which is the CFG layer's job (see `json_parser` /
//! `s_expression_parser`).
//!
//! Part 1: the composable regex API extracts every innermost text.
//! Part 2: the composable CFG API validates the nesting and cross-checks.

use RegExt::{CfgParser, Expr, GrammarRegistry, Match, Pattern, Regex};

/// `<div>` TEXT `</div>` where TEXT contains no `<`: a div that cannot
/// contain another div — i.e. an innermost div.
fn innermost_div() -> Regex {
    Regex::from_pattern(
        Pattern::lit("<div>")
            .then(Pattern::neg_class("<").star())
            .then(Pattern::lit("</div>")),
    )
    .expect("innermost div pattern compiles")
}

/// Every innermost div's text, in document order: find the leftmost one,
/// strip the tags, and keep going on the remainder.
fn innermost_texts(re: &Regex, text: &str) -> Vec<String> {
    let mut texts = Vec::new();
    let mut rest = text;
    while let Some(m) = re.find(rest) {
        let inner = &rest[m.start + 5..m.end - 6]; // strip `<div>` / `</div>`
        texts.push(inner.trim().to_string());
        rest = &rest[m.end..];
    }
    texts
}

/// Properly nested divs, well-formedness included — the composable CFG API:
///
/// ```text
/// DOC   -> BODY
/// BODY  -> BLOCK BODY | epsilon
/// BLOCK -> DIV | WORD
/// DIV   -> "<div>" BODY "</div>"
/// WORD  -> "[a-zA-Z0-9]+"
/// ```
fn div_parser() -> CfgParser {
    let doc = Expr::call("BODY");
    let body = Expr::call("BLOCK")
        .then(Expr::call("BODY"))
        .or(Expr::epsilon());
    let block = Expr::call("DIV").or(Expr::call("WORD"));
    let div = Expr::str("<div>")
        .then(Expr::call("BODY"))
        .then(Expr::str("</div>"));
    let word = Expr::str("[a-zA-Z0-9]+");

    let mut registry = GrammarRegistry::new();
    doc.named("DOC", &mut registry).unwrap();
    body.named("BODY", &mut registry).unwrap();
    block.named("BLOCK", &mut registry).unwrap();
    div.named("DIV", &mut registry).unwrap();
    word.named("WORD", &mut registry).unwrap();
    CfgParser::from_registry(registry).expect("div grammar compiles")
}

/// The innermost divs' texts, read off the winning derivation: a DIV
/// reduction is innermost iff no other DIV reduction sits strictly inside it.
fn cfg_innermost_texts(parser: &CfgParser, text: &str) -> Option<Vec<String>> {
    let spans = parser.token_spans(text);
    let n = spans.len().saturating_sub(1);
    let reductions = parser.slice_reductions(text, 0, n)?;
    let divs: Vec<(usize, usize)> = reductions
        .iter()
        .filter(|r| r.lhs == "DIV")
        .map(|r| (r.start, r.end))
        .collect();
    let innermost: Vec<(usize, usize)> = divs
        .iter()
        .copied()
        .filter(|d| !divs.iter().any(|o| *o != *d && o.0 > d.0 && o.1 < d.1))
        .collect();
    Some(
        innermost
            .iter()
            .map(|(s, e)| {
                let m = if s == e {
                    Match::new(spans[*s].start, spans[*s].start)
                } else {
                    Match::new(spans[*s].start, spans[e - 1].end)
                };
                text[m.start + 5..m.end - 6].trim().to_string()
            })
            .collect(),
    )
}

fn main() {
    let re = innermost_div();
    let parser = div_parser();

    // Part 1 — the regex layer: find the leftmost innermost div, repeat.
    let nested = "<div>outer <div>mid <div>core</div> tail</div> end</div>";
    assert_eq!(innermost_texts(&re, nested), vec!["core"]);

    let siblings = "<div>a <div>one</div> b <div>two</div> c</div>";
    assert_eq!(innermost_texts(&re, siblings), vec!["one", "two"]);

    let flat = "<div>hello</div> <div>world</div>";
    assert_eq!(innermost_texts(&re, flat), vec!["hello", "world"]);

    assert_eq!(innermost_texts(&re, "<div></div>"), vec![""]);

    // The regex layer honestly cannot reach across the nesting.
    assert!(
        !re.is_match("<div>a<div>b</div>c</div>"),
        "a div containing a div is not in the pattern's language"
    );

    // Part 2 — the CFG layer: the nesting itself is well-formedness, which
    // a regular pattern cannot express.
    assert_eq!(
        parser.conflict_count(),
        0,
        "the div grammar is conflict-free LALR(1)"
    );
    assert!(
        parser.is_match("<div>a<div>b</div>c</div>"),
        "proper nesting"
    );
    assert!(!parser.is_match("<div>a<div>b</div>"), "unclosed div");
    assert!(!parser.is_match("<div>a</div></div>"), "extra closing tag");

    // ...and its derivation agrees with the regex layer's extraction.
    assert_eq!(
        cfg_innermost_texts(&parser, nested),
        Some(vec!["core".into()])
    );
    assert_eq!(
        cfg_innermost_texts(&parser, siblings),
        Some(vec!["one".into(), "two".into()])
    );
    assert_eq!(
        cfg_innermost_texts(&parser, flat),
        Some(vec!["hello".into(), "world".into()])
    );
    assert_eq!(
        cfg_innermost_texts(&parser, "<div>a<div>b</div>"),
        None,
        "malformed markup has no winning derivation"
    );

    println!("innermost_div: 13 assertions passed");
}
