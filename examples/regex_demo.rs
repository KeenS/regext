use parser_vm::Regex;

fn main() {
    println!("=== Regex API Demo ===\n");
    
    println!("Regex crate compatible API:");
    println!("Pattern: 'test'");
    println!();
    
    let re = Regex::new("test").unwrap();
    
    let tests = vec![
        ("exact match", "test", true),
        ("contains", "this is a test", true),
        ("no match", "abc", false),
        ("partial", "testing", true),
    ];
    
    for (name, input, expected) in tests {
        let result = re.is_match(input);
        let status = if result == expected { "✓" } else { "✗" };
        println!("  {} {}: '{}' -> {}", status, name, input, result);
    }
    
    println!("\nRegex API is upward compatible with regex crate:");
    println!("  - Regex::new()");
    println!("  - is_match()");
    println!("  - find()");
    println!("  - captures()");
}
