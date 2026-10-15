fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    serde_json::to_writer(stdout.lock(), &eoka_protocol::sidecar::schema_bundle())?;
    Ok(())
}
