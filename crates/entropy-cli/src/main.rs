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
        /// A shared library to search for externs, before this process and libm.
        #[arg(long)]
        dylib: Vec<PathBuf>,
    },
    /// Compile a program to a native executable.
    Build {
        file: PathBuf,
        /// Where to write the output. Defaults to the file name without `.en`, plus `.o` or
        /// `.clif` for those outputs.
        #[arg(short)]
        o: Option<PathBuf>,
        /// Baked into the executable: 0, or from 0.001 to 100.
        #[arg(long, default_value = "2", value_parser = rate)]
        mutation_rate: f32,
        #[arg(long, value_enum, default_value = "exe")]
        emit: Emit,
        /// A library to link, for externs.
        #[arg(short = 'l')]
        lib: Vec<String>,
        /// A directory to search for `-l` libraries.
        #[arg(short = 'L')]
        lib_dir: Vec<PathBuf>,
    },
    /// Print a program's syntax tree or typed HIR.
    Dump { what: Dump, file: PathBuf },
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum Emit {
    Exe,
    Obj,
    Clif,
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
            dylib,
        } => run(&file, mutation_rate, seed, fuel, &dylib),
        Command::Build {
            file,
            o,
            mutation_rate,
            emit,
            lib,
            lib_dir,
        } => build(&file, o, mutation_rate, emit, &Libs { lib, lib_dir }),
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

fn run(
    file: &Path,
    rate: f32,
    seed: Option<u64>,
    fuel: Option<u64>,
    dylibs: &[PathBuf],
) -> ExitCode {
    let hir = match compile(file) {
        Ok(hir) => hir,
        Err(code) => return code,
    };
    match entropy_codegen::jit::run(&hir, rate, seed, fuel, dylibs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => codegen_failed(&e),
    }
}

/// The runtime as a static library, compiled by `build.rs`, and the system libraries it needs.
const RUNTIME_LIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libentropy_runtime.a"));
const NATIVE_LIBS: &str = include_str!(concat!(env!("OUT_DIR"), "/native-libs.txt"));

/// Libraries to link, from `-l` and `-L`.
struct Libs {
    lib: Vec<String>,
    lib_dir: Vec<PathBuf>,
}

fn build(file: &Path, out: Option<PathBuf>, rate: f32, emit: Emit, libs: &Libs) -> ExitCode {
    let hir = match compile(file) {
        Ok(hir) => hir,
        Err(code) => return code,
    };
    let object = match entropy_codegen::aot::object(&hir, rate) {
        Ok(object) => object,
        Err(e) => return codegen_failed(&e),
    };
    let out = out.unwrap_or_else(|| {
        let stem = PathBuf::from(file.file_stem().unwrap_or(file.as_os_str()));
        match emit {
            Emit::Exe => stem,
            Emit::Obj => stem.with_extension("o"),
            Emit::Clif => stem.with_extension("clif"),
        }
    });
    let written = match emit {
        Emit::Obj => std::fs::write(&out, &object.bytes),
        Emit::Clif => std::fs::write(&out, &object.clif),
        Emit::Exe => return link(&object.bytes, &out, libs),
    };
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("entropy: can't write {}: {e}", out.display());
            ExitCode::from(3)
        }
    }
}

/// Links the object with the runtime using the system C compiler.
fn link(object: &[u8], out: &Path, libs: &Libs) -> ExitCode {
    let dir = std::env::temp_dir().join(format!("entropy-build-{}", std::process::id()));
    let result = link_in(&dir, object, out, libs);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("entropy: {message}");
            ExitCode::from(3)
        }
    }
}

fn link_in(dir: &Path, object: &[u8], out: &Path, libs: &Libs) -> Result<(), String> {
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        std::fs::write(&path, bytes)
            .map(|()| path)
            .map_err(|e| format!("can't write {}: {e}", dir.display()))
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let obj = write("program.o", object)?;
    let lib = write("libentropy_runtime.a", RUNTIME_LIB)?;
    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let result = std::process::Command::new(&cc)
        .arg(obj)
        .arg(lib)
        // User libraries come before the system ones they may need, such as `-lm`.
        .args(libs.lib_dir.iter().map(|d| format!("-L{}", d.display())))
        .args(libs.lib.iter().map(|l| format!("-l{l}")))
        .args(NATIVE_LIBS.split_whitespace())
        .arg("-o")
        .arg(out)
        .output();
    let output = match result {
        Ok(output) => output,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(format!(
                "can't find the C linker `{}`. `entropy build` needs one. On macOS, run \
                 `xcode-select --install`. On Debian or Ubuntu, install `build-essential`.",
                cc.to_string_lossy()
            ));
        }
        Err(e) => return Err(format!("can't run `{}`: {e}", cc.to_string_lossy())),
    };
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "linking failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
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
