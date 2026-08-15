use parser_vm::Regex;

fn main() {
    println!("=== Regex API Compatibility Demo ===\n");
    
    // Simple regex mode
    println!("1. Simple regex mode (backwards compatible):");
    let re = Regex::new(r"test").unwrap();
    println!("   Pattern: test");
    println!("   is_match('test'): {}", re.is_match("test"));
    println!("   is_match('a test b'): {}", re.is_match("a test b"));
    println!("   is_match('abc'): {}", re.is_match("abc"));
    println!("   find('a test b'): {:?}", re.find("a test b"));
    println!();
    
    // CFG mode
    println!("2. CFG mode (extended):");
    let re = Regex::new(r"E -> E '+' NUM | NUM; NUM -> digit").unwrap();
    println!("   Pattern: E -> E '+' NUM | NUM; NUM -> digit");
    println!("   is_match('digit'): {}", re.is_match("digit"));
    println!("   is_match('digit + digit'): {}", re.is_match("digit + digit"));
    println!("   is_match('digit +'): {}", re.is_match("digit +"));
    println!();
    
    // Captures
    println!("3. Captures:");
    let re = Regex::new(r"hello").unwrap();
    if let Some(caps) = re.captures("hello world") {
        println!("   captures('hello world'): {:?}", caps.matches);
    }
    println!();
    
    println!("=== API is upward compatible with regex crate ===");
}
