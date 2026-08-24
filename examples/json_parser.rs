use parser_vm::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(result, expected, "[{}] input '{}': expected {}", name, input, expected);
}

fn main() {
    // Simplified JSON-like grammar; CFG parsing handles the recursion regex cannot
    let grammar = r#"Value -> Object | Array | STRING | NUMBER | true | false | null; Object -> lbrace Members rbrace; Members -> Pair Members | Pair; Pair -> STRING colon Value; Array -> lbrack Elements rbrack; Elements -> Value Elements | Value; STRING -> hello | world; NUMBER -> 123 | 456"#;
    let parser = CfgParser::new(grammar).unwrap();

    check(&parser, "string", "hello", true);
    check(&parser, "number", "123", true);
    check(&parser, "boolean", "true", true);
    check(&parser, "null", "null", true);
    check(&parser, "object with pair", "lbrace hello colon 123 rbrace", true);
    check(&parser, "array with values", "lbrack hello 123 rbrack", true);
    check(&parser, "nested array", "lbrack lbrack hello rbrack rbrack", true);
    check(&parser, "complex", "lbrace hello colon lbrack 123 456 rbrack rbrace", true);
    check(&parser, "two top-level values", "hello world", false);
    check(&parser, "pair without value", "lbrace hello rbrace", false);

    println!("json_parser: 10 assertions passed");
}
