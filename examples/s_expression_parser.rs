use parser_vm::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(result, expected, "[{}] input '{}': expected {}", name, input, expected);
}

fn main() {
    // S-expressions: Word is a maximal-munch run of symbol glyphs, Num a digit run.
    let grammar = r#"SExp -> "(" SExps ")" | "(" ")" | Atom; SExps -> SExp SExps | SExp; Atom -> Word | Num; Word -> "[-a-zA-Z_*=+<>]+" ; Num -> "[0-9]+""#;
    let parser = CfgParser::new(grammar).unwrap();

    check(&parser, "number", "42", true);
    check(&parser, "symbol", "foo", true);
    check(&parser, "operator as atom", "+", true);
    check(&parser, "simple list", "( + 1 2 )", true);
    check(&parser, "empty list", "( )", true);
    check(&parser, "list of lists", "( ( ) ( ) )", true);
    check(&parser, "function call", "( fib (- n 1) )", true);
    check(&parser, "if expression", "( if (= n 0) 1 (* n (fib (- n 1))) )", true);
    check(&parser, "define", "( define ( power b e ) ( if (= e 0 ) 1 ( * b ( power b ( - e 1 ) ) ) ) )", true);
    check(&parser, "two top-level atoms", "foo bar", false);
    check(&parser, "trailing atom", "( + 1 ) 2", false);
    check(&parser, "unbalanced open", "( 1", false);
    check(&parser, "lone close", ")", false);

    println!("s_expression_parser: 13 assertions passed");
}
