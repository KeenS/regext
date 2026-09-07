use RegExt::{Engine, GrammarRegistry, LRCompiler, LalrTable, Symbol, expr};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MySymbol {
    S,
    E,
    T,
    F,
    LPar,
    RPar,
    Plus,
    Mult,
    Num,
    SPrime,
    EPrime,
    TPrime,
    FPrime,
    Eof,
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
                "EOF" => MySymbol::Eof,
                _ => panic!("Unknown terminal: {}", t),
            },
            Symbol::NonTerminal(n) => match n.as_str() {
                "S" => MySymbol::S,
                "E" => MySymbol::E,
                "T" => MySymbol::T,
                "F" => MySymbol::F,
                "S_prime" => MySymbol::SPrime,
                "E_prime" => MySymbol::EPrime,
                "T_prime" => MySymbol::TPrime,
                "F_prime" => MySymbol::FPrime,
                _ => panic!("Unknown non-terminal: {}", n),
            },
        }
    }
}

fn run(table: &LalrTable<MySymbol>, input: Vec<MySymbol>) -> Result<MySymbol, String> {
    let mut engine = Engine::new(0);
    engine.run(table, input)
}

fn main() {
    // 1. Balanced parentheses: S -> ( S ) S | epsilon
    let mut reg_paren = GrammarRegistry::new();
    expr(r#""(" S ")" S"#).named("S", &mut reg_paren);
    expr("epsilon").named("S", &mut reg_paren);

    let table = LRCompiler::compile::<MySymbol>(&reg_paren);
    let paren_input = vec![
        MySymbol::LPar,
        MySymbol::LPar,
        MySymbol::RPar,
        MySymbol::LPar,
        MySymbol::RPar,
        MySymbol::LPar,
        MySymbol::LPar,
        MySymbol::LPar,
        MySymbol::RPar,
        MySymbol::RPar,
        MySymbol::RPar,
        MySymbol::RPar,
    ];
    assert_eq!(
        run(&table, paren_input).as_ref(),
        Ok(&MySymbol::S),
        "balanced parens must parse"
    );

    let table = LRCompiler::compile::<MySymbol>(&reg_paren);
    assert!(
        run(&table, vec![MySymbol::LPar, MySymbol::LPar]).is_err(),
        "unbalanced parens must be rejected"
    );

    // 2. Arithmetic: E -> E + T | T; T -> T * F | F; F -> ( E ) | n
    let mut reg_math = GrammarRegistry::new();
    expr(r#"E "+" T | T"#).named("E", &mut reg_math);
    expr(r#"T "*" F | F"#).named("T", &mut reg_math);
    expr(r#""(" E ")" | "n""#).named("F", &mut reg_math);

    let table = LRCompiler::compile::<MySymbol>(&reg_math);
    let math_input = vec![
        MySymbol::Num,
        MySymbol::Plus,
        MySymbol::Num,
        MySymbol::Mult,
        MySymbol::LPar,
        MySymbol::Num,
        MySymbol::RPar,
    ];
    assert_eq!(
        run(&table, math_input).as_ref(),
        Ok(&MySymbol::E),
        "'n + n * ( n )' must parse"
    );

    println!("demo: 3 assertions passed");
}
