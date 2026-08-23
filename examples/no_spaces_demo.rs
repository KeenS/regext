use parser_vm::CfgParser;

fn main() {
    println!("=== No Spaces Required Demo ===\n");
    
    // Grammar with terminals that don't require spaces
    let parser = CfgParser::new("S -> ( S ) | a").unwrap();
    
    println!("Grammar: S -> ( S ) | a");
    println!("Testing without spaces:\n");
    
    let tests = vec![
        ("a", true),
        ("(a)", true),
        ("((a))", true),
        ("(a)(a)", false), // Should fail - needs space or proper grammar
    ];
    
    for (input, expected) in tests {
        let result = parser.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} '{}' -> {} (expected {})", status, input, result, expected);
    }
    
    println!("\nLexer should handle input without spaces");
}
