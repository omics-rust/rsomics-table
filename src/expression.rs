mod lexer;
mod parser;
mod value;

use std::cmp::Ordering;

use regex::Regex;
use rsomics_common::{Result, RsomicsError};
use unicode_width::UnicodeWidthStr;

use parser::{Ast, BinaryOp, Function, UnaryOp};
use value::{Literal, Value, equal};

use crate::expression::lexer::FieldRef;

pub(crate) struct Program {
    root: Expr,
    numeric_as_string: bool,
}

enum Expr {
    Literal(Literal),
    Field(usize),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    In(Box<Expr>, Vec<Literal>),
    Regex(Box<Expr>, Regex, bool),
    Function(Function, Box<Expr>),
}

impl Program {
    pub(crate) fn compile(
        source: &str,
        width: usize,
        header: Option<&[Vec<u8>]>,
        numeric_as_string: bool,
    ) -> Result<Self> {
        let root = compile(parser::parse(source)?, width, header)?;
        if !root.returns_bool() {
            return Err(invalid("filter expression cannot return a Boolean value"));
        }
        Ok(Self {
            root,
            numeric_as_string,
        })
    }

    pub(crate) fn evaluate(&self, fields: &[Vec<u8>], record: u64) -> Result<bool> {
        match self.root.evaluate(fields, self.numeric_as_string) {
            Ok(Value::Bool(value)) => Ok(value),
            Ok(_) => Err(invalid(format!(
                "record {record}: filter expression returned a non-Boolean value"
            ))),
            Err(message) => Err(invalid(format!("record {record}: {message}"))),
        }
    }
}

impl Expr {
    fn returns_bool(&self) -> bool {
        match self {
            Self::Literal(Literal::Bool(_)) => true,
            Self::Unary(UnaryOp::Not, _) | Self::In(_, _) | Self::Regex(_, _, _) => true,
            Self::Binary(operation, _, _) => matches!(
                operation,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or
            ),
            _ => false,
        }
    }

    fn evaluate<'a>(
        &'a self,
        fields: &'a [Vec<u8>],
        numeric_as_string: bool,
    ) -> std::result::Result<Value<'a>, String> {
        match self {
            Self::Literal(value) => Ok(value.value()),
            Self::Field(index) => value::field(&fields[*index], numeric_as_string)
                .map_err(|message| format!("field {} {message}", index + 1)),
            Self::Unary(operation, operand) => {
                let value = operand.evaluate(fields, numeric_as_string)?;
                match (operation, value) {
                    (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
                    (UnaryOp::Negate, Value::Number(value)) => checked_number(-value),
                    (UnaryOp::Not, _) => Err("! requires a Boolean operand".to_owned()),
                    (UnaryOp::Negate, _) => Err("unary - requires a numeric operand".to_owned()),
                }
            }
            Self::Binary(BinaryOp::And, left, right) => {
                let Value::Bool(left) = left.evaluate(fields, numeric_as_string)? else {
                    return Err("&& requires Boolean operands".to_owned());
                };
                if !left {
                    return Ok(Value::Bool(false));
                }
                let Value::Bool(right) = right.evaluate(fields, numeric_as_string)? else {
                    return Err("&& requires Boolean operands".to_owned());
                };
                Ok(Value::Bool(right))
            }
            Self::Binary(BinaryOp::Or, left, right) => {
                let Value::Bool(left) = left.evaluate(fields, numeric_as_string)? else {
                    return Err("|| requires Boolean operands".to_owned());
                };
                if left {
                    return Ok(Value::Bool(true));
                }
                let Value::Bool(right) = right.evaluate(fields, numeric_as_string)? else {
                    return Err("|| requires Boolean operands".to_owned());
                };
                Ok(Value::Bool(right))
            }
            Self::Binary(operation, left, right) => {
                let left = left.evaluate(fields, numeric_as_string)?;
                let right = right.evaluate(fields, numeric_as_string)?;
                evaluate_binary(*operation, left, right)
            }
            Self::In(value, choices) => {
                let value = value.evaluate(fields, numeric_as_string)?;
                Ok(Value::Bool(
                    choices.iter().any(|choice| equal(value, choice.value())),
                ))
            }
            Self::Regex(value, pattern, negated) => {
                let Value::Text(value) = value.evaluate(fields, numeric_as_string)? else {
                    return Err("regex matching requires a text operand".to_owned());
                };
                Ok(Value::Bool(pattern.is_match(value) != *negated))
            }
            Self::Function(function, argument) => {
                let Value::Text(value) = argument.evaluate(fields, numeric_as_string)? else {
                    return Err("len and ulen require a text operand".to_owned());
                };
                let length = match function {
                    Function::Len => value.len(),
                    Function::UnicodeLen => UnicodeWidthStr::width(value),
                };
                Ok(Value::Number(length as f64))
            }
        }
    }
}

fn compile(ast: Ast, width: usize, header: Option<&[Vec<u8>]>) -> Result<Expr> {
    Ok(match ast {
        Ast::Literal(value) => Expr::Literal(value),
        Ast::Field(field) => Expr::Field(resolve(field, width, header)?),
        Ast::Unary(operation, operand) => {
            Expr::Unary(operation, Box::new(compile(*operand, width, header)?))
        }
        Ast::Binary(operation @ (BinaryOp::Match | BinaryOp::NotMatch), value, pattern) => {
            let Ast::Literal(Literal::Text(pattern)) = *pattern else {
                return Err(invalid("regex pattern must be a string literal"));
            };
            let pattern = Regex::new(&pattern)
                .map_err(|error| invalid(format!("invalid regular expression: {error}")))?;
            Expr::Regex(
                Box::new(compile(*value, width, header)?),
                pattern,
                matches!(operation, BinaryOp::NotMatch),
            )
        }
        Ast::Binary(operation, left, right) => Expr::Binary(
            operation,
            Box::new(compile(*left, width, header)?),
            Box::new(compile(*right, width, header)?),
        ),
        Ast::In(value, choices) => Expr::In(Box::new(compile(*value, width, header)?), choices),
        Ast::Function(function, argument) => {
            Expr::Function(function, Box::new(compile(*argument, width, header)?))
        }
    })
}

fn resolve(field: FieldRef, width: usize, header: Option<&[Vec<u8>]>) -> Result<usize> {
    match field {
        FieldRef::Index(index) => {
            if index > width {
                return Err(invalid(format!(
                    "field {index} is out of range for {width} fields"
                )));
            }
            Ok(index - 1)
        }
        FieldRef::Name(name) => header
            .ok_or_else(|| invalid("field names require a header"))?
            .iter()
            .position(|field| field.as_slice() == name.as_bytes())
            .ok_or_else(|| invalid(format!("field {name:?} is not present"))),
    }
}

fn evaluate_binary<'a>(
    operation: BinaryOp,
    left: Value<'a>,
    right: Value<'a>,
) -> std::result::Result<Value<'a>, String> {
    match operation {
        BinaryOp::Add => numeric(left, right, "+", |left, right| left + right),
        BinaryOp::Subtract => numeric(left, right, "-", |left, right| left - right),
        BinaryOp::Multiply => numeric(left, right, "*", |left, right| left * right),
        BinaryOp::Divide => {
            if matches!(right, Value::Number(0.0)) {
                return Err("division by zero".to_owned());
            }
            numeric(left, right, "/", |left, right| left / right)
        }
        BinaryOp::Remainder => {
            if matches!(right, Value::Number(0.0)) {
                return Err("remainder by zero".to_owned());
            }
            numeric(left, right, "%", |left, right| left % right)
        }
        BinaryOp::Equal => Ok(Value::Bool(equal(left, right))),
        BinaryOp::NotEqual => Ok(Value::Bool(!equal(left, right))),
        BinaryOp::Less => Ok(Value::Bool(order(left, right).is_some_and(Ordering::is_lt))),
        BinaryOp::LessEqual => Ok(Value::Bool(order(left, right).is_some_and(Ordering::is_le))),
        BinaryOp::Greater => Ok(Value::Bool(order(left, right).is_some_and(Ordering::is_gt))),
        BinaryOp::GreaterEqual => Ok(Value::Bool(order(left, right).is_some_and(Ordering::is_ge))),
        BinaryOp::Match | BinaryOp::NotMatch | BinaryOp::And | BinaryOp::Or => {
            Err("invalid compiled expression operator".to_owned())
        }
    }
}

fn numeric<'a>(
    left: Value<'a>,
    right: Value<'a>,
    operation: &str,
    apply: impl FnOnce(f64, f64) -> f64,
) -> std::result::Result<Value<'a>, String> {
    let (Value::Number(left), Value::Number(right)) = (left, right) else {
        return Err(format!("{operation} requires numeric operands"));
    };
    checked_number(apply(left, right))
}

fn checked_number<'a>(value: f64) -> std::result::Result<Value<'a>, String> {
    if value.is_finite() {
        Ok(Value::Number(value))
    } else {
        Err("arithmetic result is not finite".to_owned())
    }
}

fn order(left: Value<'_>, right: Value<'_>) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.partial_cmp(&right),
        (Value::Text(left), Value::Text(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

fn invalid(message: impl Into<String>) -> RsomicsError {
    RsomicsError::InvalidInput(message.into())
}
