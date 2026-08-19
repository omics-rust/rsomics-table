use std::fmt;
use std::io::{self, BufRead};

use rsomics_common::RsomicsError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Record {
    pub(crate) fields: Vec<Vec<u8>>,
    pub(crate) number: u64,
    pub(crate) line: u64,
    pub(crate) offset: u64,
}

#[derive(Debug)]
pub(crate) enum RecordError {
    Io(io::Error),
    Syntax {
        record: u64,
        line: u64,
        offset: u64,
        message: &'static str,
    },
}

impl fmt::Display for RecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Syntax {
                record,
                line,
                offset,
                message,
            } => write!(
                formatter,
                "record {record}, line {line}, byte {offset}: {message}"
            ),
        }
    }
}

impl From<io::Error> for RecordError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<RecordError> for RsomicsError {
    fn from(error: RecordError) -> Self {
        match error {
            RecordError::Io(error) => Self::Io(error),
            error @ RecordError::Syntax { .. } => Self::InvalidInput(error.to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    FieldStart,
    Unquoted,
    Quoted,
    AfterQuote,
}

pub(crate) struct RecordReader<R: BufRead> {
    source: R,
    delimiter: u8,
    comment: Option<u8>,
    bytes: u64,
    newlines: u64,
    last_was_newline: bool,
    records: u64,
}

impl<R: BufRead> RecordReader<R> {
    pub(crate) fn new(source: R, delimiter: u8, comment: Option<u8>) -> Self {
        Self {
            source,
            delimiter,
            comment,
            bytes: 0,
            newlines: 0,
            last_was_newline: false,
            records: 0,
        }
    }

    pub(crate) fn next_record(&mut self) -> Result<Option<Record>, RecordError> {
        loop {
            let start_line = self.current_line();
            let start_offset = self.bytes;
            let Some(first) = self.next_byte()? else {
                return Ok(None);
            };
            if self.comment == Some(first) {
                self.skip_comment()?;
                continue;
            }
            return self.read_record(first, start_line, start_offset).map(Some);
        }
    }

    pub(crate) fn bytes_read(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn physical_lines(&self) -> u64 {
        self.newlines + u64::from(self.bytes > 0 && !self.last_was_newline)
    }

    fn read_record(
        &mut self,
        first: u8,
        start_line: u64,
        start_offset: u64,
    ) -> Result<Record, RecordError> {
        let mut fields = Vec::new();
        let mut field = Vec::new();
        let mut state = State::FieldStart;
        let mut current = Some(first);

        loop {
            let byte = match current.take() {
                Some(byte) => Some(byte),
                None => self.next_byte()?,
            };
            let Some(byte) = byte else {
                return match state {
                    State::Quoted => Err(self.syntax("unterminated quoted field")),
                    _ => {
                        if field.last() == Some(&b'\r') {
                            field.pop();
                        }
                        fields.push(field);
                        Ok(self.finish(fields, start_line, start_offset))
                    }
                };
            };

            match state {
                State::FieldStart => match byte {
                    b'"' => state = State::Quoted,
                    b'\n' => {
                        fields.push(field);
                        return Ok(self.finish(fields, start_line, start_offset));
                    }
                    byte if byte == self.delimiter => fields.push(Vec::new()),
                    byte => {
                        field.push(byte);
                        state = State::Unquoted;
                    }
                },
                State::Unquoted => match byte {
                    b'"' => return Err(self.syntax("bare quote in unquoted field")),
                    b'\n' => {
                        if field.last() == Some(&b'\r') {
                            field.pop();
                        }
                        fields.push(field);
                        return Ok(self.finish(fields, start_line, start_offset));
                    }
                    byte if byte == self.delimiter => {
                        fields.push(std::mem::take(&mut field));
                        state = State::FieldStart;
                    }
                    byte => field.push(byte),
                },
                State::Quoted => match byte {
                    b'"' => state = State::AfterQuote,
                    b'\n' => {
                        if field.last() == Some(&b'\r') {
                            field.pop();
                        }
                        field.push(b'\n');
                    }
                    byte => field.push(byte),
                },
                State::AfterQuote => match byte {
                    b'"' => {
                        field.push(b'"');
                        state = State::Quoted;
                    }
                    b'\r' => match self.next_byte()? {
                        Some(b'\n') | None => {
                            fields.push(field);
                            return Ok(self.finish(fields, start_line, start_offset));
                        }
                        Some(_) => {
                            return Err(self.syntax("unexpected byte after closing quote"));
                        }
                    },
                    b'\n' => {
                        fields.push(field);
                        return Ok(self.finish(fields, start_line, start_offset));
                    }
                    byte if byte == self.delimiter => {
                        fields.push(std::mem::take(&mut field));
                        state = State::FieldStart;
                    }
                    _ => return Err(self.syntax("unexpected byte after closing quote")),
                },
            }
        }
    }

    fn finish(&mut self, fields: Vec<Vec<u8>>, line: u64, offset: u64) -> Record {
        self.records += 1;
        Record {
            fields,
            number: self.records,
            line,
            offset,
        }
    }

    fn skip_comment(&mut self) -> Result<(), RecordError> {
        while let Some(byte) = self.next_byte()? {
            if byte == b'\n' {
                break;
            }
        }
        Ok(())
    }

    fn next_byte(&mut self) -> Result<Option<u8>, RecordError> {
        let buffer = self.source.fill_buf()?;
        let Some(&byte) = buffer.first() else {
            return Ok(None);
        };
        self.source.consume(1);
        self.bytes += 1;
        self.last_was_newline = byte == b'\n';
        if self.last_was_newline {
            self.newlines += 1;
        }
        Ok(Some(byte))
    }

    fn current_line(&self) -> u64 {
        self.newlines + 1
    }

    fn syntax(&self, message: &'static str) -> RecordError {
        RecordError::Syntax {
            record: self.records + 1,
            line: self.current_line(),
            offset: self.bytes.saturating_sub(1),
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use super::*;

    fn records(input: &[u8]) -> Result<Vec<Record>, RecordError> {
        let mut reader = RecordReader::new(BufReader::new(Cursor::new(input)), b',', Some(b'#'));
        let mut records = Vec::new();
        while let Some(record) = reader.next_record()? {
            records.push(record);
        }
        Ok(records)
    }

    #[test]
    fn parses_quotes_delimiters_and_embedded_newlines() {
        let rows = records(b"a,b\n1,\"x,y\"\n2,\"left\nright\"\n").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].fields[1], b"x,y");
        assert_eq!(rows[2].fields[1], b"left\nright");
        assert_eq!(rows[2].line, 3);
    }

    #[test]
    fn normalizes_crlf_but_preserves_bare_cr() {
        let rows = records(b"v\r\na\rb\r\n\"c\r\nd\"\r\n").unwrap();
        assert_eq!(rows[0].fields[0], b"v");
        assert_eq!(rows[1].fields[0], b"a\rb");
        assert_eq!(rows[2].fields[0], b"c\nd");
    }

    #[test]
    fn skips_explicit_comments_only_at_line_start() {
        let rows = records(b"# note\na,b\n x,#value\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].fields[1], b"#value");
        assert_eq!(rows[0].number, 1);
        assert_eq!(rows[0].line, 2);
    }

    #[test]
    fn rejects_quote_errors_with_position() {
        let error = records(b"a,b\n1,a\"b\n").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("record 2"), "{message}");
        assert!(message.contains("bare quote"), "{message}");
    }

    #[test]
    fn returns_empty_records_instead_of_silently_skipping() {
        let rows = records(b"a\n\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].fields, vec![Vec::<u8>::new()]);
    }
}
