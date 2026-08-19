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
}

#[derive(Debug, Args)]
pub(crate) struct ValidateArgs {
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

    /// Require every field to be valid UTF-8.
    #[arg(long)]
    pub(crate) utf8: bool,

    /// Maximum number of recoverable structural errors to report.
    #[arg(long, default_value = "1")]
    pub(crate) max_errors: NonZeroUsize,
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
    fn help_exposes_only_validate() {
        let error =
            rsomics_help::try_parse_from::<Cli, _, _>(["rsomics-table", "--help"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DisplayHelp);
        let help = error.to_string();
        assert!(help.contains("validate"), "{help}");
        for absent in ["select", "filter", "sort", "join", "groupby"] {
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
