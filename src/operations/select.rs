use std::path::Path;

use rsomics_common::{Result, RsomicsError, reject_output_alias};
use serde::Serialize;

use crate::cli::SelectArgs;
use crate::dialect::Dialect;
use crate::fields::{Plan, Spec, validate_header};
use crate::io::input::{Compression, open};
use crate::io::output;
use crate::io::reader::{Record, RecordReader};
use crate::io::writer::RecordWriter;

#[derive(Debug, Serialize)]
pub(crate) struct SelectSummary {
    records: u64,
    fields: u64,
    input_compression: Compression,
    output_compression: Compression,
}

pub(crate) fn run(arguments: &SelectArgs, json: bool) -> Result<SelectSummary> {
    if json && arguments.output == Path::new("-") {
        return Err(RsomicsError::ConfigError(
            "--json requires a named table output".to_owned(),
        ));
    }
    reject_output_alias(&arguments.output, [arguments.input.input.as_path()])?;
    let dialect = Dialect::new(
        arguments.input.resolved_delimiter(),
        arguments.input.comment,
        !arguments.input.no_header,
    )?;
    let spec = Spec::parse(&arguments.fields)?;
    let input = open(&arguments.input.input)?;
    let input_compression = input.compression;
    let output_compression = if output::gzip_enabled(&arguments.output, arguments.gzip) {
        Compression::Gzip
    } else {
        Compression::Plain
    };
    output::write(&arguments.output, arguments.gzip, |sink| {
        let options = ProcessOptions {
            dialect,
            spec: &spec,
            fuzzy: arguments.fuzzy_fields,
            no_output_header: arguments.no_output_header,
            output_delimiter: arguments.resolved_output_delimiter(),
            input_compression,
            output_compression,
        };
        process(input.reader, sink, &options)
    })
}

struct ProcessOptions<'a> {
    dialect: Dialect,
    spec: &'a Spec,
    fuzzy: bool,
    no_output_header: bool,
    output_delimiter: u8,
    input_compression: Compression,
    output_compression: Compression,
}

fn process(
    source: Box<dyn std::io::BufRead>,
    sink: &mut dyn std::io::Write,
    options: &ProcessOptions<'_>,
) -> Result<SelectSummary> {
    let mut reader = RecordReader::new(source, options.dialect.delimiter, options.dialect.comment);
    let first = reader
        .next_record()?
        .ok_or_else(|| RsomicsError::InvalidInput("input table is empty".to_owned()))?;
    let width = first.fields.len();
    let mut writer = RecordWriter::new(sink, options.output_delimiter);
    let (plan, mut records) = if options.dialect.header {
        validate_header(&first.fields)?;
        let plan = options
            .spec
            .resolve(width, Some(&first.fields), options.fuzzy)?;
        if !options.no_output_header {
            write_projected(&mut writer, &first, &plan)?;
        }
        (plan, 0u64)
    } else {
        let plan = options.spec.resolve(width, None, options.fuzzy)?;
        write_projected(&mut writer, &first, &plan)?;
        (plan, 1u64)
    };

    while let Some(record) = reader.next_record()? {
        check_width(&record, width)?;
        write_projected(&mut writer, &record, &plan)?;
        records += 1;
    }
    writer.finish().map_err(RsomicsError::Io)?;
    Ok(SelectSummary {
        records,
        fields: plan.len() as u64,
        input_compression: options.input_compression,
        output_compression: options.output_compression,
    })
}

fn write_projected<W: std::io::Write>(
    writer: &mut RecordWriter<W>,
    record: &Record,
    plan: &Plan,
) -> Result<()> {
    writer
        .write(
            plan.indices()
                .iter()
                .map(|index| record.fields[*index].as_slice()),
        )
        .map_err(RsomicsError::Io)
}

fn check_width(record: &Record, expected: usize) -> Result<()> {
    if record.fields.len() == expected {
        return Ok(());
    }
    Err(RsomicsError::InvalidInput(format!(
        "record {} has {} fields; expected {expected}",
        record.number,
        record.fields.len()
    )))
}
