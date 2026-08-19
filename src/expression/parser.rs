use rsomics_common::{Result, RsomicsError};

use super::lexer::{FieldRef, Token, TokenKind, lex};
use super::value::Literal;

#[derive(Clone, Copy, Debug)]
pub(super) enum UnaryOp {
    Not,
    Negate,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Match,
    NotMatch,
    And,
    Or,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Function {
    Len,
    UnicodeLen,
}

#[derive(Debug)]
pub(super) enum Ast {
    Literal(Literal),
    Field(FieldRef),
    Unary(UnaryOp, Box<Ast>),
    Binary(BinaryOp, Box<Ast>, Box<Ast>),
    In(Box<Ast>, Vec<Literal>),
    Function(Function, Box<Ast>),
}

pub(super) fn parse(source: &str) -> Result<Ast> {
    let tokens = lex(source)?;
    let mut parser = Parser {
        tokens,
        cursor: 0,
        depth: 0,
    };
    let expression = parser.expression(0)?;
    if !matches!(parser.peek().kind, TokenKind::End) {
        return Err(parser.error("unexpected token after expression"));
    }
    Ok(expression)
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    depth: usize,
}

impl Parser {
    fn expression(&mut self, minimum: u8) -> Result<Ast> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.error("expression nesting exceeds 128"));
        }
        let mut left = self.prefix()?;
        loop {
            if matches!(self.peek().kind, TokenKind::In) {
                let precedence = 3;
                if precedence < minimum {
                    break;
                }
                self.advance();
                left = Ast::In(Box::new(left), self.literal_list()?);
                continue;
            }
            let Some((precedence, operation)) = binary(&self.peek().kind) else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.advance();
            let right = self.expression(precedence + 1)?;
            left = Ast::Binary(operation, Box::new(left), Box::new(right));
        }
        self.depth -= 1;
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Ast> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Number(value) => Ok(Ast::Literal(Literal::Number(value))),
            TokenKind::Text(value) => Ok(Ast::Literal(Literal::Text(value))),
            TokenKind::True => Ok(Ast::Literal(Literal::Bool(true))),
            TokenKind::False => Ok(Ast::Literal(Literal::Bool(false))),
            TokenKind::Null => Ok(Ast::Literal(Literal::Null)),
            TokenKind::Field(field) => Ok(Ast::Field(field)),
            TokenKind::Bang => Ok(Ast::Unary(UnaryOp::Not, Box::new(self.expression(6)?))),
            TokenKind::Minus => Ok(Ast::Unary(UnaryOp::Negate, Box::new(self.expression(6)?))),
            TokenKind::LeftParen => {
                let expression = self.expression(0)?;
                self.expect_right_paren()?;
                Ok(expression)
            }
            TokenKind::Ident(name) => self.function(&name),
            _ => Err(RsomicsError::InvalidInput(format!(
                "expression byte {}: expected a value",
                token.offset
            ))),
        }
    }

    fn function(&mut self, name: &str) -> Result<Ast> {
        if !matches!(self.advance().kind, TokenKind::LeftParen) {
            return Err(self.error(format!("function {name} requires parentheses")));
        }
        let argument = self.expression(0)?;
        self.expect_right_paren()?;
        let function = match name {
            "len" => Function::Len,
            "ulen" => Function::UnicodeLen,
            _ => return Err(self.error(format!("unknown function {name}"))),
        };
        Ok(Ast::Function(function, Box::new(argument)))
    }

    fn literal_list(&mut self) -> Result<Vec<Literal>> {
        if !matches!(self.advance().kind, TokenKind::LeftParen) {
            return Err(self.error("in requires a parenthesized literal list"));
        }
        let mut values = Vec::new();
        loop {
            values.push(self.literal()?);
            match self.peek().kind {
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::RightParen => {
                    self.advance();
                    break;
                }
                _ => return Err(self.error("expected a comma or ) in literal list")),
            }
        }
        Ok(values)
    }

    fn literal(&mut self) -> Result<Literal> {
        let negative = matches!(self.peek().kind, TokenKind::Minus);
        if negative {
            self.advance();
        }
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Number(value) if negative => Ok(Literal::Number(-value)),
            TokenKind::Number(value) => Ok(Literal::Number(value)),
            TokenKind::Text(value) if !negative => Ok(Literal::Text(value)),
            TokenKind::True if !negative => Ok(Literal::Bool(true)),
            TokenKind::False if !negative => Ok(Literal::Bool(false)),
            TokenKind::Null if !negative => Ok(Literal::Null),
            _ => Err(RsomicsError::InvalidInput(format!(
                "expression byte {}: in accepts literals only",
                token.offset
            ))),
        }
    }

    fn expect_right_paren(&mut self) -> Result<()> {
        if matches!(self.advance().kind, TokenKind::RightParen) {
            Ok(())
        } else {
            Err(self.error("expected )"))
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.cursor];
        if !matches!(token.kind, TokenKind::End) {
            self.cursor += 1;
        }
        token
    }

    fn error(&self, message: impl Into<String>) -> RsomicsError {
        RsomicsError::InvalidInput(format!(
            "expression byte {}: {}",
            self.peek().offset,
            message.into()
        ))
    }
}

fn binary(kind: &TokenKind) -> Option<(u8, BinaryOp)> {
    Some(match kind {
        TokenKind::Or => (1, BinaryOp::Or),
        TokenKind::And => (2, BinaryOp::And),
        TokenKind::Equal => (3, BinaryOp::Equal),
        TokenKind::NotEqual => (3, BinaryOp::NotEqual),
        TokenKind::Less => (3, BinaryOp::Less),
        TokenKind::LessEqual => (3, BinaryOp::LessEqual),
        TokenKind::Greater => (3, BinaryOp::Greater),
        TokenKind::GreaterEqual => (3, BinaryOp::GreaterEqual),
        TokenKind::Match => (3, BinaryOp::Match),
        TokenKind::NotMatch => (3, BinaryOp::NotMatch),
        TokenKind::Plus => (4, BinaryOp::Add),
        TokenKind::Minus => (4, BinaryOp::Subtract),
        TokenKind::Star => (5, BinaryOp::Multiply),
        TokenKind::Slash => (5, BinaryOp::Divide),
        TokenKind::Percent => (5, BinaryOp::Remainder),
        _ => return None,
    })
}
