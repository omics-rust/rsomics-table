#[derive(Clone, Debug)]
pub(super) enum Literal {
    Number(f64),
    Text(String),
    Bool(bool),
    Null,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Value<'a> {
    Number(f64),
    Text(&'a str),
    Bool(bool),
    Null,
}

impl Literal {
    pub(super) fn value(&self) -> Value<'_> {
        match self {
            Self::Number(value) => Value::Number(*value),
            Self::Text(value) => Value::Text(value),
            Self::Bool(value) => Value::Bool(*value),
            Self::Null => Value::Null,
        }
    }
}

pub(super) fn field(bytes: &[u8], numeric_as_string: bool) -> Result<Value<'_>, &'static str> {
    let text = std::str::from_utf8(bytes).map_err(|_| "is not valid UTF-8")?;
    if !numeric_as_string
        && let Ok(value) = text.parse::<f64>()
        && value.is_finite()
    {
        return Ok(Value::Number(value));
    }
    Ok(Value::Text(text))
}

pub(super) fn equal(left: Value<'_>, right: Value<'_>) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::Text(left), Value::Text(right)) => left == right,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}
