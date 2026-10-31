use std::path::PathBuf;

use anyhow::bail;
use clap::{Parser, Subcommand};

/// Inspect and extract Bun single-file executables.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the embedded module table without writing anything.
    Inspect {
        /// The executable produced by `bun build --compile`.
        binary: PathBuf,
    },
    /// Write every embedded module to a directory.
    Extract {
        /// The executable produced by `bun build --compile`.
        binary: PathBuf,
        /// Directory to write modules into.
        #[arg(short, long, default_value = "extracted")]
        output: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        // TODO(alvin): wire these up once pao-core can decode a module table.
        Command::Inspect { .. } | Command::Extract { .. } => {
            bail!("not implemented yet: pao-core has no parser")
        }
    }
}
