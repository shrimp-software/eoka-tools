use eoka_server::{runtime, state::AppState};
use tokio::io::{self, BufReader};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(runtime::serve(
        BufReader::new(io::stdin()),
        io::stdout(),
        AppState::new(),
    ));
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    result
}
