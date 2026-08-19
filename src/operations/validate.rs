use std::io;

use rsomics_common::{Result, RsomicsError, Validation};
use serde::Serialize;

use crate::cli::ValidateArgs;
use crate::dialect::Dialect;
use crate::fields::header_issues;
use crate::io::input::{Compression, open};
use crate::io::reader::{Record, RecordError, RecordReader};

#[derive(Debug, Serialize)]
pub(crate) struct ValidationReport {
    pub(crate) records: u64,
    pub(crate) fields: u64,
    pub(crate) physical_lines: u64,
    pub(crate) uncompressed_bytes: u64,
    pub(crate) header: bool,
    pub(crate) delimiter: char,
    pub(crate) compression: Compression,
    pub(crate) errors: Vec<ValidationIssue>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ValidationIssue {
    record: u64,
    line: u64,
    byte_offset: u64,
    message: String,
}

pub(crate) fn run(arguments: &ValidateArgs) -> Result<Validation<ValidationReport>> {
    let dialect = Dialect::new(
        arguments.input.resolved_delimiter(),
        arguments.input.comment,
        !arguments.input.no_header,
    )?;
    let input = open(&arguments.input.input)?;
    let compression = input.compression;
    let mut reader = RecordReader::new(input.reader, dialect.delimiter, dialect.comment);
    let mut expected_width = None;
    let mut data_records = 0u64;
    let mut errors = Vec::new();
    let limit = arguments.max_errors.get();
    let mut first = true;

    loop {
        let record = match reader.next_record() {
            Ok(Some(record)) => record,
            Ok(None) => break,
            Err(error) => {
                errors.push(issue_from_reader(error, compression)?);
                break;
            }
        };

        if first {
            first = false;
            expected_width = Some(record.fields.len());
            if dialect.header {
                errors.extend(
                    header_issues(&record.fields, arguments.utf8, limit)
                        .into_iter()
                        .map(|message| issue(&record, message)),
                );
                if errors.len() >= limit {
                    break;
                }
                continue;
            }
        }

        if record.fields.len() != expected_width.unwrap_or(record.fields.len()) {
            let expected = expected_width.unwrap();
            errors.push(issue(
                &record,
                format!(
                    "record {} has {} {}; expected {expected}",
                    record.number,
                    record.fields.len(),
                    plural(record.fields.len(), "field", "fields")
                ),
            ));
        }
        if arguments.utf8 && errors.len() < limit {
            for (index, field) in record.fields.iter().enumerate() {
                if std::str::from_utf8(field).is_err() {
                    errors.push(issue(
                        &record,
                        format!("field {} is not valid UTF-8", index + 1),
                    ));
                    break;
                }
            }
        }
        data_records += 1;
        if errors.len() >= limit {
            break;
        }
    }

    if first && dialect.header {
        errors.push(ValidationIssue {
            record: 1,
            line: 1,
            byte_offset: 0,
            message: "header-mode input is empty".to_owned(),
        });
    }

    let report = ValidationReport {
        records: data_records,
        fields: expected_width.unwrap_or(0) as u64,
        physical_lines: reader.physical_lines(),
        uncompressed_bytes: reader.bytes_read(),
        header: dialect.header,
        delimiter: char::from(dialect.delimiter),
        compression,
        errors,
    };
    if report.errors.is_empty() {
        Ok(Validation::Valid(report))
    } else {
        let message = report.errors[0].message.clone();
        Ok(Validation::Invalid { report, message })
    }
}

fn issue(record: &Record, message: String) -> ValidationIssue {
    ValidationIssue {
        record: record.number,
        line: record.line,
        byte_offset: record.offset,
        message,
    }
}

fn issue_from_reader(error: RecordError, compression: Compression) -> Result<ValidationIssue> {
    match error {
        RecordError::Syntax {
            record,
            line,
            offset,
            message,
        } => Ok(ValidationIssue {
            record,
            line,
            byte_offset: offset,
            message: format!("record {record}, line {line}, byte {offset}: {message}"),
        }),
        RecordError::Io(error)
            if compression == Compression::Gzip
                && matches!(
                    error.kind(),
                    io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof
                ) =>
        {
            Ok(ValidationIssue {
                record: 0,
                line: 0,
                byte_offset: 0,
                message: format!("invalid gzip stream: {error}"),
            })
        }
        RecordError::Io(error) => Err(RsomicsError::Io(error)),
    }
}

fn plural(value: usize, singular: &'static str, plural: &'static str) -> &'static str {
    if value == 1 { singular } else { plural }
}
