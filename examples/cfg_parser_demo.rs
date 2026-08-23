use parser_vm::CfgParser;

fn main() {
    println!("=== CFG Parser Demo ===\n");
    
    println!("Context-Free Grammar parser with real syntax");
    println!("Beyond regex capabilities!\n");
    
    // S-expression parser with real parentheses
    println!("1. S-Expression Grammar:");
    let parser = CfgParser::new("SExp -> ( SExpList ) | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar | 1").unwrap();
    
    let tests = vec![
        ("atom", "foo", true),
        ("list", "( foo )", true),
        ("nested", "( foo ( bar ) )", true),
        ("invalid", "baz", false),
    ];
    
    for (name, input, expected) in &tests {
        let result = parser.is_match(input);
        let status = if result == *expected { "✓" } else { "✗" };
        println!("  {} {}: '{}' -> {}", status, name, input, result);
    }
    
    println!("\n2. Arithmetic Grammar:");
    let parser = CfgParser::new("E -> E + T | T; T -> T * F | F; F -> ( E ) | NUM; NUM -> 1 | 2").unwrap();
    
    let tests = vec![
        ("number", "1", true),
        ("add", "1 + 2", true),
        ("mul", "1 * 2", true),
        ("parens", "( 1 + 2 )", true),
        ("invalid", "1 +", false),
    ];
    
    for (name, input, expected) in &tests {
        let result = parser.is_match(input);
        let status = if result == *expected { "✓" } else { "✗" };
        println!("  {} {}: '{}' -> {}", status, name, input, result);
    }
    
    println!("\nCFG parser handles recursive structures with real syntax!");
}
