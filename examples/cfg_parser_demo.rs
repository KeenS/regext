use parser_vm::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(
        result, expected,
        "[{}] input '{}': expected {}",
        name, input, expected
    );
}

fn main() {
    // 1. S-expressions
    let sexp = CfgParser::new(
        r#"SExp -> "(" SExpList ")" | Atom; SExpList -> SExp SExpList | SExp; Atom -> "foo" | "bar" | "1""#,
    ).unwrap();

    check(&sexp, "atom", "foo", true);
    check(&sexp, "list", "( foo )", true);
    check(&sexp, "nested", "( foo ( bar ) )", true);
    check(&sexp, "unknown atom", "baz", false);

    // 2. Arithmetic
    let arith = CfgParser::new(
        r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | NUM; NUM -> "1" | "2""#,
    )
    .unwrap();

    check(&arith, "number", "1", true);
    check(&arith, "add", "1 + 2", true);
    check(&arith, "mul", "1 * 2", true);
    check(&arith, "parens", "( 1 + 2 )", true);
    check(&arith, "trailing operator", "1 +", false);

    println!("cfg_parser_demo: 9 assertions passed");
}
