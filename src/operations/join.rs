use std::collections::HashMap;
use std::path::Path;

use rsomics_common::{Result, RsomicsError, reject_output_alias};
use serde::Serialize;

use crate::cli::{JoinArgs, JoinKind};
use crate::dialect::Dialect;
use crate::io::input::{Compression, open};
use crate::io::output;
use crate::io::reader::Record;
use crate::io::table::TableReader;
use crate::io::writer::RecordWriter;
use crate::join::{Plan, PlanOptions, Side};

#[derive(Debug, Serialize)]
pub(crate) struct JoinSummary {
    left_records: u64,
    right_records: u64,
    output_records: u64,
    matched_pairs: u64,
    left_compression: Compression,
    right_compression: Compression,
    output_compression: Compression,
}

pub(crate) fn run(arguments: &JoinArgs, json: bool) -> Result<JoinSummary> {
    if json && arguments.output.output == Path::new("-") {
        return Err(RsomicsError::ConfigError(
            "--json requires a named table output".to_owned(),
        ));
    }
    if arguments.left == Path::new("-") && arguments.right == Path::new("-") {
        return Err(RsomicsError::ConfigError(
            "only one join input may read standard input".to_owned(),
        ));
    }
    reject_output_alias(
        &arguments.output.output,
        [arguments.left.as_path(), arguments.right.as_path()],
    )?;
    let (left_spec, right_spec) = key_specs(arguments)?;
    let dialect = Dialect::new(
        arguments.input.resolved_delimiter(),
        arguments.input.comment,
        !arguments.input.no_header,
    )?;

    let right_input = open(&arguments.right)?;
    let right_compression = right_input.compression;
    let mut right_reader = TableReader::new(right_input.reader, dialect)?;
    let right_width = right_reader.width();
    let right_header = right_reader.header().cloned();
    let mut right_rows = Vec::new();
    while let Some(record) = right_reader.next_record()? {
        right_rows.push(record);
    }

    let left_input = open(&arguments.left)?;
    let left_compression = left_input.compression;
    let mut left_reader = TableReader::new(left_input.reader, dialect)?;
    let left_width = left_reader.width();
    let left_header = left_reader.header().cloned();
    let plan = Plan::compile(PlanOptions {
        left: Side {
            spec: left_spec,
            width: left_width,
            header: left_header.as_ref().map(|record| record.fields.as_slice()),
        },
        right: Side {
            spec: right_spec,
            width: right_width,
            header: right_header.as_ref().map(|record| record.fields.as_slice()),
        },
        right_suffix: &arguments.right_suffix,
        ignore_case: arguments.ignore_case,
        null_never_matches: arguments.null_never_matches,
    })?;

    let mut groups = HashMap::<Vec<u8>, Vec<usize>>::new();
    for (index, record) in right_rows.iter().enumerate() {
        if let Some(key) = plan.right_key(record)? {
            groups.entry(key).or_default().push(index);
        }
    }
    let output_compression =
        if output::gzip_enabled(&arguments.output.output, arguments.output.gzip) {
            Compression::Gzip
        } else {
            Compression::Plain
        };
    output::write(&arguments.output.output, arguments.output.gzip, |sink| {
        let options = ProcessOptions {
            right: &right_rows,
            groups: &groups,
            plan: &plan,
            arguments,
            left_compression,
            right_compression,
            output_compression,
        };
        process(&mut left_reader, sink, &options)
    })
}

struct ProcessOptions<'a> {
    right: &'a [Record],
    groups: &'a HashMap<Vec<u8>, Vec<usize>>,
    plan: &'a Plan,
    arguments: &'a JoinArgs,
    left_compression: Compression,
    right_compression: Compression,
    output_compression: Compression,
}

fn process(
    left: &mut TableReader<Box<dyn std::io::BufRead>>,
    sink: &mut dyn std::io::Write,
    options: &ProcessOptions<'_>,
) -> Result<JoinSummary> {
    let right = options.right;
    let plan = options.plan;
    let arguments = options.arguments;
    let mut writer = RecordWriter::new(sink, arguments.resolved_output_delimiter());
    if let Some(header) = plan.output_header()
        && !arguments.output.no_output_header
    {
        writer
            .write(header.iter().map(Vec::as_slice))
            .map_err(RsomicsError::Io)?;
    }
    let fill = arguments.fill.as_bytes();
    let mut matched = vec![false; right.len()];
    let mut left_records = 0u64;
    let mut output_records = 0u64;
    let mut matched_pairs = 0u64;
    let mut left_record = Record::default();
    while left.next_record_into(&mut left_record)? {
        left_records += 1;
        let key = plan.left_key(&left_record)?;
        let matches = key.as_ref().and_then(|key| options.groups.get(key));
        if let Some(indices) = matches {
            for &index in indices {
                matched[index] = true;
                writer
                    .write(
                        left_record.fields.iter().map(Vec::as_slice).chain(
                            plan.right_values()
                                .iter()
                                .map(|field| right[index].fields[*field].as_slice()),
                        ),
                    )
                    .map_err(RsomicsError::Io)?;
                output_records += 1;
                matched_pairs += 1;
            }
        } else if matches!(arguments.kind, JoinKind::Left | JoinKind::Full) {
            writer
                .write(
                    left_record
                        .fields
                        .iter()
                        .map(Vec::as_slice)
                        .chain(std::iter::repeat_n(fill, plan.right_values().len())),
                )
                .map_err(RsomicsError::Io)?;
            output_records += 1;
        }
    }
    if arguments.kind == JoinKind::Full {
        for (index, right_record) in right.iter().enumerate() {
            if matched[index] {
                continue;
            }
            writer
                .write(
                    plan.left_sources()
                        .iter()
                        .map(|source| {
                            source.map_or(fill, |field| right_record.fields[field].as_slice())
                        })
                        .chain(
                            plan.right_values()
                                .iter()
                                .map(|field| right_record.fields[*field].as_slice()),
                        ),
                )
                .map_err(RsomicsError::Io)?;
            output_records += 1;
        }
    }
    writer.finish().map_err(RsomicsError::Io)?;
    Ok(JoinSummary {
        left_records,
        right_records: right.len() as u64,
        output_records,
        matched_pairs,
        left_compression: options.left_compression,
        right_compression: options.right_compression,
        output_compression: options.output_compression,
    })
}

fn key_specs(arguments: &JoinArgs) -> Result<(&str, &str)> {
    if let Some(on) = &arguments.on {
        return Ok((on, on));
    }
    match (&arguments.left_on, &arguments.right_on) {
        (Some(left), Some(right)) => Ok((left, right)),
        _ => Err(RsomicsError::ConfigError(
            "use --on or both --left-on and --right-on".to_owned(),
        )),
    }
}
