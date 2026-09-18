use everything_core::Engine;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<_> = std::env::args().collect();
    let db = args
        .get(1)
        .ok_or("Usage: everything-index DATABASE [--scan ROOT]")?;
    let engine = Engine::open(db)?;
    if args.get(2).is_some_and(|s| s == "--scan") {
        let root = args.get(3).ok_or("--scan needs an absolute path")?;
        engine.request(
            json!({"op":"configure","config":{"roots":[root],"rules":{}},"rebuild":true}),
        )?;
        loop {
            let status = engine.status();
            eprintln!("{}", serde_json::to_string(&status)?);
            if status.pending == 0 {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        return Ok(());
    }
    // One JSON request/response per line; useful for support diagnostics without
    // granting a shell access to the UI or copying the live database.
    for line in io::stdin().lock().lines() {
        let response = match serde_json::from_str(&line?) {
            Ok(request) => match engine.request(request) {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":error}),
            },
            Err(error) => json!({"error":error.to_string()}),
        };
        println!("{response}");
        io::stdout().flush()?;
    }
    Ok(())
}
