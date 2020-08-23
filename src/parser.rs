use crate::lexer::{TokenKind, Token};
use crate::lexer::ErrorKind::Unexpected;

pub enum Value<'a> {
    Int(i32),
    String(&'a str),
    Bool(bool),
    Variable(&'a str),
}

pub enum Type {
    NonNull(Box<Type>),
    Int,
    Float,
    Bool,
    String,
    Input(String),
    Array(Box<Type>)
}

pub struct Argument<'a> {
    pub name: &'a str,
    pub value: Value<'a>
}

pub struct PlainField<'a> {
    pub name: &'a str,
    pub args: Vec<Argument<'a>>,
    pub fields: Vec<Field<'a>>,
}

pub enum Field<'a> {
    PlainField(PlainField<'a>),
    InlineFragment(InlineFragment<'a>),
    Fragment(&'a str),
}

pub struct ArgumentDef<'a> {
    pub name: &'a str,
    pub kind: Type
}

pub struct Query<'a> {
    pub name: &'a str,
    pub args: Vec<ArgumentDef<'a>>,
    pub fields: Vec<Field<'a>>,
}

pub struct Mutation<'a> {
    pub name: &'a str,
    pub args: Vec<ArgumentDef<'a>>,
    pub fields: Vec<Field<'a>>,
}

pub struct Fragment<'a> {
    pub name: &'a str,
    pub args: Vec<ArgumentDef<'a>>,
    pub on: Type,
    pub fields: Vec<Field<'a>>,
}

pub struct InlineFragment<'a> {
    pub on: Type,
    pub fields: Vec<Field<'a>>,
}

/*
enum ASTKind<'a>{
    Value(Value<'a>),
    Argument(Argument<'a>),
    Type(Type<'a>),
    Field(Field<'a>),
    Query(Query<'a>),
    Mutation(Mutation<'a>),
    Fragment(Fragment<'a>),
}*/

pub struct GraphQL<'a> {
    pub fragments: Vec<Fragment<'a>>,
    pub queries: Vec<Query<'a>>,
    pub mutations: Vec<Mutation<'a>>,
}

pub enum ErrorKind {
    SyntaxError,
    Expecting(&'static str)
}

//todo remove duplication from lexer
pub struct Error {
    column: u32,
    line: u32,
    kind: ErrorKind
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error line {}, column {}", self.line, self.column);
        match self.kind {
            ErrorKind::Expecting(str) => write!(f, " : Expecting {}", str),
            ErrorKind::SyntaxError => write!(f, " : Syntax error")
        }
    }
}

struct Parser<'a> {
    module: GraphQL<'a>,
    tokens: Vec<Token<'a>>,
    i: usize
}

impl<'a> Parser<'a> {
    fn next(&mut self) -> &Token<'a> {
        let i = self.i;
        self.i += 1;
        &self.tokens[i]
    }

    fn current(&self) -> &Token<'a> {
        &self.tokens[self.i]
    }

    fn error(&self, kind: ErrorKind) -> Error {
        let i = std::cmp::min(self.i, self.tokens.len() - 1);

        Error{
            kind,
            column: self.tokens[i].column,
            line: self.tokens[i].line,
        }
    }

    fn expect(&mut self, kind: TokenKind, expecting: &'static str) -> Result<(), Error> {
        if self.next().kind != kind {
            Err(self.error(ErrorKind::Expecting(expecting)))
        } else {
            Ok(())
        }
    }

    fn parse_name(&mut self) -> Result<&'a str, Error> {
        match self.next().kind {
            TokenKind::Identifier(str) => Ok(str),
            _ => Err(self.error(ErrorKind::Expecting("identifier"))),
        }
    }

    fn parse_type(&mut self) -> Result<Type, Error> {
        let result = match self.next().kind {
            TokenKind::Identifier(name) => Ok(Type::Input(name.to_string())),
            TokenKind::StringKeyword => Ok(Type::String),
            TokenKind::IntKeyword => Ok(Type::Int),
            TokenKind::BoolKeyword => Ok(Type::Bool),
            TokenKind::OpenSquare => {
                let elem = self.parse_type()?;
                self.expect(TokenKind::CloseSquare, "]")?;
                Ok(Type::Array(Box::new(elem)))
            }
            _ => Err(self.error(ErrorKind::Expecting("type")))
        }?;

        if self.current().kind == TokenKind::Exclamation {
            self.next();
            Ok(Type::NonNull(Box::new(result)))
        } else {
            Ok(result)
        }
    }

    fn parse_spread(&mut self) -> Result<Field<'a>, Error> {
        match self.next().kind {
            TokenKind::Identifier(name) => Ok(Field::Fragment(name)),
            TokenKind::OnKeyword => {
                let on = self.parse_type()?;
                let fields = self.parse_fields()?;

                Ok(Field::InlineFragment(InlineFragment{on, fields}))
            },
            _ => Err(self.error(ErrorKind::Expecting("inline fragment or fragment")))
        }
    }

    fn parse_optional_fields(&mut self) -> Result<Vec<Field<'a>>, Error> {
        match self.current().kind {
            TokenKind::Identifier(_) | TokenKind::CloseBracket | TokenKind::Spread => Ok(vec![]),
            TokenKind::OpenBracket => self.parse_fields(),
            _ => Err(self.error(ErrorKind::Expecting("{ or \n")))
        }
    }

    fn parse_fields(&mut self) -> Result<Vec<Field<'a>>, Error> {
        let mut fields : Vec<Field> = vec![];

        self.expect(TokenKind::OpenBracket, "{");

        while self.current().kind != TokenKind::CloseBracket {
            fields.push( self.parse_field()?);
        }
        self.next();

        Ok(fields)
    }

    fn parse_field(&mut self) -> Result<Field<'a>, Error> {
        match self.next().kind {
            TokenKind::Identifier(name) => Ok(Field::PlainField(self.parse_plain_field(name)?)),
            TokenKind::Spread => self.parse_spread(),
            _ => Err(self.error(ErrorKind::Expecting("field or spread")))
        }
    }

    fn parse_value(&mut self) -> Result<Value<'a>, Error> {
        match self.next().kind {
            TokenKind::Variable(name) => Ok(Value::Variable(name)),
            TokenKind::Int(value) => Ok(Value::Int(value)),
            TokenKind::String(value) => Ok(Value::String(value)),
            _ => Err(self.error(ErrorKind::Expecting("Value"))),
        }
    }

    //split into two
    fn parse_named_list<F: Fn(&mut Parser<'a>,  &'a str) -> Result<Argument, Error>, Argument>(&mut self, variable: bool, parse: F) -> Result<Vec<Argument>, Error> {
        match self.current().kind {
            TokenKind::OpenBracket => Ok(vec![]),
            TokenKind::Identifier(_) | TokenKind::Spread | TokenKind::CloseBracket if !variable => Ok(vec![]),
            TokenKind::OpenParen => {
                self.next();

                let mut args = vec![];

                loop { match self.next().kind {
                    TokenKind::Variable(name) if variable => {
                        self.expect(TokenKind::Colon, ":")?;
                        args.push(parse(self, name)?);
                    },
                    TokenKind::Identifier(name) if !variable => {
                        self.expect(TokenKind::Colon, ":")?;
                        args.push(parse(self, name)?);
                    },
                    TokenKind::CloseParen => break,
                    _ => return Err(self.error(ErrorKind::Expecting("identifier")))
                } }

                Ok(args)
            },
            _ => return Err(self.error(ErrorKind::Expecting("{ or (")))
        }
    }

    fn parse_plain_field(&mut self, name: &'a str) -> Result<PlainField<'a>, Error> {
        let args = self.parse_arguments()?;
        let fields = self.parse_optional_fields()?;

        Ok(PlainField{ name, args, fields })
    }

    fn parse_arguments(&mut self) -> Result<Vec<Argument<'a>>, Error> {
        self.parse_named_list(false, |parser, name|
            Ok(Argument{ name, value: parser.parse_value()? })
        )
    }

    fn parse_arguments_def(&mut self) -> Result<Vec<ArgumentDef<'a>>, Error> {
        self.parse_named_list(true, |parser, name|
            Ok(ArgumentDef{ name, kind: parser.parse_type()? })
        )
    }

    fn parse_query(&mut self) -> Result<(), Error> {
        let name = self.parse_name()?;
        let args = self.parse_arguments_def()?;
        let fields = self.parse_fields()?;

        Ok(self.module.queries.push(Query{ name, args, fields }))
    }

    fn parse_mutation(&mut self) -> Result<(), Error> {
        let name = self.parse_name()?;
        let args = self.parse_arguments_def()?;
        let fields = self.parse_optional_fields()?;

        Ok(self.module.mutations.push(Mutation{ name, args, fields }))
    }

    fn parse_fragment(&mut self) -> Result<(), Error> {
        let name = self.parse_name()?;
        self.expect(TokenKind::OnKeyword, "Expecting on $type");
        let on = self.parse_type()?;

        let args = self.parse_arguments_def()?;
        let fields = self.parse_fields()?;

        Ok(self.module.fragments.push(Fragment{name, on, args, fields}))
    }

    fn parse_toplevel(&mut self) -> Result<(), Error> {
        match self.next().kind {
            TokenKind::MutationKeyword => self.parse_mutation(),
            TokenKind::QueryKeyword => self.parse_query(),
            TokenKind::FragmentKeyword => self.parse_fragment(),
            _ => return Err(self.error(ErrorKind::Expecting("Top level consists only of query,mutation or fragment")))
        }
    }
}

pub fn parse<'a>(tokens: Vec<Token<'a>>) -> Result<GraphQL<'a>, Error> {
    let mut parser = Parser{
        module: GraphQL{
            fragments: vec![],
            queries: vec![],
            mutations: vec![],
        },
        tokens,
        i: 0
    };


    while parser.i < parser.tokens.len() {
        parser.parse_toplevel()?;
    }

    Ok(parser.module)
}