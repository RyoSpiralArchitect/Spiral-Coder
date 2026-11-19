// Exercise the public test suite with controlled serializations of a real CLI result.
use std::{
    env,
    fs::OpenOptions,
    io::Write,
    process::{self, Command},
};

fn main() {
    let output = Command::new(env::var_os("SPIRAL_PROBE_BINARY").unwrap())
        .args(env::args_os().skip(1))
        .output()
        .unwrap();
    std::io::stderr().write_all(&output.stderr).unwrap();
    let mut bytes = output.stdout;
    if output.status.success() {
        if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(rows) = value["iterations"]
                .as_array_mut()
                .filter(|rows| rows.len() == 2)
            {
                let mut hits = OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(env::var_os("SPIRAL_PROBE_HITS").unwrap())
                    .unwrap();
                writeln!(hits, "two rows").unwrap();
                match env::var("SPIRAL_PROBE_MODE").unwrap().as_str() {
                    "pretty" => bytes = serde_json::to_vec_pretty(&value).unwrap(),
                    "swapped_task_ids" => {
                        let first = rows[0]["task_id"].clone();
                        rows[0]["task_id"] = rows[1]["task_id"].clone();
                        rows[1]["task_id"] = first;
                        bytes = serde_json::to_vec(&value).unwrap();
                    }
                    "wrong_numbers" => {
                        rows[0]["iteration"] = 10.into();
                        rows[1]["iteration"] = 20.into();
                        bytes = serde_json::to_vec(&value).unwrap();
                    }
                    "reversed_rows" => {
                        rows.reverse();
                        bytes = serde_json::to_vec(&value).unwrap();
                    }
                    "native" => (),
                    _ => process::exit(2),
                }
            }
        }
    }
    std::io::stdout().write_all(&bytes).unwrap();
    process::exit(output.status.code().unwrap_or(1));
}
