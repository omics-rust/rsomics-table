pub(crate) mod numeric;
pub(crate) mod order;
pub(crate) mod text;

use std::collections::{HashMap, HashSet};

use rsomics_common::{Result, RsomicsError};

use crate::fields::Spec as FieldSpec;
use crate::io::reader::Record;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Numeric(numeric::Operation),
    Order(order::Operation),
    Text(text::Operation),
}

impl Kind {
    fn candidate(value: &str) -> Result<Option<Self>> {
        let kind = match value {
            "sum" => Self::Numeric(numeric::Operation::Sum),
            "min" => Self::Numeric(numeric::Operation::Min),
            "max" => Self::Numeric(numeric::Operation::Max),
            "absmin" => Self::Numeric(numeric::Operation::AbsMin),
            "absmax" => Self::Numeric(numeric::Operation::AbsMax),
            "range" => Self::Numeric(numeric::Operation::Range),
            "mean" => Self::Numeric(numeric::Operation::Mean),
            "geomean" => Self::Numeric(numeric::Operation::GeoMean),
            "harmmean" => Self::Numeric(numeric::Operation::HarmMean),
            "pvar" => Self::Numeric(numeric::Operation::PVar),
            "svar" => Self::Numeric(numeric::Operation::SVar),
            "pstdev" => Self::Numeric(numeric::Operation::PStdev),
            "sstdev" => Self::Numeric(numeric::Operation::SStdev),
            "pskew" => Self::Numeric(numeric::Operation::PSkew),
            "sskew" => Self::Numeric(numeric::Operation::SSkew),
            "pkurt" => Self::Numeric(numeric::Operation::PKurt),
            "skurt" => Self::Numeric(numeric::Operation::SKurt),
            "median" => Self::Order(order::Operation::Median),
            "q1" => Self::Order(order::Operation::Q1),
            "q3" => Self::Order(order::Operation::Q3),
            "iqr" => Self::Order(order::Operation::Iqr),
            "mad" => Self::Order(order::Operation::Mad),
            "madraw" => Self::Order(order::Operation::MadRaw),
            "count" => Self::Text(text::Operation::Count),
            "first" => Self::Text(text::Operation::First),
            "last" => Self::Text(text::Operation::Last),
            "unique" => Self::Text(text::Operation::Unique),
            "collapse" => Self::Text(text::Operation::Collapse),
            "countunique" => Self::Text(text::Operation::CountUnique),
            "mode" => Self::Text(text::Operation::Mode),
            "antimode" => Self::Text(text::Operation::Antimode),
            value if value.starts_with("perc:") => {
                let percentile = value[5..].parse::<u8>().map_err(|_| {
                    invalid(format!(
                        "invalid percentile in aggregate operation {value:?}"
                    ))
                })?;
                if percentile > 100 {
                    return Err(invalid(format!(
                        "percentile is outside 0..=100: {percentile}"
                    )));
                }
                Self::Order(order::Operation::Percentile(percentile))
            }
            _ => return Ok(None),
        };
        Ok(Some(kind))
    }

    fn name(self) -> String {
        match self {
            Self::Numeric(operation) => operation.name().to_owned(),
            Self::Order(operation) => operation.name(),
            Self::Text(operation) => operation.name().to_owned(),
        }
    }

    fn numeric(self) -> bool {
        matches!(self, Self::Numeric(_) | Self::Order(_))
    }

    fn state(self) -> Accumulator {
        match self {
            Self::Numeric(operation) => Accumulator::Numeric(numeric::State::new(operation)),
            Self::Order(operation) => Accumulator::Order(order::State::new(operation)),
            Self::Text(operation) => Accumulator::Text(text::State::new(operation)),
        }
    }
}

struct Request {
    field: String,
    kind: Kind,
    alias: Option<String>,
}

impl Request {
    fn parse(input: &str) -> Result<Self> {
        let (body, alias) = split_alias(input)?;
        let positions = separators(body, b':')?;
        for position in positions.into_iter().rev() {
            let operation = &body[position + 1..];
            if let Some(kind) = Kind::candidate(operation)? {
                let field = &body[..position];
                if field.is_empty() {
                    return Err(invalid("aggregate field is empty"));
                }
                return Ok(Self {
                    field: field.to_owned(),
                    kind,
                    alias: alias.map(str::to_owned),
                });
            }
        }
        Err(invalid(format!(
            "aggregate must be FIELD:OPERATION[=ALIAS]: {input:?}"
        )))
    }
}

fn split_alias(input: &str) -> Result<(&str, Option<&str>)> {
    let positions = separators(input, b'=')?;
    if positions.len() > 1 {
        return Err(invalid("aggregate contains more than one alias separator"));
    }
    let Some(position) = positions.first().copied() else {
        return Ok((input, None));
    };
    let alias = &input[position + 1..];
    if alias.is_empty() {
        return Err(invalid("aggregate alias is empty"));
    }
    Ok((&input[..position], Some(alias)))
}

fn separators(input: &str, separator: u8) -> Result<Vec<usize>> {
    let bytes = input.as_bytes();
    let mut positions = Vec::new();
    let mut braced = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'$' if !braced && bytes.get(index + 1) == Some(&b'{') => {
                braced = true;
                index += 1;
            }
            b'}' if braced => braced = false,
            value if !braced && value == separator => positions.push(index),
            _ => {}
        }
        index += 1;
    }
    if braced {
        return Err(invalid("aggregate field has an unclosed ${...} wrapper"));
    }
    Ok(positions)
}

struct Aggregate {
    field: usize,
    kind: Kind,
    numeric_slot: Option<usize>,
}

struct NumericInput {
    field: usize,
    operation: String,
}

pub(crate) struct Plan {
    group_fields: Vec<usize>,
    aggregates: Vec<Aggregate>,
    numeric_inputs: Vec<NumericInput>,
    output_header: Option<Vec<Vec<u8>>>,
    collapse_delimiter: Vec<u8>,
}

impl Plan {
    pub(crate) fn compile(
        group: Option<&str>,
        aggregate_specs: &[String],
        width: usize,
        header: Option<&[Vec<u8>]>,
        collapse_delimiter: &str,
    ) -> Result<Self> {
        if aggregate_specs.is_empty() {
            return Err(invalid("at least one aggregate is required"));
        }
        let group_fields = group.map_or_else(
            || Ok(Vec::new()),
            |group| {
                FieldSpec::parse(group)
                    .and_then(|spec| spec.resolve(width, header, false))
                    .map(|plan| plan.indices().to_vec())
            },
        )?;
        reject_repeated(&group_fields, "group")?;
        let requests = aggregate_specs
            .iter()
            .map(|spec| Request::parse(spec))
            .collect::<Result<Vec<_>>>()?;
        let mut aggregates = Vec::with_capacity(requests.len());
        let mut numeric_inputs = Vec::new();
        let mut numeric_slots = HashMap::new();
        for request in &requests {
            let fields = FieldSpec::parse(&request.field)?.resolve(width, header, false)?;
            if fields.len() != 1 {
                return Err(invalid(format!(
                    "aggregate field {:?} resolves to {} fields; expected one",
                    request.field,
                    fields.len()
                )));
            }
            let field = fields.indices()[0];
            let numeric_slot = if request.kind.numeric() {
                Some(if let Some(slot) = numeric_slots.get(&field) {
                    *slot
                } else {
                    let slot = numeric_inputs.len();
                    numeric_inputs.push(NumericInput {
                        field,
                        operation: request.kind.name(),
                    });
                    numeric_slots.insert(field, slot);
                    slot
                })
            } else {
                None
            };
            aggregates.push(Aggregate {
                field,
                kind: request.kind,
                numeric_slot,
            });
        }
        let output_header = build_header(header, &group_fields, &aggregates, &requests)?;
        Ok(Self {
            group_fields,
            aggregates,
            numeric_inputs,
            output_header,
            collapse_delimiter: collapse_delimiter.as_bytes().to_vec(),
        })
    }

    pub(crate) fn key(&self, record: &Record) -> Key {
        Key(self
            .group_fields
            .iter()
            .map(|field| record.fields[*field].clone())
            .collect())
    }

    pub(crate) fn key_matches(&self, key: &Key, record: &Record) -> bool {
        key.0
            .iter()
            .zip(&self.group_fields)
            .all(|(value, field)| value.as_slice() == record.fields[*field].as_slice())
            && key.0.len() == self.group_fields.len()
    }

    pub(crate) fn new_group(&self) -> Group {
        Group {
            states: self
                .aggregates
                .iter()
                .map(|aggregate| aggregate.kind.state())
                .collect(),
        }
    }

    pub(crate) fn new_scratch(&self) -> Scratch {
        Scratch {
            numeric_values: vec![None; self.numeric_inputs.len()],
        }
    }

    pub(crate) fn push(
        &self,
        group: &mut Group,
        record: &Record,
        ignore_non_numeric: bool,
        scratch: &mut Scratch,
    ) -> Result<u64> {
        let mut ignored = 0u64;
        for (input, value) in self.numeric_inputs.iter().zip(&mut scratch.numeric_values) {
            *value = match parse_number(&record.fields[input.field]) {
                Some(value) => Some(value),
                None if ignore_non_numeric => {
                    ignored += 1;
                    None
                }
                None => {
                    return Err(invalid(format!(
                        "record {}, line {}, byte {}, field {} for {}: expected a finite number, got {:?}",
                        record.number,
                        record.line,
                        record.offset,
                        input.field + 1,
                        input.operation,
                        String::from_utf8_lossy(&record.fields[input.field])
                    )));
                }
            };
        }
        for (aggregate, state) in self.aggregates.iter().zip(&mut group.states) {
            if let Some(slot) = aggregate.numeric_slot {
                if let Some(value) = scratch.numeric_values[slot] {
                    state.push_number(value)?;
                }
            } else {
                state.push_text(&record.fields[aggregate.field])?;
            }
        }
        Ok(ignored)
    }

    pub(crate) fn finish(&self, group: Group) -> Vec<Vec<u8>> {
        group
            .states
            .into_iter()
            .map(|state| state.finish(&self.collapse_delimiter))
            .collect()
    }

    pub(crate) fn output_header(&self) -> Option<&[Vec<u8>]> {
        self.output_header.as_deref()
    }

    pub(crate) fn aggregate_count(&self) -> usize {
        self.aggregates.len()
    }
}

fn parse_number(value: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(value).ok()?;
    let number = text.trim().parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

fn build_header(
    header: Option<&[Vec<u8>]>,
    group_fields: &[usize],
    aggregates: &[Aggregate],
    requests: &[Request],
) -> Result<Option<Vec<Vec<u8>>>> {
    let Some(header) = header else {
        return Ok(None);
    };
    let mut output = group_fields
        .iter()
        .map(|field| header[*field].clone())
        .collect::<Vec<_>>();
    for (aggregate, request) in aggregates.iter().zip(requests) {
        let label = if let Some(alias) = &request.alias {
            alias.as_bytes().to_vec()
        } else {
            let field = std::str::from_utf8(&header[aggregate.field]).map_err(|_| {
                invalid(format!(
                    "header field {} is not valid UTF-8; give the aggregate an alias",
                    aggregate.field + 1
                ))
            })?;
            format!("{}({field})", aggregate.kind.name()).into_bytes()
        };
        output.push(label);
    }
    let mut used = HashSet::new();
    if let Some(name) = output.iter().find(|name| !used.insert(name.as_slice())) {
        return Err(invalid(format!(
            "groupby output header {:?} is duplicated",
            String::from_utf8_lossy(name)
        )));
    }
    Ok(Some(output))
}

fn reject_repeated(indices: &[usize], purpose: &str) -> Result<()> {
    let mut seen = HashSet::new();
    if let Some(index) = indices.iter().find(|index| !seen.insert(**index)) {
        return Err(invalid(format!(
            "{purpose} fields repeat field {}",
            index + 1
        )));
    }
    Ok(())
}

enum Accumulator {
    Numeric(numeric::State),
    Order(order::State),
    Text(text::State),
}

impl Accumulator {
    fn push_number(&mut self, value: f64) -> Result<()> {
        match self {
            Self::Numeric(state) => state.push(value),
            Self::Order(state) => state.push(value),
            Self::Text(_) => return Err(internal("numeric value reached a text accumulator")),
        }
        Ok(())
    }

    fn push_text(&mut self, value: &[u8]) -> Result<()> {
        match self {
            Self::Text(state) => state.push(value),
            Self::Numeric(_) | Self::Order(_) => {
                return Err(internal("text value reached a numeric accumulator"));
            }
        }
        Ok(())
    }

    fn finish(self, delimiter: &[u8]) -> Vec<u8> {
        match self {
            Self::Numeric(state) => state.finish(),
            Self::Order(state) => state.finish(),
            Self::Text(state) => state.finish(delimiter),
        }
    }
}

pub(crate) struct Group {
    states: Vec<Accumulator>,
}

pub(crate) struct Scratch {
    numeric_values: Vec<Option<f64>>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Key(Vec<Vec<u8>>);

impl Key {
    pub(crate) fn fields(&self) -> &[Vec<u8>] {
        &self.0
    }
}

fn invalid(message: impl Into<String>) -> RsomicsError {
    RsomicsError::InvalidInput(message.into())
}

fn internal(message: impl Into<String>) -> RsomicsError {
    RsomicsError::ConfigError(message.into())
}
