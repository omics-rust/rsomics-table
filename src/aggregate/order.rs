use super::numeric::format_g14;

const MAD_SCALE: f64 = 1.4826;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Median,
    Q1,
    Q3,
    Iqr,
    Percentile(u8),
    Mad,
    MadRaw,
}

impl Operation {
    pub(crate) fn name(self) -> String {
        match self {
            Self::Median => "median".to_owned(),
            Self::Q1 => "q1".to_owned(),
            Self::Q3 => "q3".to_owned(),
            Self::Iqr => "iqr".to_owned(),
            Self::Percentile(value) => format!("perc:{value}"),
            Self::Mad => "mad".to_owned(),
            Self::MadRaw => "madraw".to_owned(),
        }
    }
}

pub(crate) struct State {
    operation: Operation,
    values: Vec<f64>,
}

impl State {
    pub(crate) fn new(operation: Operation) -> Self {
        Self {
            operation,
            values: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, value: f64) {
        self.values.push(value);
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.values.sort_by(f64::total_cmp);
        let value = match self.operation {
            Operation::Median => quantile(&self.values, 0.5),
            Operation::Q1 => quantile(&self.values, 0.25),
            Operation::Q3 => quantile(&self.values, 0.75),
            Operation::Iqr => quantile(&self.values, 0.75) - quantile(&self.values, 0.25),
            Operation::Percentile(value) => quantile(&self.values, f64::from(value) / 100.0),
            Operation::Mad | Operation::MadRaw => {
                let median = quantile(&self.values, 0.5);
                let mut deviations = self
                    .values
                    .iter()
                    .map(|value| (value - median).abs())
                    .collect::<Vec<_>>();
                deviations.sort_by(f64::total_cmp);
                let raw = quantile(&deviations, 0.5);
                if self.operation == Operation::Mad {
                    raw * MAD_SCALE
                } else {
                    raw
                }
            }
        };
        format_g14(value).into_bytes()
    }
}

fn quantile(values: &[f64], probability: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    if values.len() == 1 {
        return values[0];
    }
    let position = probability * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let fraction = position - lower as f64;
    values[lower] + fraction * (values[(lower + 1).min(values.len() - 1)] - values[lower])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(operation: Operation, values: &[f64]) -> Vec<u8> {
        let mut state = State::new(operation);
        for value in values {
            state.push(*value);
        }
        state.finish()
    }

    #[test]
    fn quantiles_use_linear_method_seven() {
        let values = [28.0, 29.0, 30.0, 31.0, 35.0];
        assert_eq!(rendered(Operation::Median, &values), b"30");
        assert_eq!(rendered(Operation::Q1, &values), b"29");
        assert_eq!(rendered(Operation::Q3, &values), b"31");
        assert_eq!(rendered(Operation::Percentile(90), &values), b"33.4");
    }

    #[test]
    fn median_absolute_deviation_scaling_is_explicit() {
        let values = [1.0, 2.0, 8.0];
        assert_eq!(rendered(Operation::MadRaw, &values), b"1");
        assert_eq!(rendered(Operation::Mad, &values), b"1.4826");
    }

    #[test]
    fn empty_and_singleton_order_states_are_explicit() {
        assert_eq!(rendered(Operation::Median, &[]), b"nan");
        assert_eq!(rendered(Operation::Median, &[5.0]), b"5");
    }
}
