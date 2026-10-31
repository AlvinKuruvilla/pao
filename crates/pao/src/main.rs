mod commands;
mod output;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::output::Color;

/// Inspect and extract Bun single-file executables.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// When to color output. `auto` colors a terminal and honours `NO_COLOR`
    /// and `CLICOLOR`.
    #[arg(long, value_enum, default_value_t = Color::Auto, global = true)]
    color: Color,
    /// Write to stdout even when it is a terminal, instead of through `$PAGER`.
    #[arg(long, global = true)]
    no_pager: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Print the embedded module table without writing anything.
    Inspect {
        /// The executable produced by `bun build --compile`.
        binary: PathBuf,
    },
    /// Write every embedded module, and the original sources from any source
    /// maps, to a directory.
    Extract {
        /// The executable produced by `bun build --compile`.
        binary: PathBuf,
        /// Directory to write into. Defaults to `<binary name>.extracted` in
        /// the current directory.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write into an existing, non-empty directory, replacing files with
        /// the same names.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let Cli {
        command,
        color,
        no_pager,
    } = Cli::parse();
    match command {
        Command::Inspect { binary } => commands::inspect::run(&binary, color, no_pager),
        Command::Extract {
            binary,
            output,
            force,
        } => commands::extract::run(&binary, output.as_deref(), force),
    }
}
