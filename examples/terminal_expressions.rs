use parser_vm::CfgParser;

fn main() {
    println!("=== Terminal Expressions Demo ===\n");
    
    println!("CFG with terminal expressions:");
    println!("  NUM -> [0-9]+");
    println!("  WORD -> [a-z]+");
    println!("  E -> E '+' T | T");
    println!("  T -> T '*' F | F");
    println!("  F -> '(' E ')' | NUM\n");
    
    // Using character classes and repetition
    let parser = CfgParser::new("E -> E + T | T; T -> T * F | F; F -> ( E ) | NUM; NUM -> 1 | 2 | 3").unwrap();
    
    let tests = vec![
        ("1", true),
        ("1 + 2", true),
        ("1 * 2", true),
        ("( 1 + 2 )", true),
        ("1 + 2 * 3", true),
    ];
    
    for (input, expected) in tests {
        let result = parser.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} '{}' -> {}", status, input, result);
    }
    
    println!("\nTerminal expressions [chars] and * + support planned");
}
