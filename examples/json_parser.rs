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
    // A genuine JSON parser (RFC 8259 values) on RegExt's LALR(1) engine.
    //
    // Containers are written right-recursively (no left recursion, no
    // conflicts) so lists can be any length, including zero:
    //   Object -> "{" "}"          |  "{" Pair Tail
    //   Tail   -> "," Pair Tail    |  "}"
    //   Pair   -> Str ":" Value
    //   Array  -> "[" "]"          |  "[" Value VTail
    //   VTail  -> "," Value VTail  |  "]"
    //   Value  -> Object | Array | Str | Number | "true" | "false" | "null"
    //
    // Terminals:
    //   "any"         -> one whole JSON string: the lexer's quoted-string rule
    //                    (keeps whitespace and handles \" escapes inside)
    //   "[-0-9.eE+]+" -> one number token: the maximal-munch run of '-',
    //                    digits, '.', 'e', 'E', '+' — covers 42, -7, 3.14,
    //                    6.02e23, -2.5e-3 (a run is a [class] + marker)
    //   { } [ ] : ,   -> structural punctuation
    //   true/false/null -> keywords
    //
    // Known lexer-level limitations (the VM tokenizes, it does not
    // predicate on token content):
    //   - number-adjacent junk such as "1.2.3" or a lone "-" is over-accepted
    //     (one maximal-munch number token),
    //   - string *contents* are opaque: an invalid escape like "\q" is not
    //     rejected.
    // Everything structural — containers, commas, colons, quoted keys,
    // nesting, and keywords — is enforced exactly.
    let grammar = r#"Value -> Object | Array | Str | Number | "true" | "false" | "null"; Object -> "{" "}" | "{" Pair Tail; Tail -> "," Pair Tail | "}"; Pair -> Str ":" Value; Array -> "[" "]" | "[" Value VTail; VTail -> "," Value VTail | "]"; Str -> "any"; Number -> "[-0-9.eE+]+""#;
    let parser = CfgParser::new(grammar).unwrap();

    // -- scalar values -------------------------------------------------------
    check(&parser, "integer", "42", true);
    check(&parser, "negative integer", "-7", true);
    check(&parser, "zero", "0", true);
    check(&parser, "fraction", "3.14", true);
    check(&parser, "exponent", "6.02e23", true);
    check(&parser, "uppercase exponent", "2E-8", true);
    check(&parser, "negative fraction with exponent", "-2.5e-3", true);
    check(&parser, "true", "true", true);
    check(&parser, "false", "false", true);
    check(&parser, "null", "null", true);
    check(&parser, "empty string", r#""""#, true);
    check(&parser, "string with spaces", r#""hello world""#, true);
    check(&parser, "escaped quotes", r#""a \"quoted\" word""#, true);
    check(&parser, "string with escapes", r#""tab\tnew\nline""#, true);
    check(&parser, "structural chars in string", r#""][{}:,]""#, true);
    check(&parser, "keyword as string", r#""true""#, true);

    // -- arrays --------------------------------------------------------------
    check(&parser, "empty array", "[]", true);
    check(&parser, "array", "[1, 2, 3]", true);
    check(&parser, "single element array", "[true]", true);
    check(
        &parser,
        "heterogeneous array",
        r#"[ "rust", 7, [true, null] ]"#,
        true,
    );
    check(&parser, "nested arrays", "[[[]]]", true);
    check(
        &parser,
        "array of objects",
        r#"[ {"a": 1}, {"b": 2} ]"#,
        true,
    );

    // -- objects -------------------------------------------------------------
    check(&parser, "empty object", "{}", true);
    check(&parser, "object", r#"{"name": "shun", "age": 30}"#, true);
    check(&parser, "single pair", r#"{"only": null}"#, true);
    check(
        &parser,
        "object with everything",
        r#"{"pi": 3.14, "tags": ["a", "b"], "ok": true, "none": null, "delta": -1.5, "big": 6.02e23}"#,
        true,
    );
    check(
        &parser,
        "deep nesting",
        r#"{"a": [1, [2, [3]]], "b": {"c": "deep"}}"#,
        true,
    );
    check(
        &parser,
        "nested empty containers",
        r#"{"a": {"b": [{}, []]}}"#,
        true,
    );
    check(
        &parser,
        "keyword keys",
        r#"{"true": 1, "null": [false]}"#,
        true,
    );

    // -- a real document -----------------------------------------------------
    let document = r#"{
        "name": "RegExt",
        "version": 3,
        "features": ["lr", "slr", "json"],
        "limits": {"depth": null, "count": 42.5},
        "active": true
    }"#;
    check(&parser, "pretty-printed document", document, true);

    // -- invalid JSON --------------------------------------------------------
    check(&parser, "empty document", "", false);
    check(&parser, "whitespace only", "   \n\t ", false);
    check(&parser, "not a json value", "( )", false);
    check(&parser, "two top-level values", "1 2", false);
    check(&parser, "missing value", r#"{"name": }"#, false);
    check(&parser, "missing key", r#"{: 1}"#, false);
    check(&parser, "missing colon", r#"{"a" 1}"#, false);
    check(&parser, "missing comma", r#"{"a": 1 "b": 2}"#, false);
    check(&parser, "missing comma in array", "[1 2]", false);
    check(&parser, "trailing comma in array", "[1,]", false);
    check(&parser, "trailing comma in object", r#"{"a": 1,}"#, false);
    check(&parser, "leading comma in object", r#"{, "a": 1}"#, false);
    check(&parser, "double comma", r#"{"a": 1,, "b": 2}"#, false);
    check(&parser, "extra closing brace", r#"{"a": 1} }"#, false);
    check(&parser, "mismatched brackets", r#"{"a": [1}"#, false);
    check(&parser, "unclosed string", r#""unclosed"#, false);
    check(&parser, "unquoted key", r#"{name: 1}"#, false);
    check(&parser, "single quoted string", "''", false);
    check(&parser, "bare word", "hello", false);
    check(&parser, "truncated literal", "nul", false);
    check(&parser, "keyword with trailing junk", "truex", false);

    println!("json_parser: 51 assertions passed");
}
