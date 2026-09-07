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
    // Arithmetic: terminals are quoted ("+"), non-terminals bare (E, T, F, NUM).
    let parser = CfgParser::new(
        r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | NUM; NUM -> "1" | "2" | "3" | "4" | "5""#,
    ).unwrap();

    check(&parser, "number", "1", true);
    check(&parser, "addition", "1 + 2", true);
    check(&parser, "multiplication", "2 * 3", true);
    check(&parser, "precedence", "1 + 2 * 3", true);
    check(&parser, "parentheses", "( 1 + 2 )", true);
    check(&parser, "complex", "1 + 2 * 3 + 4", true);
    check(&parser, "nested parentheses", "( ( 1 + 2 ) * 3 )", true);
    check(&parser, "trailing operator", "1 +", false);
    check(&parser, "unknown operator", "1 ? 2", false);

    println!("arithmetic_parser: 9 assertions passed");
}
