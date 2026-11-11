use std::{env, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: ariang-gateway <private-config.toml>");
        return ExitCode::FAILURE;
    }
    // Configuration and transport errors can contain credentials or destinations.
    match ariang_gateway::run(&args[1]).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => {
            eprintln!(
                "gateway startup or runtime failed; check private configuration and listener"
            );
            ExitCode::FAILURE
        }
    }
}
