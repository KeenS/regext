use parser_vm::CfgParser;

fn main() {
    println!("=== Arithmetic Without Spaces ===\n");
    
    let parser = CfgParser::new("E -> E + T | T; T -> T * F | F; F -> ( E ) | num").unwrap();
    
    println!("Grammar: E -> E + T | T; T -> T * F | F; F -> ( E ) | num\n");
    
    let tests = vec![
        ("num", true),
        ("num+num", true),
        ("num*num", true),
        ("(num+num)", true),
        ("num+num*num", true),
        ("(num+num)*num", true),
    ];
    
    for (input, expected) in tests {
        let result = parser.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} '{}' -> {} (expected {})", status, input, result, expected);
    }
    
    println!("\nNo spaces needed between tokens!");
}
