//! Dev-only: runs a program on the reference evaluator.
//! Usage: `cargo run -p entropy-reference -- <file> [--seed N] [--rate R] [--fuel N]`

use std::io::{self, IsTerminal};
use std::process::ExitCode;

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
    let color = io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let (code, stderr) = entropy_reference::run_source(
        &src,
        &args.file,
        args.rate,
        args.seed,
        args.fuel,
        color,
        io::stdout().lock(),
    );
    eprint!("{stderr}");
    ExitCode::from(code)
}
