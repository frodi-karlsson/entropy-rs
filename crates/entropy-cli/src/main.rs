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
    /// Compile a program in memory and run it.
    Run {
        file: PathBuf,
        /// Seed for the decay RNG. Without one, the OS picks.
        #[arg(long)]
        seed: Option<u64>,
        /// How far each decay step can move a value: 0, or from 0.001 to 100.
        #[arg(long, default_value = "2", value_parser = rate)]
        mutation_rate: f32,
        /// Stop with exit code 4 after this many decay steps.
        #[arg(long)]
        fuel: Option<u64>,
    },
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
        Command::Run {
            file,
            seed,
            mutation_rate,
            fuel,
        } => run(&file, mutation_rate, seed, fuel),
        Command::Dump { what, file } => dump(what, &file),
    }
}

fn rate(s: &str) -> Result<f32, String> {
    let r: f32 = s.parse().map_err(|_| format!("`{s}` is not a number"))?;
    if r == 0.0 || (0.001..=100.0).contains(&r) {
        Ok(r)
    } else {
        Err("must be 0, or from 0.001 to 100".to_string())
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

/// Reads and checks a program, printing any diagnostics.
fn compile(file: &Path) -> Result<entropy_sema::hir::Program, ExitCode> {
    let src = read(file)?;
    let (hir, diagnostics) = entropy_sema::analyze(&src);
    let failed = report(&diagnostics, file, &src);
    match hir {
        Some(hir) if !failed => Ok(hir),
        _ => Err(ExitCode::from(1)),
    }
}

/// Exit 3 for a program codegen can't handle yet or a failed link, 101 for our own bugs.
fn codegen_failed(e: &entropy_codegen::Error) -> ExitCode {
    eprintln!("entropy: {e}");
    match e {
        entropy_codegen::Error::Internal(_) => ExitCode::from(101),
        _ => ExitCode::from(3),
    }
}

fn run(file: &Path, rate: f32, seed: Option<u64>, fuel: Option<u64>) -> ExitCode {
    let hir = match compile(file) {
        Ok(hir) => hir,
        Err(code) => return code,
    };
    match entropy_codegen::jit::run(&hir, rate, seed, fuel) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => codegen_failed(&e),
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
