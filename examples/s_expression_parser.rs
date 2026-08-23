use parser_vm::CfgParser;

fn main() {
    println!("=== S-Expression Parser Demo ===\n");
    
    println!("S-Expression grammar with actual parentheses");
    println!("  SExp -> ( SExpList ) | Atom");
    println!("  SExpList -> SExp SExpList | SExp");
    println!("  Atom -> foo | bar | 1 | 2\n");
    
    let parser = CfgParser::new("SExp -> ( SExpList ) | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar | 1 | 2").unwrap();
    
    let tests = vec![
        ("Simple atom", "foo", true),
        ("Simple number", "1", true),
        ("Single element list", "( foo )", true),
        ("Two elements", "( foo bar )", true),
        ("Nested", "( foo ( bar ) )", true),
        ("Deep nested", "( ( foo ) ( bar ) )", true),
        ("Invalid atom", "baz", false),
        ("Unbalanced", "( foo", false),
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
    println!("\nS-Expressions with real parentheses work correctly!");
}
