use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use flate2::bufread::MultiGzDecoder;
use serde::Serialize;

use rsomics_common::{Context, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Compression {
    Plain,
    Gzip,
}

pub(crate) struct Input {
    pub(crate) reader: Box<dyn BufRead>,
    pub(crate) compression: Compression,
}

pub(crate) fn open(path: &Path) -> Result<Input> {
    let raw: Box<dyn Read> = if path == Path::new("-") {
        Box::new(std::io::stdin())
    } else {
        Box::new(File::open(path).rs_with_context(|| format!("opening input {}", path.display()))?)
    };
    from_reader(raw)
}

fn from_reader(raw: Box<dyn Read>) -> Result<Input> {
    let mut buffered = BufReader::new(raw);
    let gzip = buffered.fill_buf()?.starts_with(&[0x1f, 0x8b]);
    if gzip {
        Ok(Input {
            reader: Box::new(BufReader::new(MultiGzDecoder::new(buffered))),
            compression: Compression::Gzip,
        })
    } else {
        Ok(Input {
            reader: Box::new(buffered),
            compression: Compression::Plain,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use flate2::{Compression as Level, write::GzEncoder};

    use super::*;

    #[test]
    fn detects_stream_magic_instead_of_filename() {
        let mut encoder = GzEncoder::new(Vec::new(), Level::default());
        encoder.write_all(b"a,b\n1,2\n").unwrap();
        let compressed = encoder.finish().unwrap();
        let mut input = from_reader(Box::new(Cursor::new(compressed))).unwrap();
        assert_eq!(input.compression, Compression::Gzip);
        let mut decoded = Vec::new();
        input.reader.read_to_end(&mut decoded).unwrap();
        assert_eq!(decoded, b"a,b\n1,2\n");
    }
}
