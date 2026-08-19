use std::io::Write;

use flate2::{Compression, write::GzEncoder};

pub(crate) enum EncodedWriter<W: Write> {
    Plain(W),
    Gzip(GzEncoder<W>),
}

impl<W: Write> EncodedWriter<W> {
    pub(crate) fn plain(sink: W) -> Self {
        Self::Plain(sink)
    }

    pub(crate) fn gzip(sink: W) -> Self {
        Self::Gzip(GzEncoder::new(sink, Compression::default()))
    }

    pub(crate) fn finish(self) -> std::io::Result<W> {
        match self {
            Self::Plain(mut sink) => {
                sink.flush()?;
                Ok(sink)
            }
            Self::Gzip(sink) => sink.finish(),
        }
    }
}

impl<W: Write> Write for EncodedWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(sink) => sink.write(buffer),
            Self::Gzip(sink) => sink.write(buffer),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(sink) => sink.flush(),
            Self::Gzip(sink) => sink.flush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use flate2::read::GzDecoder;

    use super::*;

    #[test]
    fn gzip_finish_exposes_complete_stream() {
        let mut writer = EncodedWriter::gzip(Vec::new());
        writer.write_all(b"a,b\n1,2\n").unwrap();
        let encoded = writer.finish().unwrap();
        let mut decoded = Vec::new();
        GzDecoder::new(encoded.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, b"a,b\n1,2\n");
    }

    #[test]
    fn plain_finish_flushes_and_returns_sink() {
        let mut writer = EncodedWriter::plain(Vec::new());
        writer.write_all(b"a,b\n").unwrap();
        assert_eq!(writer.finish().unwrap(), b"a,b\n");
    }
}
