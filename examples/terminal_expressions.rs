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
    let parser = CfgParser::new(
        r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | NUM; NUM -> "1" | "2" | "3""#,
    )
    .unwrap();

    check(&parser, "number", "1", true);
    check(&parser, "add", "1 + 2", true);
    check(&parser, "mul", "1 * 2", true);
    check(&parser, "parens", "( 1 + 2 )", true);
    check(&parser, "precedence", "1 + 2 * 3", true);
    check(&parser, "number out of range", "7", false);
    check(&parser, "trailing operator", "1 +", false);

    println!("terminal_expressions: 7 assertions passed");
}
