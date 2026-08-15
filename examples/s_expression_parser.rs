use parser_vm::Regex;

fn main() {
    println!("=== S-Expression Parser Demo ===\n");
    
    println!("S-Expression grammar with regex-compatible API");
    println!("Uses CFG to parse nested structures beyond regex capabilities\n");
    
    let grammar = r#"SExp -> lparen SExpList rparen | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar | 1 | 2 | 3"#;
    
    let re = Regex::new(grammar).unwrap();
    
    let tests = vec![
        ("Simple atom", "foo", true),
        ("Simple number", "1", true),
        ("Empty list", "lparen rparen", false),
        ("Single element", "lparen foo rparen", true),
        ("Two elements", "lparen foo bar rparen", true),
        ("Nested", "lparen foo lparen bar rparen rparen", true),
        ("Deep nested", "lparen lparen foo rparen lparen bar rparen rparen", true),
        ("Invalid atom", "baz", false),
    ];
    
    println!("Test Results:");
    for (name, input, expected) in tests {
        let result = re.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} {}: '{}' -> {} (expected {})", status, name, input, result, expected);
        assert_eq!(result, expected, "Test failed: {}", name);
    }
    
    println!("\n✓ All S-expression tests passed!");
    println!("S-Expressions demonstrate context-free power:");
    println!("- Nested parentheses handled correctly");
    println!("- Recursive structures supported");
    println!("- Regex cannot handle this!");
}
