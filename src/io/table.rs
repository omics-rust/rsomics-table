use std::io::BufRead;

use rsomics_common::{Result, RsomicsError};

use crate::dialect::Dialect;
use crate::fields::validate_header;
use crate::io::reader::{Record, RecordReader};

pub(crate) struct TableReader<R: BufRead> {
    records: RecordReader<R>,
    header: Option<Record>,
    pending: Option<Record>,
    width: usize,
}

impl<R: BufRead> TableReader<R> {
    pub(crate) fn new(source: R, dialect: Dialect) -> Result<Self> {
        let mut records = RecordReader::new(source, dialect.delimiter, dialect.comment);
        let first = records
            .next_record()?
            .ok_or_else(|| RsomicsError::InvalidInput("input table is empty".to_owned()))?;
        let width = first.fields.len();
        let (header, pending) = if dialect.header {
            validate_header(&first.fields)?;
            (Some(first), None)
        } else {
            (None, Some(first))
        };
        Ok(Self {
            records,
            header,
            pending,
            width,
        })
    }

    pub(crate) fn header(&self) -> Option<&Record> {
        self.header.as_ref()
    }

    pub(crate) fn width(&self) -> usize {
        self.width
    }

    pub(crate) fn next_record(&mut self) -> Result<Option<Record>> {
        let mut record = Record::default();
        if self.next_record_into(&mut record)? {
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn next_record_into(&mut self, record: &mut Record) -> Result<bool> {
        let present = if let Some(pending) = self.pending.take() {
            *record = pending;
            true
        } else {
            self.records.next_record_into(record)?
        };
        if !present {
            return Ok(false);
        }
        if record.fields.len() != self.width {
            return Err(RsomicsError::InvalidInput(format!(
                "record {} has {} fields; expected {}",
                record.number,
                record.fields.len(),
                self.width
            )));
        }
        Ok(true)
    }
}
