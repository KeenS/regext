use parser_vm::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MySymbol {
    S, E, T, F,
    LPar, RPar, Plus, Mult, Num,
    S_Prime, E_Prime, T_Prime, F_Prime,
    EOF,
}

impl From<Symbol> for MySymbol {
    fn from(s: Symbol) -> Self {
        match s {
            Symbol::Terminal(t) => match t.as_str() {
                "(" => MySymbol::LPar,
                ")" => MySymbol::RPar,
                "+" => MySymbol::Plus,
                "*" => MySymbol::Mult,
                "n" => MySymbol::Num,
                "EOF" => MySymbol::EOF,
                _ => panic!("Unknown terminal: {}", t),
            },
            Symbol::NonTerminal(n) => match n.as_str() {
                "S" => MySymbol::S,
                "E" => MySymbol::E,
                "T" => MySymbol::T,
                "F" => MySymbol::F,
                "S_prime" => MySymbol::S_Prime,
                "E_prime" => MySymbol::E_Prime,
                "T_prime" => MySymbol::T_Prime,
                "F_prime" => MySymbol::F_Prime,
                _ => panic!("Unknown non-terminal: {}", n),
            },
        }
    }
}

fn test_parser(name: &str, reg: &GrammarRegistry, input: Vec<MySymbol>) {
    let table = LRCompiler::compile::<MySymbol>(reg);
    let mut engine = Engine::new(0);
    println!("Testing {}: input {:?}", name, input);
    match engine.run(&table, input) {
        Ok(res) => println!("Result: Success! -> {:?}", res),
        Err(e) => println!("Result: Error -> {}", e),
    }
    println!();
}

fn main() {
    // 1. Balanced Parentheses: S -> ( S ) S | epsilon
    let mut reg_paren = GrammarRegistry::new();
    expr("( S ) S").named("S", &mut reg_paren);
    expr("epsilon").named("S", &mut reg_paren);
    
    let paren_input = vec![
        MySymbol::LPar, MySymbol::LPar, MySymbol::RPar, MySymbol::LPar, MySymbol::RPar, 
        MySymbol::LPar, MySymbol::LPar, MySymbol::LPar, MySymbol::RPar, MySymbol::RPar, MySymbol::RPar, MySymbol::RPar
    ];
    test_parser("Parentheses", &reg_paren, paren_input);

    // 2. Arithmetic: E -> E + T | T; T -> T * F | F; F -> ( E ) | n
    let mut reg_math = GrammarRegistry::new();
    expr("E + T | T").named("E", &mut reg_math);
    expr("T * F | F").named("T", &mut reg_math);
    expr("( E ) | n").named("F", &mut reg_math);
    
    let math_input = vec![
        MySymbol::Num, MySymbol::Plus, MySymbol::Num, MySymbol::Mult, 
        MySymbol::LPar, MySymbol::Num, MySymbol::RPar
    ];
    test_parser("Arithmetic", &reg_math, math_input);
}
