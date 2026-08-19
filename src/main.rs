#![forbid(unsafe_code)]

mod cli;
mod dialect;
mod expression;
mod fields;
mod io;
mod operations;

use std::io::Write;

use rsomics_common::{Result, RsomicsError, ToolMeta, Validation, run, run_validation};

const META: ToolMeta = ToolMeta {
    name: "rsomics-table",
    version: env!("CARGO_PKG_VERSION"),
};

fn main() -> std::process::ExitCode {
    let cli = rsomics_help::parse::<cli::Cli>();
    let output = cli.output;
    match cli.command {
        cli::Command::Validate(arguments) => {
            let json = output.json;
            run_validation(&output, META, || {
                let result = operations::validate::run(&arguments)?;
                if !json && let Validation::Valid(report) = &result {
                    emit_valid_summary(report.records, report.fields)?;
                }
                Ok(result)
            })
        }
        cli::Command::Select(arguments) => {
            let json = output.json;
            run(&output, META, || operations::select::run(&arguments, json))
        }
        cli::Command::Filter(arguments) => {
            let json = output.json;
            run(&output, META, || operations::filter::run(&arguments, json))
        }
    }
}

fn emit_valid_summary(records: u64, fields: u64) -> Result<()> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    writeln!(
        stdout,
        "valid: {} {}, {} {}",
        records,
        plural(records, "record", "records"),
        fields,
        plural(fields, "field", "fields")
    )
    .map_err(RsomicsError::Io)?;
    stdout.flush().map_err(RsomicsError::Io)
}

fn plural(value: u64, singular: &'static str, plural: &'static str) -> &'static str {
    if value == 1 { singular } else { plural }
}
