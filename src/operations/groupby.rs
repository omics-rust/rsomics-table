use std::collections::{HashMap, HashSet};
use std::path::Path;

use rsomics_common::{Result, RsomicsError, reject_output_alias};
use serde::Serialize;

use crate::aggregate::{Group, Key, Plan};
use crate::cli::GroupbyArgs;
use crate::dialect::Dialect;
use crate::io::input::{Compression, open};
use crate::io::output;
use crate::io::reader::Record;
use crate::io::table::TableReader;
use crate::io::writer::RecordWriter;

#[derive(Debug, Serialize)]
pub(crate) struct GroupbySummary {
    input_records: u64,
    groups: u64,
    aggregates: u64,
    ignored_numeric_values: u64,
    consecutive: bool,
    input_compression: Compression,
    output_compression: Compression,
}

pub(crate) fn run(arguments: &GroupbyArgs, json: bool) -> Result<GroupbySummary> {
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
            group: arguments.group.as_deref(),
            aggregates: &arguments.aggregates,
            consecutive: arguments.consecutive,
            ignore_non_numeric: arguments.ignore_non_numeric,
            collapse_delimiter: &arguments.collapse_delimiter,
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
    group: Option<&'a str>,
    aggregates: &'a [String],
    consecutive: bool,
    ignore_non_numeric: bool,
    collapse_delimiter: &'a str,
    no_output_header: bool,
    output_delimiter: u8,
    input_compression: Compression,
    output_compression: Compression,
}

fn process(
    source: Box<dyn std::io::BufRead>,
    sink: &mut dyn std::io::Write,
    options: &ProcessOptions<'_>,
) -> Result<GroupbySummary> {
    let mut reader = TableReader::new(source, options.dialect)?;
    let plan = Plan::compile(
        options.group,
        options.aggregates,
        reader.width(),
        reader.header().map(|record| record.fields.as_slice()),
        options.collapse_delimiter,
    )?;
    let mut writer = RecordWriter::new(sink, options.output_delimiter);
    if let Some(header) = plan.output_header()
        && !options.no_output_header
    {
        writer
            .write(header.iter().map(Vec::as_slice))
            .map_err(RsomicsError::Io)?;
    }
    let progress = if options.consecutive {
        consecutive(&mut reader, &mut writer, &plan, options.ignore_non_numeric)?
    } else {
        global(&mut reader, &mut writer, &plan, options.ignore_non_numeric)?
    };
    writer.finish().map_err(RsomicsError::Io)?;
    Ok(GroupbySummary {
        input_records: progress.records,
        groups: progress.groups,
        aggregates: plan.aggregate_count() as u64,
        ignored_numeric_values: progress.ignored,
        consecutive: options.consecutive,
        input_compression: options.input_compression,
        output_compression: options.output_compression,
    })
}

struct Progress {
    records: u64,
    groups: u64,
    ignored: u64,
}

fn global(
    reader: &mut TableReader<Box<dyn std::io::BufRead>>,
    writer: &mut RecordWriter<&mut dyn std::io::Write>,
    plan: &Plan,
    ignore_non_numeric: bool,
) -> Result<Progress> {
    let mut groups = HashMap::<Key, Group>::new();
    let mut scratch = plan.new_scratch();
    let mut record = Record::default();
    let mut records = 0u64;
    let mut ignored = 0u64;
    while reader.next_record_into(&mut record)? {
        let key = plan.key(&record);
        let group = groups.entry(key).or_insert_with(|| plan.new_group());
        ignored += plan.push(group, &record, ignore_non_numeric, &mut scratch)?;
        records += 1;
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let group_count = groups.len() as u64;
    for (key, group) in groups {
        write_group(writer, plan, &key, group)?;
    }
    Ok(Progress {
        records,
        groups: group_count,
        ignored,
    })
}

fn consecutive(
    reader: &mut TableReader<Box<dyn std::io::BufRead>>,
    writer: &mut RecordWriter<&mut dyn std::io::Write>,
    plan: &Plan,
    ignore_non_numeric: bool,
) -> Result<Progress> {
    let mut current = None::<(Key, Group)>;
    let mut completed = HashSet::new();
    let mut scratch = plan.new_scratch();
    let mut record = Record::default();
    let mut records = 0u64;
    let mut groups = 0u64;
    let mut ignored = 0u64;
    while reader.next_record_into(&mut record)? {
        let changed = current
            .as_ref()
            .is_some_and(|(active, _)| !plan.key_matches(active, &record));
        if changed {
            let key = plan.key(&record);
            if completed.contains(&key) {
                return Err(RsomicsError::InvalidInput(format!(
                    "record {}, line {}: consecutive group {:?} reappears",
                    record.number,
                    record.line,
                    display_key(&key)
                )));
            }
            if let Some((previous, group)) = current.take() {
                write_group(writer, plan, &previous, group)?;
                completed.insert(previous);
                groups += 1;
            }
            current = Some((key, plan.new_group()));
        }
        let (_, group) = current.get_or_insert_with(|| (plan.key(&record), plan.new_group()));
        ignored += plan.push(group, &record, ignore_non_numeric, &mut scratch)?;
        records += 1;
    }
    if let Some((key, group)) = current {
        write_group(writer, plan, &key, group)?;
        groups += 1;
    }
    Ok(Progress {
        records,
        groups,
        ignored,
    })
}

fn write_group(
    writer: &mut RecordWriter<&mut dyn std::io::Write>,
    plan: &Plan,
    key: &Key,
    group: Group,
) -> Result<()> {
    let values = plan.finish(group);
    writer
        .write(
            key.fields()
                .iter()
                .map(Vec::as_slice)
                .chain(values.iter().map(Vec::as_slice)),
        )
        .map_err(RsomicsError::Io)
}

fn display_key(key: &Key) -> Vec<String> {
    key.fields()
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}
