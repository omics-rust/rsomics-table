use std::fmt;
use std::io::{self, BufRead};

use rsomics_common::RsomicsError;

const READ_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
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
    buffer: Box<[u8]>,
    cursor: usize,
    filled: usize,
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
            buffer: vec![0; READ_BUFFER_SIZE].into_boxed_slice(),
            cursor: 0,
            filled: 0,
        }
    }

    pub(crate) fn next_record(&mut self) -> Result<Option<Record>, RecordError> {
        let mut record = Record::default();
        if self.next_record_into(&mut record)? {
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn next_record_into(&mut self, record: &mut Record) -> Result<bool, RecordError> {
        loop {
            let start_line = self.current_line();
            let start_offset = self.bytes;
            let Some(first) = self.next_byte()? else {
                return Ok(false);
            };
            if self.comment == Some(first) {
                self.skip_comment()?;
                continue;
            }
            self.read_record_into(first, start_line, start_offset, record)?;
            return Ok(true);
        }
    }

    pub(crate) fn bytes_read(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn physical_lines(&self) -> u64 {
        self.newlines + u64::from(self.bytes > 0 && !self.last_was_newline)
    }

    fn read_record_into(
        &mut self,
        first: u8,
        start_line: u64,
        start_offset: u64,
        record: &mut Record,
    ) -> Result<(), RecordError> {
        for field in &mut record.fields {
            field.clear();
        }
        if record.fields.is_empty() {
            record.fields.push(Vec::new());
        }
        let mut field = 0usize;
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
                        if record.fields[field].last() == Some(&b'\r') {
                            record.fields[field].pop();
                        }
                        self.finish(record, field + 1, start_line, start_offset);
                        Ok(())
                    }
                };
            };

            match state {
                State::FieldStart => match byte {
                    b'"' => state = State::Quoted,
                    b'\n' => {
                        self.finish(record, field + 1, start_line, start_offset);
                        return Ok(());
                    }
                    byte if byte == self.delimiter => {
                        field += 1;
                        ensure_field(&mut record.fields, field);
                    }
                    byte => {
                        record.fields[field].push(byte);
                        state = State::Unquoted;
                    }
                },
                State::Unquoted => match byte {
                    b'"' => return Err(self.syntax("bare quote in unquoted field")),
                    b'\n' => {
                        if record.fields[field].last() == Some(&b'\r') {
                            record.fields[field].pop();
                        }
                        self.finish(record, field + 1, start_line, start_offset);
                        return Ok(());
                    }
                    byte if byte == self.delimiter => {
                        field += 1;
                        ensure_field(&mut record.fields, field);
                        state = State::FieldStart;
                    }
                    byte => record.fields[field].push(byte),
                },
                State::Quoted => match byte {
                    b'"' => state = State::AfterQuote,
                    b'\n' => {
                        if record.fields[field].last() == Some(&b'\r') {
                            record.fields[field].pop();
                        }
                        record.fields[field].push(b'\n');
                    }
                    byte => record.fields[field].push(byte),
                },
                State::AfterQuote => match byte {
                    b'"' => {
                        record.fields[field].push(b'"');
                        state = State::Quoted;
                    }
                    b'\r' => match self.next_byte()? {
                        Some(b'\n') | None => {
                            self.finish(record, field + 1, start_line, start_offset);
                            return Ok(());
                        }
                        Some(_) => {
                            return Err(self.syntax("unexpected byte after closing quote"));
                        }
                    },
                    b'\n' => {
                        self.finish(record, field + 1, start_line, start_offset);
                        return Ok(());
                    }
                    byte if byte == self.delimiter => {
                        field += 1;
                        ensure_field(&mut record.fields, field);
                        state = State::FieldStart;
                    }
                    _ => return Err(self.syntax("unexpected byte after closing quote")),
                },
            }
        }
    }

    fn finish(&mut self, record: &mut Record, fields: usize, line: u64, offset: u64) {
        self.records += 1;
        record.fields.truncate(fields);
        record.number = self.records;
        record.line = line;
        record.offset = offset;
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
        if self.cursor == self.filled {
            self.filled = self.source.read(&mut self.buffer)?;
            self.cursor = 0;
            if self.filled == 0 {
                return Ok(None);
            }
        }
        let byte = self.buffer[self.cursor];
        self.cursor += 1;
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

fn ensure_field(fields: &mut Vec<Vec<u8>>, index: usize) {
    if index == fields.len() {
        fields.push(Vec::new());
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

    #[test]
    fn framing_crosses_internal_read_boundaries() {
        let mut input = b"id,value\n1,\"".to_vec();
        input.extend(std::iter::repeat_n(b'x', READ_BUFFER_SIZE));
        input.extend_from_slice(b"\nend\"\n2,done\n");
        let rows = records(&input).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].fields[1].len(), READ_BUFFER_SIZE + 4);
        assert_eq!(&rows[1].fields[1][READ_BUFFER_SIZE..], b"\nend");
        assert_eq!(rows[2].fields[1], b"done");
    }
}
