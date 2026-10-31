//! A parser for the module graph that `bun build --compile` embeds in a
//! single-file executable.
//!
//! Bun attaches the bundled program to a copy of its own runtime, either
//! appended to the file or in a section of the executable, depending on the
//! platform and release (`docs/format.md` has the table). The payload ends
//! with a fixed trailer, `"\n---- Bun! ----\n"`, preceded by an offsets
//! record that locates a table of module entries. Each entry points at a
//! module's path, its contents, and optionally a source map and
//! JavaScriptCore bytecode.
//!
//! The entry layout has changed across Bun releases and no version is
//! recorded, so [`locate::locate`] tries each known [`LayoutVersion`] and
//! keeps the one that decodes.
//!
//! This crate only reads byte slices; reading the file from disk belongs to
//! the caller, so the CLI, the Tauri backend and tests share one parser.
//!
//! [`LayoutVersion`]: layout_version::LayoutVersion

pub mod byte_reader;
pub mod entry;
pub mod flags;
pub mod layout_version;
pub mod locate;
pub mod names;
pub mod native;
pub mod offsets;
pub mod payload;
pub mod project;
pub mod sourcemap;
pub mod string_pointer;
pub mod version;
pub mod zstd_frame;
