use parser_vm::Regex;

fn main() {
    println!("=== Arithmetic Expression Parser Demo ===\n");
    
    println!("Arithmetic grammar with operator precedence:");
    println!("  E -> E plus T | T");
    println!("  T -> T star F | F");
    println!("  F -> lparen E rparen | NUM");
    println!();
    
    let grammar = r#"E -> E plus T | T; T -> T star F | F; F -> lparen E rparen | NUM; NUM -> 1 | 2 | 3 | 4 | 5"#;
    
    let re = Regex::new(grammar).unwrap();
    
    let tests = vec![
        ("Number", "1", true),
        ("Addition", "1 plus 2", true),
        ("Multiplication", "2 star 3", true),
        ("Mixed", "1 plus 2 star 3", true),
        ("Parentheses", "lparen 1 plus 2 rparen", true),
        ("Complex", "1 plus 2 star 3 plus 4", true),
        ("Left assoc", "1 plus 2 plus 3", true),
        ("Right assoc", "1 star 2 star 3", true),
        ("Nested parens", "lparen lparen 1 plus 2 rparen star 3 rparen", true),
        ("Invalid", "1 plus", false),
        ("Invalid op", "1 unknown 2", false),
    ];
    
    println!("Test Results:");
    for (name, input, expected) in tests {
        let result = re.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} {}: '{}' -> {} (expected {})", status, name, input, result, expected);
        assert_eq!(result, expected, "Test failed: {}", name);
    }
    
    println!("\n✓ All arithmetic tests passed!");
    println!("Arithmetic parser demonstrates:");
    println!("- Operator precedence via grammar structure");
    println!("- Left recursion handled by SLR parser");
    println!("- Beyond regex capabilities");
}
