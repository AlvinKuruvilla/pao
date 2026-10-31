//! Where a command's output goes: through a pager when stdout is a terminal,
//! with colour decided once for the real stdout.

use std::env;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::process::{self, Child, Stdio};

use anstream::{AutoStream, ColorChoice};
use anyhow::Context;
use clap::ValueEnum;

/// When to colour output.
#[derive(Clone, Copy, ValueEnum)]
pub enum Color {
    Auto,
    Always,
    Never,
}

/// Runs `write` against the command's output and finishes the output.
///
/// The order of the finishing steps matters: the buffer is flushed, then the
/// pager's pipe is closed, which is what tells the pager the output is
/// complete, and only then is the pager waited for. A reader that quits early
/// (`q` in the pager, or `| head`) is a normal exit, not an error.
pub fn with_output(
    color: Color,
    no_pager: bool,
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> anyhow::Result<()> {
    // Colour is decided against the real stdout before a pager takes its
    // place: the pager's pipe is never a terminal, so deciding later would
    // always strip colour that the pager can show.
    let choice = match color {
        Color::Auto => AutoStream::choice(&io::stdout()),
        Color::Always => ColorChoice::AlwaysAnsi,
        Color::Never => ColorChoice::Never,
    };
    let mut pager = if no_pager { None } else { start_pager() };
    let sink: Box<dyn Write> = match pager.as_mut().and_then(|child| child.stdin.take()) {
        Some(stdin) => Box::new(stdin),
        None => Box::new(io::stdout().lock()),
    };

    // Buffered: an unbuffered write per line is slow on output with
    // thousands of lines.
    let mut out = BufWriter::new(AutoStream::new(sink, choice));
    let result = write(&mut out).and_then(|()| out.flush());
    drop(out);
    if let Some(mut child) = pager {
        child.wait().context("wait for the pager to exit")?;
    }
    match result {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result.context("write the output"),
    }
}

/// Starts a pager the way git does, if stdout is a terminal: `$PAGER`, or
/// `less` when it is unset, with `LESS=FR` unless `LESS` is already set.
/// `F` exits at once when the output fits on one screen, and `R` shows
/// colours.
///
/// git's default is `FRX`. `X` keeps the output on screen after exiting, but
/// it also stops `less` repainting, so every jump past a screenful (`G`, a
/// `/` search) prints `...skipping...`, and searching thousands of rows is
/// how `inspect` output gets read. git adds `X` because `less` before 530
/// (2017) blanked output that `F` quit on at once; anyone on such a `less`
/// can set `LESS=FRX`.
///
/// `None` means write to stdout directly: it is not a terminal, `$PAGER` is
/// empty or `cat`, or the pager could not be started. `$PAGER` is split on
/// whitespace, not parsed by a shell, so quoted arguments are not supported.
fn start_pager() -> Option<Child> {
    if !io::stdout().is_terminal() {
        return None;
    }
    let pager = env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    let mut words = pager.split_whitespace();
    let program = words.next()?;
    if program == "cat" {
        return None;
    }
    let mut command = process::Command::new(program);
    command.args(words).stdin(Stdio::piped());
    if env::var_os("LESS").is_none() {
        command.env("LESS", "FR");
    }
    command.spawn().ok()
}
