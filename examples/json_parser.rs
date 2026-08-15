use parser_vm::Regex;

fn main() {
    println!("=== JSON-like Parser Demo ===\n");
    
    println!("Simplified JSON grammar with regex-compatible API");
    println!("Demonstrates context-free parsing beyond regex capabilities\n");
    
    let grammar = r#"Value -> Object | Array | STRING | NUMBER | true | false | null; Object -> lbrace Members rbrace; Members -> Pair Members | Pair; Pair -> STRING colon Value; Array -> lbrack Elements rbrack; Elements -> Value Elements | Value; STRING -> hello | world; NUMBER -> 123 | 456"#;
    
    let re = Regex::new(grammar).unwrap();
    
    let tests = vec![
        ("String", "hello", true),
        ("Number", "123", true),
        ("Boolean", "true", true),
        ("Null", "null", true),
        ("Object with pair", "lbrace hello colon 123 rbrace", true),
        ("Array with values", "lbrack hello 123 rbrack", true),
        ("Nested array", "lbrack lbrack hello rbrack rbrack", true),
        ("Complex", "lbrace hello colon lbrack 123 456 rbrack rbrace", true),
        ("Invalid", "hello world", false),
        ("Invalid structure", "lbrace hello rbrace", false),
    ];
    
    println!("Test Results:");
    let mut passed = 0;
    for (name, input, expected) in tests {
        let result = re.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        if result == expected { passed += 1; }
        println!("  {} {}: '{}' -> {} (expected {})", status, name, input, result, expected);
    }
    
    println!("\nResults: {}/{} tests passed", passed, 10);
    println!("JSON parser demonstrates recursive structures with regex-compatible API");
}
