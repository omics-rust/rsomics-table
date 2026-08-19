#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Sum,
    Min,
    Max,
    AbsMin,
    AbsMax,
    Range,
    Mean,
    GeoMean,
    HarmMean,
    PVar,
    SVar,
    PStdev,
    SStdev,
    PSkew,
    SSkew,
    PKurt,
    SKurt,
}

impl Operation {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Min => "min",
            Self::Max => "max",
            Self::AbsMin => "absmin",
            Self::AbsMax => "absmax",
            Self::Range => "range",
            Self::Mean => "mean",
            Self::GeoMean => "geomean",
            Self::HarmMean => "harmmean",
            Self::PVar => "pvar",
            Self::SVar => "svar",
            Self::PStdev => "pstdev",
            Self::SStdev => "sstdev",
            Self::PSkew => "pskew",
            Self::SSkew => "sskew",
            Self::PKurt => "pkurt",
            Self::SKurt => "skurt",
        }
    }
}

pub(crate) enum State {
    Sum(Compensated),
    Min(Option<f64>),
    Max(Option<f64>),
    AbsMin(Option<f64>),
    AbsMax(Option<f64>),
    Range {
        min: Option<f64>,
        max: Option<f64>,
    },
    Mean {
        sum: Compensated,
        count: u64,
    },
    GeoMean {
        logs: Compensated,
        count: u64,
        zero: bool,
        negative: bool,
    },
    HarmMean {
        reciprocals: Compensated,
        count: u64,
        zero: bool,
    },
    Moments {
        operation: Operation,
        state: Moments,
    },
}

impl State {
    pub(crate) fn new(operation: Operation) -> Self {
        match operation {
            Operation::Sum => Self::Sum(Compensated::default()),
            Operation::Min => Self::Min(None),
            Operation::Max => Self::Max(None),
            Operation::AbsMin => Self::AbsMin(None),
            Operation::AbsMax => Self::AbsMax(None),
            Operation::Range => Self::Range {
                min: None,
                max: None,
            },
            Operation::Mean => Self::Mean {
                sum: Compensated::default(),
                count: 0,
            },
            Operation::GeoMean => Self::GeoMean {
                logs: Compensated::default(),
                count: 0,
                zero: false,
                negative: false,
            },
            Operation::HarmMean => Self::HarmMean {
                reciprocals: Compensated::default(),
                count: 0,
                zero: false,
            },
            operation => Self::Moments {
                operation,
                state: Moments::default(),
            },
        }
    }

    pub(crate) fn push(&mut self, value: f64) {
        match self {
            Self::Sum(sum) => sum.add(value),
            Self::Min(current) => update_min(current, value),
            Self::Max(current) => update_max(current, value),
            Self::AbsMin(current) => update_min(current, value.abs()),
            Self::AbsMax(current) => update_max(current, value.abs()),
            Self::Range { min, max } => {
                update_min(min, value);
                update_max(max, value);
            }
            Self::Mean { sum, count } => {
                sum.add(value);
                *count += 1;
            }
            Self::GeoMean {
                logs,
                count,
                zero,
                negative,
            } => {
                *count += 1;
                if value < 0.0 {
                    *negative = true;
                } else if value == 0.0 {
                    *zero = true;
                } else {
                    logs.add(value.ln());
                }
            }
            Self::HarmMean {
                reciprocals,
                count,
                zero,
            } => {
                *count += 1;
                if value == 0.0 {
                    *zero = true;
                } else {
                    reciprocals.add(value.recip());
                }
            }
            Self::Moments { state, .. } => state.push(value),
        }
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        format_g14(match self {
            Self::Sum(sum) => sum.value(),
            Self::Min(value) | Self::Max(value) | Self::AbsMin(value) | Self::AbsMax(value) => {
                value.unwrap_or(f64::NAN)
            }
            Self::Range { min, max } => match (min, max) {
                (Some(min), Some(max)) => max - min,
                _ => f64::NAN,
            },
            Self::Mean { sum, count } => divide(sum.value(), count),
            Self::GeoMean {
                logs,
                count,
                zero,
                negative,
            } => {
                if count == 0 || negative {
                    f64::NAN
                } else if zero {
                    0.0
                } else {
                    (logs.value() / count as f64).exp()
                }
            }
            Self::HarmMean {
                reciprocals,
                count,
                zero,
            } => {
                if count == 0 {
                    f64::NAN
                } else if zero {
                    0.0
                } else {
                    count as f64 / reciprocals.value()
                }
            }
            Self::Moments { operation, state } => state.value(operation),
        })
        .into_bytes()
    }
}

fn update_min(current: &mut Option<f64>, value: f64) {
    *current = Some(current.map_or(value, |current| current.min(value)));
}

fn update_max(current: &mut Option<f64>, value: f64) {
    *current = Some(current.map_or(value, |current| current.max(value)));
}

fn divide(sum: f64, count: u64) -> f64 {
    if count == 0 {
        f64::NAN
    } else {
        sum / count as f64
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Compensated {
    sum: f64,
    correction: f64,
}

impl Compensated {
    fn add(&mut self, value: f64) {
        let next = self.sum + value;
        if self.sum.abs() >= value.abs() {
            self.correction += (self.sum - next) + value;
        } else {
            self.correction += (value - next) + self.sum;
        }
        self.sum = next;
    }

    fn value(self) -> f64 {
        self.sum + self.correction
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Moments {
    count: u64,
    mean: f64,
    m2: f64,
    m3: f64,
    m4: f64,
}

impl Moments {
    fn push(&mut self, value: f64) {
        if self.count == 0 {
            self.merge(Self {
                count: 1,
                mean: value,
                ..Self::default()
            });
            return;
        }
        let previous = self.count as f64;
        self.count += 1;
        let count = self.count as f64;
        let delta = value - self.mean;
        let delta_n = delta / count;
        let delta_n2 = delta_n * delta_n;
        let term = delta * delta_n * previous;
        self.m4 += term * delta_n2 * (count * count - 3.0 * count + 3.0) + 6.0 * delta_n2 * self.m2
            - 4.0 * delta_n * self.m3;
        self.m3 += term * delta_n * (count - 2.0) - 3.0 * delta_n * self.m2;
        self.m2 += term;
        self.mean += delta_n;
    }

    fn merge(&mut self, other: Self) {
        if other.count == 0 {
            return;
        }
        if self.count == 0 {
            *self = other;
            return;
        }
        let left = self.count as f64;
        let right = other.count as f64;
        let count = left + right;
        let delta = other.mean - self.mean;
        let delta2 = delta * delta;
        let delta3 = delta2 * delta;
        let delta4 = delta3 * delta;
        let merged_m2 = self.m2 + other.m2 + delta2 * left * right / count;
        let merged_m3 = self.m3
            + other.m3
            + delta3 * left * right * (left - right) / (count * count)
            + 3.0 * delta * (left * other.m2 - right * self.m2) / count;
        let merged_m4 = self.m4
            + other.m4
            + delta4 * left * right * (left * left - left * right + right * right) / count.powi(3)
            + 6.0 * delta2 * (left * left * other.m2 + right * right * self.m2) / (count * count)
            + 4.0 * delta * (left * other.m3 - right * self.m3) / count;
        self.mean += delta * right / count;
        self.count += other.count;
        self.m2 = merged_m2;
        self.m3 = merged_m3;
        self.m4 = merged_m4;
    }

    fn value(self, operation: Operation) -> f64 {
        let count = self.count as f64;
        match operation {
            Operation::PVar => {
                if self.count == 0 {
                    f64::NAN
                } else {
                    self.m2 / count
                }
            }
            Operation::SVar => {
                if self.count < 2 {
                    f64::NAN
                } else {
                    self.m2 / (count - 1.0)
                }
            }
            Operation::PStdev => {
                if self.count == 0 {
                    f64::NAN
                } else {
                    (self.m2 / count).sqrt()
                }
            }
            Operation::SStdev => {
                if self.count < 2 {
                    f64::NAN
                } else {
                    (self.m2 / (count - 1.0)).sqrt()
                }
            }
            Operation::PSkew => self.population_skew(),
            Operation::SSkew => {
                if self.count < 3 {
                    f64::NAN
                } else {
                    self.population_skew() * (count * (count - 1.0)).sqrt() / (count - 2.0)
                }
            }
            Operation::PKurt => self.population_kurtosis(),
            Operation::SKurt => {
                if self.count < 4 {
                    f64::NAN
                } else {
                    (count - 1.0) / ((count - 2.0) * (count - 3.0))
                        * ((count + 1.0) * self.population_kurtosis() + 6.0)
                }
            }
            _ => f64::NAN,
        }
    }

    fn population_skew(self) -> f64 {
        if self.count == 0 || self.m2 == 0.0 {
            f64::NAN
        } else {
            (self.count as f64).sqrt() * self.m3 / self.m2.powf(1.5)
        }
    }

    fn population_kurtosis(self) -> f64 {
        if self.count == 0 || self.m2 == 0.0 {
            f64::NAN
        } else {
            self.count as f64 * self.m4 / (self.m2 * self.m2) - 3.0
        }
    }
}

pub(crate) fn format_g14(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let scientific = format!("{value:.13e}");
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return scientific;
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return scientific;
    };
    if !(-4..14).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exponent.abs());
    }
    let decimals = (13 - exponent).max(0) as usize;
    let fixed = format!("{value:.decimals$}");
    if fixed.contains('.') {
        fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        fixed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_format_matches_datamash_contract() {
        for (value, expected) in [
            (1.0, "1"),
            (1.0 / 3.0, "0.33333333333333"),
            (3e-6, "3e-06"),
            (3e10, "30000000000"),
            (12_345_678.123_456_79, "12345678.123457"),
            (999_999_999_999_999.0, "1e+15"),
            (0.000_099_999_999_999_999_5, "0.0001"),
            (-0.0, "0"),
        ] {
            assert_eq!(format_g14(value), expected);
        }
    }

    #[test]
    fn moments_merge_matches_one_pass_state() {
        let values = [1.5, 2.0, 3.5, 2.5, 10.0, 12.0, 8.0, 9.5, 11.0];
        let mut whole = Moments::default();
        let mut left = Moments::default();
        let mut right = Moments::default();
        for value in values {
            whole.push(value);
        }
        for value in &values[..4] {
            left.push(*value);
        }
        for value in &values[4..] {
            right.push(*value);
        }
        left.merge(right);
        assert_eq!(left.count, whole.count);
        for operation in [
            Operation::PVar,
            Operation::SVar,
            Operation::PSkew,
            Operation::SSkew,
            Operation::PKurt,
            Operation::SKurt,
        ] {
            let merged = left.value(operation);
            let direct = whole.value(operation);
            assert!((merged - direct).abs() < 1e-12, "{operation:?}");
        }
    }

    #[test]
    fn empty_and_singleton_states_are_explicit() {
        assert_eq!(State::new(Operation::Sum).finish(), b"0");
        assert_eq!(State::new(Operation::Mean).finish(), b"nan");
        let mut sample = State::new(Operation::SVar);
        sample.push(5.0);
        assert_eq!(sample.finish(), b"nan");
        let mut population = State::new(Operation::PVar);
        population.push(5.0);
        assert_eq!(population.finish(), b"0");
    }

    #[test]
    fn large_offsets_keep_small_variance() {
        let mut state = State::new(Operation::PVar);
        for value in [1e12 + 1.0, 1e12 + 2.0, 1e12 + 3.0, 1e12 + 4.0] {
            state.push(value);
        }
        assert_eq!(state.finish(), b"1.25");
    }
}
