use eoka::stealth::ChromePatcher;
use std::{fs, time::{Duration, UNIX_EPOCH}};

fn main() {
    let executable = std::env::temp_dir().join("invalid.app/Contents/MacOS/Chrome");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    let original = b"not a Mach-O binary webdriver";
    fs::write(&executable, original).unwrap();
    fs::File::options().write(true).open(&executable).unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1)).unwrap();
    let patcher = ChromePatcher::new(&executable).unwrap();
    let first_error = patcher.patch().is_err();
    let cache_accepted = patcher.is_patched();
    let second_error = patcher.get_patched_path().is_err();
    println!("{}", serde_json::json!({
        "firstError": first_error,
        "cacheAccepted": cache_accepted,
        "secondError": second_error,
        "originalUnchanged": fs::read(executable).unwrap() == original,
    }));
}
