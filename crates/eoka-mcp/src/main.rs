#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => eoka_mcp::run_server().await,
        [flag] if flag == "--version" => {
            println!("eoka-mcp {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => {
            eprintln!("Usage: eoka-mcp [--version]");
            std::process::exit(2);
        }
    }
}
