use parser_vm::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(result, expected, "[{}] input '{}': expected {}", name, input, expected);
}

fn main() {
    // No spaces needed between tokens.
    let parser = CfgParser::new(
        r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | "num""#,
    ).unwrap();

    check(&parser, "number", "num", true);
    check(&parser, "add", "num+num", true);
    check(&parser, "mul", "num*num", true);
    check(&parser, "parens", "(num+num)", true);
    check(&parser, "mixed", "num+num*num", true);
    check(&parser, "parens mul", "(num+num)*num", true);

    println!("no_spaces_arithmetic: 6 assertions passed");
}
