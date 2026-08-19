use std::num::NonZeroUsize;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use rsomics_common::OutputArgs;

#[derive(Debug, Parser)]
#[command(
    name = "rsomics-table",
    version,
    about = "Strict, high-performance CSV and TSV workflows",
    subcommand_required = true
)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub(crate) output: OutputArgs,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Validate table framing and structure.
    Validate(ValidateArgs),

    /// Select and reorder fields.
    Select(SelectArgs),

    /// Filter records with a typed expression.
    Filter(FilterArgs),
}

#[derive(Debug, Args)]
pub(crate) struct InputArgs {
    /// Input table, or - for standard input.
    #[arg(value_name = "TABLE", default_value = "-")]
    pub(crate) input: PathBuf,

    /// Read tab-delimited input.
    #[arg(long, conflicts_with = "delimiter")]
    pub(crate) tsv: bool,

    /// Input delimiter as one ASCII byte or \\t.
    #[arg(short = 'd', long, value_parser = parse_byte, default_value = ",")]
    pub(crate) delimiter: u8,

    /// Treat input as headerless.
    #[arg(long)]
    pub(crate) no_header: bool,

    /// Ignore lines beginning with this ASCII byte.
    #[arg(long, value_parser = parse_byte)]
    pub(crate) comment: Option<u8>,
}

impl InputArgs {
    pub(crate) fn resolved_delimiter(&self) -> u8 {
        if self.tsv { b'\t' } else { self.delimiter }
    }
}

#[derive(Debug, Args)]
pub(crate) struct ValidateArgs {
    #[command(flatten)]
    pub(crate) input: InputArgs,

    /// Require every field to be valid UTF-8.
    #[arg(long)]
    pub(crate) utf8: bool,

    /// Maximum number of recoverable structural errors to report.
    #[arg(long, default_value = "1")]
    pub(crate) max_errors: NonZeroUsize,
}

#[derive(Debug, Args)]
pub(crate) struct SelectArgs {
    #[command(flatten)]
    pub(crate) input: InputArgs,

    /// Fields to select, reorder, or exclude.
    #[arg(short = 'f', long, allow_hyphen_values = true)]
    pub(crate) fields: String,

    /// Match field names with csvtk-compatible fuzzy patterns.
    #[arg(short = 'F', long)]
    pub(crate) fuzzy_fields: bool,

    #[command(flatten)]
    pub(crate) output: TableOutputArgs,

    /// Omit the projected header from output.
    #[arg(long)]
    pub(crate) no_output_header: bool,
}

impl SelectArgs {
    pub(crate) fn resolved_output_delimiter(&self) -> u8 {
        self.output
            .resolved_delimiter(self.input.resolved_delimiter())
    }
}

#[derive(Debug, Args)]
pub(crate) struct FilterArgs {
    #[command(flatten)]
    pub(crate) input: InputArgs,

    /// Boolean expression used to keep records.
    #[arg(short = 'w', long = "where")]
    pub(crate) expression: String,

    /// Treat numeric field spellings as text.
    #[arg(long)]
    pub(crate) numeric_as_string: bool,

    #[command(flatten)]
    pub(crate) output: TableOutputArgs,

    /// Omit the input header from output.
    #[arg(long)]
    pub(crate) no_output_header: bool,
}

impl FilterArgs {
    pub(crate) fn resolved_output_delimiter(&self) -> u8 {
        self.output
            .resolved_delimiter(self.input.resolved_delimiter())
    }
}

#[derive(Debug, Args)]
pub(crate) struct TableOutputArgs {
    /// Output table, or - for standard output.
    #[arg(short = 'o', long, value_name = "TABLE", default_value = "-")]
    pub(crate) output: PathBuf,

    /// Output delimiter as one ASCII byte or \\t.
    #[arg(short = 'D', long, value_parser = parse_byte, conflicts_with = "output_tsv")]
    pub(crate) output_delimiter: Option<u8>,

    /// Write tab-delimited output.
    #[arg(long)]
    pub(crate) output_tsv: bool,

    /// Write gzip-compressed output.
    #[arg(long)]
    pub(crate) gzip: bool,
}

impl TableOutputArgs {
    fn resolved_delimiter(&self, input_delimiter: u8) -> u8 {
        if self.output_tsv {
            b'\t'
        } else {
            self.output_delimiter.unwrap_or(input_delimiter)
        }
    }
}

fn parse_byte(value: &str) -> Result<u8, String> {
    if value == r"\t" {
        return Ok(b'\t');
    }
    let bytes = value.as_bytes();
    if bytes.len() != 1 || !bytes[0].is_ascii() {
        return Err("expected one ASCII byte or \\t".to_owned());
    }
    Ok(bytes[0])
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    #[test]
    fn command_tree_is_valid() {
        rsomics_help::command::<Cli>().debug_assert();
    }

    #[test]
    fn help_exposes_only_completed_commands() {
        let error =
            rsomics_help::try_parse_from::<Cli, _, _>(["rsomics-table", "--help"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DisplayHelp);
        let help = error.to_string();
        assert!(help.contains("validate"), "{help}");
        assert!(help.contains("select"), "{help}");
        assert!(help.contains("filter"), "{help}");
        for absent in ["sort", "join", "groupby"] {
            assert!(!help.contains(&format!("  {absent}")), "{help}");
        }
    }

    #[test]
    fn delimiter_is_one_ascii_byte() {
        assert_eq!(parse_byte(r"\t").unwrap(), b'\t');
        assert_eq!(parse_byte("|").unwrap(), b'|');
        assert!(parse_byte("").is_err());
        assert!(parse_byte("::").is_err());
        assert!(parse_byte("，").is_err());
    }
}
