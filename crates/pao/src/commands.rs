//! One module per subcommand. Each `run` takes the subcommand's arguments and
//! carries out the whole command; `main` only parses and dispatches.

pub mod extract;
pub mod inspect;
