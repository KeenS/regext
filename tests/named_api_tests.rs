use parser_vm::{GrammarRegistry, LRCompiler, Engine, expr};

#[test]
fn test_named_api_basic() {
    let mut registry = GrammarRegistry::new();
    
    let _s = expr("a | b").named("S", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    
    let result = engine.run(&table, vec!["a".to_string()]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "S");
}

#[test]
fn test_named_api_sequence() {
    let mut registry = GrammarRegistry::new();
    
    let _e = expr("a b | c").named("E", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    
    let result = engine.run(&table, vec!["a".to_string(), "b".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn test_named_api_nested() {
    let mut registry = GrammarRegistry::new();
    
    // Define S first as start symbol
    let _s = expr("A B").named("S", &mut registry);
    let _a = expr("a | b").named("A", &mut registry);
    let _b = expr("c | d").named("B", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    
    let result = engine.run(&table, vec!["a".to_string(), "c".to_string()]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "S");
}

#[test]
fn test_named_api_arithmetic() {
    let mut registry = GrammarRegistry::new();
    
    // Define in reverse order to make E start symbol
    let _e = expr("E plus T | T").named("E", &mut registry);
    let _t = expr("T star F | F").named("T", &mut registry);
    let _f = expr("lparen E rparen | num").named("F", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    
    let result = engine.run(&table, vec!["num".to_string()]);
    assert!(result.is_ok());
}
