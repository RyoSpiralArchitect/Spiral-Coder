//! Test-only binary. Sample the exact fixture, then run the real candidate.
use std::{
    env,
    process::{self, Command, Stdio},
};

fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if let [command, option, directory] = args.as_slice() {
        if command == "coupled-status" && option == "--run-dir" {
            let _ = Command::new(env::var_os("SPIRAL_FIXTURE_PYTHON").unwrap())
                .args(["-S", "scripts/status_fixture_observe.py", "--snapshot"])
                .arg(directory)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let result = Command::new(env::var_os("SPIRAL_FIXTURE_CLI").unwrap())
        .args(&args)
        .status()
        .unwrap();
    process::exit(result.code().unwrap_or(1));
}
