use serde_json::Value;
use std::{
    env, fs,
    io::{self, BufRead, Write},
};
use worldrunner_compat_content::{AdmissionPolicy, ImportedScene, materialize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 4 {
        return Err("usage: materialize GRAPH_JSON ADMISSION_JSON OUTPUT_JSON; native requests on stdout, unwrapped response data on stdin".into());
    }
    let graph: ImportedScene = serde_json::from_slice(&fs::read(&args[1])?)?;
    let policy: AdmissionPolicy = serde_json::from_slice(&fs::read(&args[2])?)?;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let result = materialize(&graph, &policy, |request| -> Result<Value, String> {
        serde_json::to_writer(&mut output, &request).map_err(|e| e.to_string())?;
        writeln!(output)
            .and_then(|()| output.flush())
            .map_err(|e| e.to_string())?;
        let mut line = String::new();
        input.read_line(&mut line).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if let Some(error) = value.get("$transport_error") {
            return Err(error.to_string());
        }
        Ok(value)
    })?;
    fs::write(&args[3], serde_json::to_vec_pretty(&result)?)?;
    Ok(())
}
