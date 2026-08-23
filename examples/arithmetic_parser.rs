use parser_vm::CfgParser;

fn main() {
    println!("=== Arithmetic Expression Parser Demo ===\n");
    
    println!("Arithmetic grammar with actual operators");
    println!("  E -> E + T | T");
    println!("  T -> T * F | F");
    println!("  F -> ( E ) | NUM");
    println!("  NUM -> 1 | 2 | 3 | 4 | 5\n");
    
    let parser = CfgParser::new("E -> E + T | T; T -> T * F | F; F -> ( E ) | NUM; NUM -> 1 | 2 | 3 | 4 | 5").unwrap();
    
    let tests = vec![
        ("Number", "1", true),
        ("Addition", "1 + 2", true),
        ("Multiplication", "2 * 3", true),
        ("Mixed", "1 + 2 * 3", true),
        ("Parentheses", "( 1 + 2 )", true),
        ("Complex", "1 + 2 * 3 + 4", true),
        ("Nested parens", "( ( 1 + 2 ) * 3 )", true),
        ("Invalid", "1 +", false),
        ("Invalid op", "1 ? 2", false),
    ];
    
    println!("Test Results:");
    let mut passed = 0;
    for (name, input, expected) in &tests {
        let result = parser.is_match(input);
        let status = if result == *expected { "✓" } else { "✗" };
        if result == *expected { passed += 1; }
        println!("  {} {}: '{}' -> {} (expected {})", status, name, input, result, expected);
    }
    
    println!("\nResults: {}/{} tests passed", passed, tests.len());
    println!("\nArithmetic parser with real operators and parentheses!");
}
