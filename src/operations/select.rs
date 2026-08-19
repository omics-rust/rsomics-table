use std::path::Path;

use rsomics_common::{Result, RsomicsError, reject_output_alias};
use serde::Serialize;

use crate::cli::SelectArgs;
use crate::dialect::Dialect;
use crate::fields::{Plan, Spec};
use crate::io::input::{Compression, open};
use crate::io::output;
use crate::io::reader::Record;
use crate::io::table::TableReader;
use crate::io::writer::RecordWriter;

#[derive(Debug, Serialize)]
pub(crate) struct SelectSummary {
    records: u64,
    fields: u64,
    input_compression: Compression,
    output_compression: Compression,
}

pub(crate) fn run(arguments: &SelectArgs, json: bool) -> Result<SelectSummary> {
    if json && arguments.output.output == Path::new("-") {
        return Err(RsomicsError::ConfigError(
            "--json requires a named table output".to_owned(),
        ));
    }
    reject_output_alias(&arguments.output.output, [arguments.input.input.as_path()])?;
    let dialect = Dialect::new(
        arguments.input.resolved_delimiter(),
        arguments.input.format.comment,
        !arguments.input.format.no_header,
    )?;
    let spec = Spec::parse(&arguments.fields)?;
    let input = open(&arguments.input.input)?;
    let input_compression = input.compression;
    let output_compression =
        if output::gzip_enabled(&arguments.output.output, arguments.output.gzip) {
            Compression::Gzip
        } else {
            Compression::Plain
        };
    output::write(&arguments.output.output, arguments.output.gzip, |sink| {
        let options = ProcessOptions {
            dialect,
            spec: &spec,
            fuzzy: arguments.fuzzy_fields,
            no_output_header: arguments.output.no_output_header,
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
    let mut reader = TableReader::new(source, options.dialect)?;
    let width = reader.width();
    let mut writer = RecordWriter::new(sink, options.output_delimiter);
    let plan = options.spec.resolve(
        width,
        reader.header().map(|record| record.fields.as_slice()),
        options.fuzzy,
    )?;
    if let Some(header) = reader.header()
        && !options.no_output_header
    {
        write_projected(&mut writer, header, &plan)?;
    }
    let mut records = 0u64;

    while let Some(record) = reader.next_record()? {
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
