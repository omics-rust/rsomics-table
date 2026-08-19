use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Count,
    First,
    Last,
    Unique,
    Collapse,
    CountUnique,
    Mode,
    Antimode,
}

impl Operation {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::First => "first",
            Self::Last => "last",
            Self::Unique => "unique",
            Self::Collapse => "collapse",
            Self::CountUnique => "countunique",
            Self::Mode => "mode",
            Self::Antimode => "antimode",
        }
    }
}

pub(crate) enum State {
    Count(u64),
    First(Option<Vec<u8>>),
    Last(Option<Vec<u8>>),
    Unique(BTreeSet<Vec<u8>>),
    Collapse(Vec<Vec<u8>>),
    CountUnique(BTreeSet<Vec<u8>>),
    Frequencies {
        operation: Operation,
        values: BTreeMap<Vec<u8>, u64>,
    },
}

impl State {
    pub(crate) fn new(operation: Operation) -> Self {
        match operation {
            Operation::Count => Self::Count(0),
            Operation::First => Self::First(None),
            Operation::Last => Self::Last(None),
            Operation::Unique => Self::Unique(BTreeSet::new()),
            Operation::Collapse => Self::Collapse(Vec::new()),
            Operation::CountUnique => Self::CountUnique(BTreeSet::new()),
            Operation::Mode | Operation::Antimode => Self::Frequencies {
                operation,
                values: BTreeMap::new(),
            },
        }
    }

    pub(crate) fn push(&mut self, value: &[u8]) {
        match self {
            Self::Count(count) => *count += 1,
            Self::First(first) => {
                if first.is_none() {
                    *first = Some(value.to_vec());
                }
            }
            Self::Last(last) => match last {
                Some(last) => {
                    last.clear();
                    last.extend_from_slice(value);
                }
                None => *last = Some(value.to_vec()),
            },
            Self::Unique(values) | Self::CountUnique(values) => {
                values.insert(value.to_vec());
            }
            Self::Frequencies { values, .. } => {
                if let Some(count) = values.get_mut(value) {
                    *count += 1;
                } else {
                    values.insert(value.to_vec(), 1);
                }
            }
            Self::Collapse(values) => values.push(value.to_vec()),
        }
    }

    pub(crate) fn finish(self, delimiter: &[u8]) -> Vec<u8> {
        match self {
            Self::Count(count) => count.to_string().into_bytes(),
            Self::First(value) | Self::Last(value) => value.unwrap_or_default(),
            Self::Unique(values) => join(values, delimiter),
            Self::Collapse(values) => join(values, delimiter),
            Self::CountUnique(values) => values.len().to_string().into_bytes(),
            Self::Frequencies { operation, values } => mode(values, operation),
        }
    }
}

fn join(values: impl IntoIterator<Item = Vec<u8>>, delimiter: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            output.extend_from_slice(delimiter);
        }
        output.extend_from_slice(&value);
    }
    output
}

fn mode(values: BTreeMap<Vec<u8>, u64>, operation: Operation) -> Vec<u8> {
    let mut selected = None;
    for (value, count) in values {
        let replace = selected
            .as_ref()
            .is_none_or(|(_, selected_count)| match operation {
                Operation::Mode => count > *selected_count,
                Operation::Antimode => count < *selected_count,
                _ => false,
            });
        if replace {
            selected = Some((value, count));
        }
    }
    selected.map_or_else(Vec::new, |(value, _)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_ties_choose_the_smallest_bytes() {
        let mut mode = State::new(Operation::Mode);
        let mut antimode = State::new(Operation::Antimode);
        for value in [b"b".as_slice(), b"a", b"b", b"c", b"a"] {
            mode.push(value);
            antimode.push(value);
        }
        assert_eq!(mode.finish(b","), b"a");
        assert_eq!(antimode.finish(b","), b"c");
    }

    #[test]
    fn unique_is_sorted_and_collapse_is_stable() {
        let mut unique = State::new(Operation::Unique);
        let mut collapse = State::new(Operation::Collapse);
        for value in [b"b".as_slice(), b"a", b"b"] {
            unique.push(value);
            collapse.push(value);
        }
        assert_eq!(unique.finish(b"|"), b"a|b");
        assert_eq!(collapse.finish(b"|"), b"b|a|b");
    }
}
