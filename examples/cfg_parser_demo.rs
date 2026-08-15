use parser_vm::CfgParser;

fn main() {
    println!("=== CFG Parser Demo ===\n");
    
    println!("Context-Free Grammar parser for nested structures");
    println!("Beyond regex capabilities!\n");
    
    // S-expression parser
    println!("1. S-Expression Grammar:");
    let parser = CfgParser::new("SExp -> lparen SExpList rparen | Atom; SExpList -> SExp SExpList | SExp; Atom -> foo | bar | 1").unwrap();
    
    let tests = vec![
        ("atom", "foo", true),
        ("list", "lparen foo rparen", true),
        ("nested", "lparen foo lparen bar rparen rparen", true),
        ("invalid", "baz", false),
    ];
    
    for (name, input, expected) in tests {
        let result = parser.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} {}: '{}' -> {}", status, name, input, result);
    }
    
    println!("\n2. Arithmetic Grammar:");
    let parser = CfgParser::new("E -> E plus T | T; T -> T star F | F; F -> lparen E rparen | NUM; NUM -> 1 | 2").unwrap();
    
    let tests = vec![
        ("number", "1", true),
        ("add", "1 plus 2", true),
        ("mul", "1 star 2", true),
        ("parens", "lparen 1 plus 2 rparen", true),
        ("invalid", "1 plus", false),
    ];
    
    for (name, input, expected) in tests {
        let result = parser.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} {}: '{}' -> {}", status, name, input, result);
    }
    
    println!("\nCFG parser handles recursive structures regex cannot!");
}
