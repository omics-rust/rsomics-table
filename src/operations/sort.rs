use std::path::Path;

use rsomics_common::{Result, RsomicsError, reject_output_alias};
use serde::Serialize;

use crate::cli::SortArgs;
use crate::dialect::Dialect;
use crate::io::input::{Compression, open};
use crate::io::output;
use crate::io::table::TableReader;
use crate::io::writer::RecordWriter;
use crate::ordering::Plan;

#[derive(Debug, Serialize)]
pub(crate) struct SortSummary {
    records: u64,
    fields: u64,
    keys: u64,
    threads: u64,
    input_compression: Compression,
    output_compression: Compression,
}

pub(crate) fn run(arguments: &SortArgs, json: bool) -> Result<SortSummary> {
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
            keys: &arguments.keys,
            ignore_case: arguments.ignore_case,
            threads: arguments.threads.get(),
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
    keys: &'a [String],
    ignore_case: bool,
    threads: usize,
    no_output_header: bool,
    output_delimiter: u8,
    input_compression: Compression,
    output_compression: Compression,
}

fn process(
    source: Box<dyn std::io::BufRead>,
    sink: &mut dyn std::io::Write,
    options: &ProcessOptions<'_>,
) -> Result<SortSummary> {
    let mut reader = TableReader::new(source, options.dialect)?;
    let header = reader.header().cloned();
    let mut rows = Vec::new();
    while let Some(record) = reader.next_record()? {
        rows.push(record);
    }
    let plan = Plan::compile(
        options.keys,
        reader.width(),
        header.as_ref().map(|record| record.fields.as_slice()),
        &rows,
        options.ignore_case,
    )?;
    plan.sort(&mut rows, options.threads)?;

    let mut writer = RecordWriter::new(sink, options.output_delimiter);
    if let Some(header) = &header
        && !options.no_output_header
    {
        writer
            .write(header.fields.iter().map(Vec::as_slice))
            .map_err(RsomicsError::Io)?;
    }
    for record in &rows {
        writer
            .write(record.fields.iter().map(Vec::as_slice))
            .map_err(RsomicsError::Io)?;
    }
    writer.finish().map_err(RsomicsError::Io)?;
    Ok(SortSummary {
        records: rows.len() as u64,
        fields: reader.width() as u64,
        keys: plan.len() as u64,
        threads: options.threads as u64,
        input_compression: options.input_compression,
        output_compression: options.output_compression,
    })
}
