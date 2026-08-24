use parser_vm::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(result, expected, "[{}] input '{}': expected {}", name, input, expected);
}

fn main() {
    // SExp -> ( SExpList ) | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar | 1 | 2
    let parser = CfgParser::new("SExp -> ( SExpList ) | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar | 1 | 2").unwrap();

    check(&parser, "simple atom", "foo", true);
    check(&parser, "simple number", "1", true);
    check(&parser, "single element list", "( foo )", true);
    check(&parser, "two elements", "( foo bar )", true);
    check(&parser, "nested", "( foo ( bar ) )", true);
    check(&parser, "deep nested", "( ( foo ) ( bar ) )", true);
    check(&parser, "invalid atom", "baz", false);
    check(&parser, "unbalanced", "( foo", false);

    println!("s_expression_parser: 8 assertions passed");
}
