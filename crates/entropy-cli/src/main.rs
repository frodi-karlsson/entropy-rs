use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use entropy_syntax::diagnostic::{Diagnostic, Severity, render};

/// A compiler for Entropy, the language where data decays as it is used.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a program for errors without running it.
    Check { file: PathBuf },
    /// Print a program's syntax tree or typed HIR.
    Dump { what: Dump, file: PathBuf },
}

#[derive(Clone, Copy, ValueEnum)]
enum Dump {
    Ast,
    Hir,
}

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|info| {
        eprintln!(
            "internal compiler error, please report it at {}/issues",
            env!("CARGO_PKG_REPOSITORY")
        );
        eprintln!("{info}");
        std::process::exit(101);
    }));
    let cli = Cli::parse();
    match cli.command {
        Command::Check { file } => check(&file),
        Command::Dump { what, file } => dump(what, &file),
    }
}

fn read(file: &Path) -> Result<String, ExitCode> {
    std::fs::read_to_string(file).map_err(|e| {
        eprintln!("entropy: can't read {}: {e}", file.display());
        ExitCode::from(2)
    })
}

/// Prints diagnostics to stderr. Returns whether any is an error.
fn report(diagnostics: &[Diagnostic], file: &Path, src: &str) -> bool {
    let color = io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    eprint!(
        "{}",
        render(diagnostics, &file.display().to_string(), src, color)
    );
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}

fn check(file: &Path) -> ExitCode {
    let src = match read(file) {
        Ok(src) => src,
        Err(code) => return code,
    };
    let (_, diagnostics) = entropy_sema::analyze(&src);
    if report(&diagnostics, file, &src) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn dump(what: Dump, file: &Path) -> ExitCode {
    let src = match read(file) {
        Ok(src) => src,
        Err(code) => return code,
    };
    match what {
        Dump::Ast => {
            let (program, diagnostics) = entropy_syntax::parser::parse(&src);
            if report(&diagnostics, file, &src) {
                return ExitCode::from(1);
            }
            println!("{program:#?}");
        }
        Dump::Hir => {
            let (hir, diagnostics) = entropy_sema::analyze(&src);
            report(&diagnostics, file, &src);
            let Some(hir) = hir else {
                return ExitCode::from(1);
            };
            println!("{hir:#?}");
        }
    }
    ExitCode::SUCCESS
}
