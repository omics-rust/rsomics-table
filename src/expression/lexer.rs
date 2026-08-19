use rsomics_common::{Result, RsomicsError};

#[derive(Clone, Debug)]
pub(super) enum FieldRef {
    Index(usize),
    Name(String),
}

#[derive(Clone, Debug)]
pub(super) enum TokenKind {
    Number(f64),
    Text(String),
    Field(FieldRef),
    True,
    False,
    Null,
    Ident(String),
    In,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
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
    LeftParen,
    RightParen,
    Comma,
    End,
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) offset: usize,
}

pub(super) fn lex(source: &str) -> Result<Vec<Token>> {
    let mut lexer = Lexer {
        source,
        offset: 0,
        tokens: Vec::new(),
    };
    while let Some(character) = lexer.peek() {
        if character.is_whitespace() {
            lexer.bump();
            continue;
        }
        let offset = lexer.offset;
        let kind = match character {
            '$' => lexer.field()?,
            '0'..='9' => lexer.number()?,
            '.' if lexer.peek_next().is_some_and(|next| next.is_ascii_digit()) => lexer.number()?,
            '\'' | '"' => lexer.string()?,
            '+' => lexer.single(TokenKind::Plus),
            '-' => lexer.single(TokenKind::Minus),
            '*' => lexer.single(TokenKind::Star),
            '/' => lexer.single(TokenKind::Slash),
            '%' => lexer.single(TokenKind::Percent),
            '(' => lexer.single(TokenKind::LeftParen),
            ')' => lexer.single(TokenKind::RightParen),
            ',' => lexer.single(TokenKind::Comma),
            '!' => lexer.bang()?,
            '=' => lexer.equal()?,
            '<' => lexer.less(),
            '>' => lexer.greater(),
            '&' => lexer.double('&', TokenKind::And, "expected &&")?,
            '|' => lexer.double('|', TokenKind::Or, "expected ||")?,
            character if character.is_alphabetic() || character == '_' => lexer.word()?,
            _ => return Err(lexer.error(format!("unexpected character {character:?}"))),
        };
        lexer.tokens.push(Token { kind, offset });
        if lexer.tokens.len() > 4096 {
            return Err(lexer.error("expression has more than 4096 tokens"));
        }
    }
    lexer.tokens.push(Token {
        kind: TokenKind::End,
        offset: source.len(),
    });
    Ok(lexer.tokens)
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut characters = self.source[self.offset..].chars();
        characters.next()?;
        characters.next()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.offset += character.len_utf8();
        Some(character)
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.bump();
        kind
    }

    fn double(
        &mut self,
        expected: char,
        kind: TokenKind,
        message: &'static str,
    ) -> Result<TokenKind> {
        self.bump();
        if self.bump() == Some(expected) {
            Ok(kind)
        } else {
            Err(self.error(message))
        }
    }

    fn bang(&mut self) -> Result<TokenKind> {
        self.bump();
        match self.peek() {
            Some('=') => {
                self.bump();
                Ok(TokenKind::NotEqual)
            }
            Some('~') => {
                self.bump();
                Ok(TokenKind::NotMatch)
            }
            _ => Ok(TokenKind::Bang),
        }
    }

    fn equal(&mut self) -> Result<TokenKind> {
        self.bump();
        match self.bump() {
            Some('=') => Ok(TokenKind::Equal),
            Some('~') => Ok(TokenKind::Match),
            _ => Err(self.error("expected == or =~")),
        }
    }

    fn less(&mut self) -> TokenKind {
        self.bump();
        if self.peek() == Some('=') {
            self.bump();
            TokenKind::LessEqual
        } else {
            TokenKind::Less
        }
    }

    fn greater(&mut self) -> TokenKind {
        self.bump();
        if self.peek() == Some('=') {
            self.bump();
            TokenKind::GreaterEqual
        } else {
            TokenKind::Greater
        }
    }

    fn number(&mut self) -> Result<TokenKind> {
        let start = self.offset;
        self.take_ascii_digits();
        if self.peek() == Some('.') {
            self.bump();
            self.take_ascii_digits();
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            let exponent = self.offset;
            self.take_ascii_digits();
            if exponent == self.offset {
                return Err(self.error("number exponent has no digits"));
            }
        }
        let spelling = &self.source[start..self.offset];
        let value = spelling
            .parse::<f64>()
            .map_err(|_| self.error(format!("invalid number {spelling:?}")))?;
        if !value.is_finite() {
            return Err(self.error(format!("number is not finite: {spelling}")));
        }
        Ok(TokenKind::Number(value))
    }

    fn take_ascii_digits(&mut self) {
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_digit())
        {
            self.bump();
        }
    }

    fn string(&mut self) -> Result<TokenKind> {
        let quote = self.bump().unwrap_or_default();
        let mut value = String::new();
        loop {
            match self.bump() {
                Some(character) if character == quote => return Ok(TokenKind::Text(value)),
                Some('\\') => match self.bump() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('0') => value.push('\0'),
                    Some('\\') => value.push('\\'),
                    Some(character) if character == quote => value.push(character),
                    Some(character) => {
                        value.push('\\');
                        value.push(character);
                    }
                    None => return Err(self.error("string ends with an escape")),
                },
                Some(character) => value.push(character),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn field(&mut self) -> Result<TokenKind> {
        self.bump();
        if self.peek() == Some('{') {
            self.bump();
            let start = self.offset;
            while !matches!(self.peek(), Some('}') | None) {
                self.bump();
            }
            if self.peek().is_none() {
                return Err(self.error("field name has an unclosed ${...} wrapper"));
            }
            let name = &self.source[start..self.offset];
            self.bump();
            if name.is_empty() {
                return Err(self.error("field name is empty"));
            }
            return Ok(TokenKind::Field(FieldRef::Name(name.to_owned())));
        }
        let start = self.offset;
        while self.peek().is_some_and(|character| {
            !character.is_whitespace()
                && !matches!(
                    character,
                    '+' | '-'
                        | '/'
                        | '*'
                        | '%'
                        | '<'
                        | '>'
                        | '='
                        | '!'
                        | '&'
                        | '|'
                        | '('
                        | ')'
                        | ','
                        | '\''
                        | '"'
                )
        }) {
            self.bump();
        }
        let spelling = &self.source[start..self.offset];
        if spelling.is_empty() {
            return Err(self.error("field reference is empty"));
        }
        if spelling.bytes().all(|byte| byte.is_ascii_digit()) {
            let index = spelling
                .parse::<usize>()
                .map_err(|_| self.error(format!("field index is too large: {spelling}")))?;
            if index == 0 {
                return Err(self.error("field indices are one-based; 0 is invalid"));
            }
            Ok(TokenKind::Field(FieldRef::Index(index)))
        } else {
            Ok(TokenKind::Field(FieldRef::Name(spelling.to_owned())))
        }
    }

    fn word(&mut self) -> Result<TokenKind> {
        let start = self.offset;
        while self
            .peek()
            .is_some_and(|character| character.is_alphanumeric() || character == '_')
        {
            self.bump();
        }
        let word = &self.source[start..self.offset];
        Ok(match word {
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "in" => TokenKind::In,
            "len" | "ulen" => TokenKind::Ident(word.to_owned()),
            _ => return Err(self.error(format!("unknown identifier {word:?}"))),
        })
    }

    fn error(&self, message: impl Into<String>) -> RsomicsError {
        RsomicsError::InvalidInput(format!(
            "expression byte {}: {}",
            self.offset,
            message.into()
        ))
    }
}
