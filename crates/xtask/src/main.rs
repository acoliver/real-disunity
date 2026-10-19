use std::process::ExitCode;

fn main() -> ExitCode {
    xtask::run(&std::env::args().skip(1).collect::<Vec<_>>())
}
