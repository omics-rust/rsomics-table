use std::io::{self, Write};

pub(crate) struct RecordWriter<W: Write> {
    sink: W,
    delimiter: u8,
    line: Vec<u8>,
}

impl<W: Write> RecordWriter<W> {
    pub(crate) fn new(sink: W, delimiter: u8) -> Self {
        Self {
            sink,
            delimiter,
            line: Vec::with_capacity(256),
        }
    }

    pub(crate) fn write<'a>(
        &mut self,
        fields: impl IntoIterator<Item = &'a [u8]>,
    ) -> io::Result<()> {
        self.line.clear();
        for (index, field) in fields.into_iter().enumerate() {
            if index > 0 {
                self.line.push(self.delimiter);
            }
            encode(field, self.delimiter, &mut self.line);
        }
        self.line.push(b'\n');
        self.sink.write_all(&self.line)
    }

    pub(crate) fn finish(mut self) -> io::Result<W> {
        self.sink.flush()?;
        Ok(self.sink)
    }
}

fn encode(field: &[u8], delimiter: u8, output: &mut Vec<u8>) {
    if !needs_quotes(field, delimiter) {
        output.extend_from_slice(field);
        return;
    }
    output.push(b'"');
    for byte in field {
        if *byte == b'"' {
            output.extend_from_slice(b"\"\"");
        } else {
            output.push(*byte);
        }
    }
    output.push(b'"');
}

fn needs_quotes(field: &[u8], delimiter: u8) -> bool {
    if field.is_empty() {
        return false;
    }
    if field == br"\." {
        return true;
    }
    if field
        .iter()
        .any(|byte| matches!(*byte, b'\r' | b'\n' | b'"') || *byte == delimiter)
    {
        return true;
    }
    first_char(field).is_some_and(char::is_whitespace)
}

fn first_char(field: &[u8]) -> Option<char> {
    let width = match *field.first()? {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => return None,
    };
    std::str::from_utf8(field.get(..width)?)
        .ok()?
        .chars()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_go_style_quoting() {
        let mut writer = RecordWriter::new(Vec::new(), b',');
        writer
            .write([b"plain".as_slice(), b"x,y", b" lead", b"he \"said\""])
            .unwrap();
        assert_eq!(
            writer.finish().unwrap(),
            b"plain,\"x,y\",\" lead\",\"he \"\"said\"\"\"\n"
        );
    }

    #[test]
    fn preserves_invalid_utf8() {
        let mut writer = RecordWriter::new(Vec::new(), b',');
        writer.write([b"a".as_slice(), &[0xff]]).unwrap();
        assert_eq!(writer.finish().unwrap(), b"a,\xff\n");
    }

    #[test]
    fn leading_space_quotes_even_with_later_invalid_utf8() {
        let mut writer = RecordWriter::new(Vec::new(), b',');
        writer.write([[b' ', 0xff].as_slice()]).unwrap();
        assert_eq!(writer.finish().unwrap(), b"\" \xff\"\n");
    }
}
