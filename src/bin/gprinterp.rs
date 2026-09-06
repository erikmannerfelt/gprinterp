//! `gprinterp validate <FILE>` — a hand-testing harness for the spec.
//!
//! The library is the product; this exists so a document can be checked
//! without writing a scratch program, and so the examples in this repository
//! have something to be checked by.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "gprinterp",
    version,
    about = "Work with gprinterp interpretation documents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a document against the specification.
    Validate {
        /// The document to validate.
        file: PathBuf,
        /// Exit non-zero on warnings as well as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Read a document and write it back canonically, preserving unknown
    /// fields. Useful for checking that a producer's output survives a
    /// round trip (SPEC §3.3).
    Normalize {
        /// The document to normalize.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { file, strict } => {
            let text = std::fs::read_to_string(&file)?;
            let document = gprinterp::Document::from_json(&text)?;
            let report = gprinterp::validate(&document);

            for error in &report.errors {
                eprintln!("error: {error}");
            }
            for warning in &report.warnings {
                eprintln!("warning: {warning}");
            }

            let n_features = document.features.len();
            let n_layers = document.layers().len();
            println!(
                "{}: {n_features} feature(s) in {n_layers} layer(s), {} error(s), {} warning(s)",
                file.display(),
                report.errors.len(),
                report.warnings.len()
            );

            let failed = !report.is_valid() || (strict && !report.warnings.is_empty());
            Ok(if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Normalize { file } => {
            let text = std::fs::read_to_string(&file)?;
            let document = gprinterp::Document::from_json(&text)?;
            println!("{}", document.to_json_pretty()?);
            Ok(ExitCode::SUCCESS)
        }
    }
}
