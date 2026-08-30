use parser_vm::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(result, expected, "[{}] input '{}': expected {}", name, input, expected);
}

fn main() {
    // A genuine S-expression (Lisp) parser on parser_vm's SLR(1) engine.
    //
    //   SExp   -> Quoted | List | Atom | Str
    //   List   -> "(" ")"          | "(" SExps ")"
    //   SExps  -> SExp SExps       | SExp
    //   Quoted -> "'" SExp         | "`" SExp | "," SExp | "," "@" SExp
    //   Atom   -> one maximal-munch run of symbol glyphs
    //   Str    -> one "quoted" string token (the lexer's quoted-string rule)
    //
    // Like a real Lisp tokenizer, an atom is the maximal run of
    // non-delimiter characters: whitespace, (, ) and the quote operators
    // ' ` , @ delimit, everything else is atom material. So -42, +17, 3.14,
    // .5, 6.02e23, -2.5e-3, x1, sin-1, ->, <=, a/b, foo.bar, even?, #f are
    // each a single atom — numbers are just atoms, as in Lisp.
    // Quote, backquote and splicing are first-class: 'a, `(a b), ,b, ,@x.
    let grammar = r#"SExp -> Quoted | List | Atom | Str; List -> "(" ")" | "(" SExps ")" ; SExps -> SExp SExps | SExp; Quoted -> "'" SExp | "`" SExp | "," SExp | "," "@" SExp; Atom -> "[-!$%*+/=?<>^_.:#~a-z0-9A-Z]+"; Str -> "any""#;
    let parser = CfgParser::new(grammar).unwrap();

    // -- atoms and numbers ----------------------------------------------------
    check(&parser, "integer", "42", true);
    check(&parser, "negative number", "-42", true);
    check(&parser, "positive number", "+17", true);
    check(&parser, "decimal", "3.14", true);
    check(&parser, "scheme-style decimal", ".5", true);
    check(&parser, "exponent", "6.02e23", true);
    check(&parser, "negative exponent", "-2.5e-3", true);
    check(&parser, "alphanumeric symbol", "x1", true);
    check(&parser, "alphanumeric symbol 2", "a1b2c3", true);
    check(&parser, "operator symbol", "->", true);
    check(&parser, "comparison symbol", "<=", true);
    check(&parser, "division form", "a/b", true);
    check(&parser, "dotted symbol", "foo.bar", true);
    check(&parser, "predicate symbol", "even?", true);
    check(&parser, "racket false", "#f", true);
    check(&parser, "operator as atom", "+", true);
    check(&parser, "lone dash atom", "-", true);

    // -- lists -----------------------------------------------------------------
    check(&parser, "empty list", "( )", true);
    check(&parser, "nested empty lists", "( ( ) ( ) )", true);
    check(&parser, "simple list", "( + 1 2 )", true);
    check(&parser, "list of numbers", "( -42 3.14 6.02e23 )", true);
    check(&parser, "list of lists", "( ( 1 2 ) ( 3 4 ) )", true);
    check(&parser, "function call", "( fib (- n 1) )", true);
    check(&parser, "if expression", "( if (= n 0) 1 (* n (fib (- n 1))) )", true);
    check(&parser, "define", "( define ( power b e ) ( if (= e 0 ) 1 ( * b ( power b ( - e 1 ) ) ) ) )", true);

    // -- quote forms -------------------------------------------------------------
    check(&parser, "quote atom", "'a", true);
    check(&parser, "quote list", "'(1 2 3)", true);
    check(&parser, "double quote", "''a", true);
    check(&parser, "quote in list", "( 'a b )", true);
    check(&parser, "backquote", "`(a b)", true);
    check(&parser, "comma splice", ",@x", true);
    check(&parser, "comma splice list", ",@(1 2)", true);
    check(&parser, "bare comma form", "( a , b )", true);
    check(&parser, "quasiquote with splice", "`(a ,b ,@(c d))", true);
    check(&parser, "quoted in nested list", "( 'a ( 'b ) )", true);

    // -- strings -----------------------------------------------------------------
    check(&parser, "string", "\"hello\"", true);
    check(&parser, "string with parens", "\"( not a list )\"", true);
    check(&parser, "string with escapes", "\"a \\\"quoted\\\" word\"", true);
    check(&parser, "list with string", "( a \"b\" 3 )", true);
    check(&parser, "quoted string", "'\"str\"", true);

    // -- invalid -------------------------------------------------------------------
    check(&parser, "empty input", "", false);
    check(&parser, "two top-level forms", "( ) ( )", false);
    check(&parser, "unbalanced open", "( 1", false);
    check(&parser, "unbalanced close", "( 1 ) )", false);
    check(&parser, "lone close", ")", false);
    check(&parser, "lone open", "(", false);
    check(&parser, "quote without form", "'", false);
    check(&parser, "backquote without form", "`", false);
    check(&parser, "comma without form", ",", false);
    check(&parser, "splicing operator without form", ",@", false);
    check(&parser, "unterminated string", "\"abc", false);
    check(&parser, "operator between atoms", "a + b", false);

    println!("s_expression_parser: 52 assertions passed");
}
