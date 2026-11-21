use clap::Parser;
use std::path::PathBuf;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "eoka-runner")]
#[command(about = "Config-based browser automation")]
#[command(version)]
struct Cli {
    #[arg(help = "Config file to run")]
    config: PathBuf,

    #[arg(long, help = "Run in headless mode (overrides config)")]
    headless: bool,

    #[arg(
        short = 'P',
        long = "param",
        value_name = "KEY=VALUE",
        help = "Set a parameter (can be used multiple times)"
    )]
    params: Vec<String>,

    #[arg(short, long, action = clap::ArgAction::Count, help = "Verbose output (-v for info, -vv for debug)")]
    verbose: u8,

    #[arg(long, help = "Validate config without running")]
    check: bool,

    #[arg(short, long, help = "Quiet mode (only errors)")]
    quiet: bool,
}

#[tokio::main]
async fn main() -> eoka_runner::Result<()> {
    let cli = Cli::parse();

    let level = if cli.quiet {
        Level::ERROR
    } else {
        match cli.verbose {
            0 => Level::WARN,
            1 => Level::INFO,
            _ => Level::DEBUG,
        }
    };

    FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    let params = eoka_runner::Params::from_args(&cli.params)?;

    let mut config = eoka_runner::Config::load_with_params(&cli.config, &params)?;

    if cli.check {
        println!("Config valid: {}", config.name);
        println!("  Target: {}", config.target.url);
        println!("  Actions: {}", config.actions.len());
        if !config.params.is_empty() {
            println!("  Parameters: {}", config.params.len());
            for (name, def) in &config.params {
                let req = if def.required { " (required)" } else { "" };
                let desc = def.description.as_deref().unwrap_or("");
                println!("    - {}{}: {}", name, req, desc);
            }
        }
        if let Some(ref success) = config.success {
            let count = success.any.as_ref().map(|v| v.len()).unwrap_or(0)
                + success.all.as_ref().map(|v| v.len()).unwrap_or(0);
            println!("  Success conditions: {}", count);
        }
        if let Some(ref on_failure) = config.on_failure {
            if let Some(ref retry) = on_failure.retry {
                println!("  Retry attempts: {}", retry.attempts);
            }
        }
        return Ok(());
    }

    if cli.headless {
        config.browser.headless = true;
    }

    println!("Running: {}", config.name);

    let base_path = cli
        .config
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let mut runner = eoka_runner::Runner::new(&config.browser).await?;
    let result = runner.run_with_base_path(&config, base_path).await?;

    println!();
    if result.success {
        println!("✓ Success");
    } else {
        println!("✗ Failed");
        if let Some(ref error) = result.error {
            println!("  Error: {}", error);
        }
    }
    println!("  Actions: {}", result.actions_executed);
    println!("  Duration: {}ms", result.duration_ms);
    if result.retries > 0 {
        println!("  Retries: {}", result.retries);
    }

    runner.close().await?;

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}
