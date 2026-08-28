use parser_vm::CfgParser;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(result, expected, "[{}] input '{}': expected {}", name, input, expected);
}

fn main() {
    // A real JSON subset (RFC 8259): objects, arrays, strings (via the "any"
    // quoted-string token), integers incl. negative, decimals, booleans, null.
    let grammar = r#"Value -> Object | Array | Str | Number | "true" | "false" | "null"; Object -> "{" Members "}"; Members -> Pair "," Members | Pair; Pair -> Str ":" Value; Array -> "[" Elements "]"; Elements -> Value "," Elements | Value; Str -> "any"; Number -> Ds | "-" Ds | Ds "." Ds | "-" Ds "." Ds; Ds -> D | D Ds; D -> "[0-9]""#;
    let parser = CfgParser::new(grammar).unwrap();

    check(&parser, "integer", "42", true);
    check(&parser, "negative integer", "-7", true);
    check(&parser, "decimal", "3.14", true);
    check(&parser, "boolean", "true", true);
    check(&parser, "null", "null", true);
    check(&parser, "string with spaces", r#""hello world""#, true);
    check(&parser, "escaped quotes", r#""a \"quoted\" word""#, true);
    check(&parser, "empty string", r#""""#, true);
    check(&parser, "array", "[1, 2, 3]", true);
    check(&parser, "heterogeneous array", r#"[ "rust", 7, [true, null] ]"#, true);
    check(&parser, "object", r#"{"name": "shun", "age": 30}"#, true);
    check(&parser, "object with everything", r#"{"pi": 3.14, "tags": ["a", "b"], "ok": true, "none": null, "delta": -1.5}"#, true);
    check(&parser, "deep nesting", r#"{"a": [1, [2, [3]]], "b": {"c": "deep"}}"#, true);
    check(&parser, "not a json value", "( )", false);
    check(&parser, "value after object", r#"{"name": }"#, false);
    check(&parser, "missing comma", r#"{"a": 1 "b": 2}"#, false);
    check(&parser, "trailing comma", "[1,]", false);
    check(&parser, "missing colon", r#"{"a" 1}"#, false);
    check(&parser, "unclosed string", r#""unclosed"#, false);
    check(&parser, "truncated literal", "nul", false);

    println!("json_parser: 20 assertions passed");
}
