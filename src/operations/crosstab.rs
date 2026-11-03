use std::collections::HashMap;
use std::path::Path;

use rsomics_common::{Result, RsomicsError, reject_output_alias};
use serde::Serialize;

use crate::aggregate::{Group, Plan};
use crate::cli::CrosstabArgs;
use crate::dialect::Dialect;
use crate::fields::Spec;
use crate::io::input::{Compression, open};
use crate::io::output;
use crate::io::reader::Record;
use crate::io::table::TableReader;
use crate::io::writer::RecordWriter;

#[derive(Debug, Serialize)]
pub(crate) struct CrosstabSummary {
    input_records: u64,
    rows: usize,
    columns: usize,
    populated_cells: usize,
    input_compression: Compression,
    output_compression: Compression,
}

pub(crate) fn run(arguments: &CrosstabArgs, json: bool) -> Result<CrosstabSummary> {
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
    let mut reader = TableReader::new(input.reader, dialect)?;
    let header = reader.header().map(|record| record.fields.as_slice());
    let row_field = axis_field(&arguments.row, reader.width(), header)?;
    let column_field = axis_field(&arguments.column, reader.width(), header)?;
    let aggregate = arguments.aggregate.as_deref().unwrap_or("1:count");
    let plan = Plan::compile_cell(aggregate, reader.width(), header)?;
    let mut scratch = plan.new_scratch();
    let mut rows = Axis::default();
    let mut columns = Axis::default();
    let mut cells = HashMap::<(usize, usize), Group>::new();
    let mut record = Record::default();
    let mut input_records = 0u64;
    while reader.next_record_into(&mut record)? {
        let row = rows.intern(&record.fields[row_field]);
        let column = columns.intern(&record.fields[column_field]);
        let group = cells
            .entry((row, column))
            .or_insert_with(|| plan.new_group());
        plan.push(group, &record, false, &mut scratch)?;
        input_records += 1;
    }
    let summary = CrosstabSummary {
        input_records,
        rows: rows.labels.len(),
        columns: columns.labels.len(),
        populated_cells: cells.len(),
        input_compression: input.compression,
        output_compression: if output::gzip_enabled(&arguments.output.output, arguments.output.gzip)
        {
            Compression::Gzip
        } else {
            Compression::Plain
        },
    };
    let rows = rows.sorted();
    let columns = columns.sorted();
    output::write(&arguments.output.output, arguments.output.gzip, |sink| {
        let mut writer = RecordWriter::new(sink, arguments.resolved_output_delimiter());
        if !arguments.output.no_output_header {
            writer
                .write(
                    std::iter::once(b"".as_slice())
                        .chain(columns.iter().map(|(label, _)| label.as_slice())),
                )
                .map_err(RsomicsError::Io)?;
        }
        for (label, row) in &rows {
            let mut fields = Vec::with_capacity(columns.len() + 1);
            fields.push(label.clone());
            for (_, column) in &columns {
                match cells.remove(&(*row, *column)) {
                    Some(group) => fields.extend(plan.finish(group)),
                    None => fields.push(arguments.filler.as_bytes().to_vec()),
                }
            }
            writer
                .write(fields.iter().map(Vec::as_slice))
                .map_err(RsomicsError::Io)?;
        }
        writer.finish().map_err(RsomicsError::Io)?;
        Ok(summary)
    })
}

fn axis_field(spec: &str, width: usize, header: Option<&[Vec<u8>]>) -> Result<usize> {
    let fields = Spec::parse(spec)?.resolve(width, header, false)?;
    if fields.len() != 1 {
        return Err(RsomicsError::InvalidInput(format!(
            "matrix axis {spec:?} resolves to {} fields; expected one",
            fields.len()
        )));
    }
    Ok(fields.indices()[0])
}

#[derive(Default)]
struct Axis {
    labels: HashMap<Vec<u8>, usize>,
}

impl Axis {
    fn intern(&mut self, label: &[u8]) -> usize {
        if let Some(index) = self.labels.get(label) {
            return *index;
        }
        let index = self.labels.len();
        self.labels.insert(label.to_vec(), index);
        index
    }

    fn sorted(self) -> Vec<(Vec<u8>, usize)> {
        let mut labels = self.labels.into_iter().collect::<Vec<_>>();
        labels.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        labels
    }
}
