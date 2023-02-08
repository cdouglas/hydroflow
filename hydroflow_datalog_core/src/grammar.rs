#[rust_sitter::grammar("datalog")]
#[allow(dead_code)]
pub mod datalog {
    #[rust_sitter::language]
    #[derive(Debug)]
    pub struct Program {
        pub rules: Vec<Declaration>,
    }

    #[derive(Debug, Clone)]
    pub enum Declaration {
        Input(#[rust_sitter::leaf(text = ".input")] (), Ident),
        Output(#[rust_sitter::leaf(text = ".output")] (), Ident),
        Rule(Rule),
    }

    #[derive(Debug, Clone)]
    pub struct Rule {
        pub target: RelationExpr,

        pub rule_type: RuleType,

        #[rust_sitter::repeat(non_empty = true)]
        #[rust_sitter::delimited(
            #[rust_sitter::leaf(text = ",")]
            ()
        )]
        pub sources: Vec<Atom>,

        #[rust_sitter::leaf(text = ".")]
        _dot: Option<()>,
    }

    #[derive(Debug, Clone)]
    pub enum RuleType {
        Sync(#[rust_sitter::leaf(text = ":-")] ()),
        NextTick(#[rust_sitter::leaf(text = ":+")] ()),
        Async(#[rust_sitter::leaf(text = ":~")] ()),
    }

    #[derive(Debug, Clone)]
    pub enum Atom {
        Relation(RelationExpr),
        Predicate(PredicateExpr),
    }

    #[derive(Debug, Clone)]
    pub struct RelationExpr {
        pub name: Ident,

        #[rust_sitter::leaf(text = "(")]
        _l_paren: (),

        #[rust_sitter::delimited(
            #[rust_sitter::leaf(text = ",")]
            ()
        )]
        pub fields: Vec<Ident>,

        #[rust_sitter::leaf(text = ")")]
        _r_paren: (),
    }

    #[derive(Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Debug)]
    pub struct Ident {
        #[rust_sitter::leaf(pattern = r"[a-zA-Z_][a-zA-Z0-9_]*", transform = |s| s.to_string())]
        pub name: String,
    }

    #[rust_sitter::extra]
    struct Whitespace {
        #[rust_sitter::leaf(pattern = r"\s")]
        _whitespace: (),
    }

    #[derive(Debug, Clone)]
    pub enum BoolOp {
        Lt(#[rust_sitter::leaf(text = "<")] ()),
        LtEq(#[rust_sitter::leaf(text = "<=")] ()),
        Gt(#[rust_sitter::leaf(text = ">")] ()),
        GtEq(#[rust_sitter::leaf(text = ">=")] ()),
        Eq(#[rust_sitter::leaf(text = "==")] ()),
    }

    #[derive(Debug, Clone)]
    pub struct PredicateExpr {
        #[rust_sitter::leaf(text = "(")]
        _l_brace: (),

        pub left: Ident,
        pub op: BoolOp,
        pub right: Ident,

        #[rust_sitter::leaf(text = ")")]
        _r_brace: (),
    }
}