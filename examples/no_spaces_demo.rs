use RegExt::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(
        result, expected,
        "[{}] input '{}': expected {}",
        name, input, expected
    );
}

fn main() {
    // The lexer splits input without requiring spaces between tokens.
    let parser = CfgParser::new(r#"S -> "(" S ")" | "a""#).unwrap();

    check(&parser, "atom", "a", true);
    check(&parser, "single parens", "(a)", true);
    check(&parser, "double parens", "((a))", true);
    check(&parser, "concatenated lists", "(a)(a)", false);

    println!("no_spaces_demo: 4 assertions passed");
}
