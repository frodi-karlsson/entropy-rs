use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: entropy <check|run|build|dump> <file>");
    ExitCode::from(2)
}
