//! Dev-only: runs a program on the reference evaluator.
//! Usage: `cargo run -p entropy-reference -- <file> [--seed N] [--rate R] [--fuel N]`

use std::io::{self, IsTerminal};
use std::process::ExitCode;

use entropy_reference::{Error, run};
use entropy_runtime::{Halt, Runtime};
use entropy_syntax::diagnostic::render;

const USAGE: &str = "usage: entropy-reference <file> [--seed N] [--rate R] [--fuel N]";

struct Args {
    file: String,
    seed: Option<u64>,
    rate: f32,
    fuel: Option<u64>,
}

fn parse_args() -> Option<Args> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        file: String::new(),
        seed: None,
        rate: 2.0,
        fuel: None,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => parsed.seed = Some(args.next()?.parse().ok()?),
            "--rate" => parsed.rate = args.next()?.parse().ok()?,
            "--fuel" => parsed.fuel = Some(args.next()?.parse().ok()?),
            _ if parsed.file.is_empty() && !arg.starts_with('-') => parsed.file = arg,
            _ => return None,
        }
    }
    let rate_ok = parsed.rate == 0.0 || (0.001..=100.0).contains(&parsed.rate);
    (!parsed.file.is_empty() && rate_ok).then_some(parsed)
}

fn main() -> ExitCode {
    let Some(args) = parse_args() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let src = match std::fs::read_to_string(&args.file) {
        Ok(src) => src,
        Err(e) => {
            eprintln!("entropy-reference: can't read {}: {e}", args.file);
            return ExitCode::from(2);
        }
    };
    let (hir, diagnostics) = entropy_sema::analyze(&src);
    let color = io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    eprint!("{}", render(&diagnostics, &args.file, &src, color));
    let Some(hir) = hir else {
        return ExitCode::from(1);
    };

    let mut rt = Runtime::new(args.rate, args.seed, args.fuel, io::stdout().lock());
    let result = run(&hir, &mut rt);
    // Flush what was printed, even when the program stopped early.
    let flushed = rt.flush();
    let result = result.and_then(|()| flushed.map_err(Error::Halt));
    match result {
        Ok(()) | Err(Error::Halt(Halt::OutputClosed)) => ExitCode::SUCCESS,
        Err(Error::Halt(Halt::OutOfFuel)) => {
            eprintln!("entropy: out of fuel");
            ExitCode::from(4)
        }
        Err(Error::Externs) => {
            eprintln!("entropy-reference: programs with externs aren't supported");
            ExitCode::from(1)
        }
        Err(Error::Internal(what)) => {
            eprintln!("entropy-reference: internal error: {what}");
            ExitCode::from(101)
        }
    }
}
