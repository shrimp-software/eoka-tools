use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn default_profile_dir() -> Option<PathBuf> {
    let root_var = if cfg!(target_os = "windows") {
        "LOCALAPPDATA"
    } else {
        "HOME"
    };
    let root = PathBuf::from(std::env::var_os(root_var)?);
    default_profile_dir_in(&root, std::env::consts::OS)
}

fn default_profile_dir_in(root: &Path, platform: &str) -> Option<PathBuf> {
    let candidates: &[&str] = match platform {
        "linux" => &[
            ".config/google-chrome",
            ".config/chromium",
            ".config/microsoft-edge",
            ".var/app/com.google.Chrome/config/google-chrome",
        ],
        "macos" => &[
            "Library/Application Support/Google/Chrome",
            "Library/Application Support/Chromium",
            "Library/Application Support/Microsoft Edge",
        ],
        "windows" => return Some(root.join("Google").join("Chrome").join("User Data")),
        _ => return None,
    };
    candidates
        .iter()
        .map(|candidate| root.join(candidate))
        .find(|path| path.exists())
}

pub fn clone_profile_dir(src: &Path) -> io::Result<PathBuf> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dst = std::env::temp_dir().join(format!("eoka-profile-clone-{}-{}", std::process::id(), n));
    fs::create_dir_all(&dst)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dst, fs::Permissions::from_mode(0o700));
    }
    copy_recursive(src, &dst)?;
    Ok(dst)
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        "SingletonSocket"
            | "SingletonCookie"
            | "SingletonLock"
            | "Crashpad"
            | "Crash Reports"
            | "GrShaderCache"
            | "ShaderCache"
            | "GraphiteDawnCache"
            | "component_crx_cache"
            | "Service Worker"
            | "Code Cache"
            | "CacheStorage"
            | "Cache"
            | "Extension Rules"
            | "Extension State"
    )
}

fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if should_skip(&name_str) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            fs::create_dir_all(&to)?;
            copy_recursive(&from, &to)?;
        } else if ft.is_file() {
            if let Err(e) = fs::copy(&from, &to) {
                eprintln!("[eoka] skipping {}: {}", from.display(), e);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("eoka-profile-test-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    #[cfg(unix)]
    fn clone_is_owner_only() {
        let src = temp_root("src");
        let profile = src.join("google-chrome");
        fs::create_dir_all(profile.join("Default")).unwrap();
        fs::write(profile.join("Default").join("Cookies"), "cookie-bytes").unwrap();

        let dst = clone_profile_dir(&profile).unwrap();
        let mode = fs::metadata(&dst).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "clone dir holds the cookie store: must be 0700"
        );
        assert!(dst.join("Default").join("Cookies").exists());
        fs::remove_dir_all(&dst).unwrap();
        fs::remove_dir_all(&src).unwrap();
    }

    #[test]
    fn default_profile_dir_finds_flatpak_chrome() {
        let root = temp_root("flatpak");
        let flatpak = root.join(".var/app/com.google.Chrome/config/google-chrome");
        fs::create_dir_all(&flatpak).unwrap();
        assert_eq!(default_profile_dir_in(&root, "linux"), Some(flatpak));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn default_profile_dir_finds_plain_chrome_before_flatpak() {
        let root = temp_root("both");
        let plain = root.join(".config/google-chrome");
        let flatpak = root.join(".var/app/com.google.Chrome/config/google-chrome");
        fs::create_dir_all(&plain).unwrap();
        fs::create_dir_all(&flatpak).unwrap();
        assert_eq!(default_profile_dir_in(&root, "linux"), Some(plain));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn default_profile_dir_preserves_macos_precedence() {
        let root = temp_root("macos");
        for candidate in [
            "Library/Application Support/Microsoft Edge",
            "Library/Application Support/Chromium",
            "Library/Application Support/Google/Chrome",
        ] {
            let profile = root.join(candidate);
            fs::create_dir_all(&profile).unwrap();
            assert_eq!(default_profile_dir_in(&root, "macos"), Some(profile));
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn default_profile_dir_uses_windows_local_app_data() {
        let root = Path::new("local-app-data");
        assert_eq!(
            default_profile_dir_in(root, "windows"),
            Some(root.join("Google").join("Chrome").join("User Data"))
        );
    }

    #[test]
    fn default_profile_dir_none_without_profiles() {
        let root = temp_root("empty");
        for platform in ["linux", "macos", "unsupported"] {
            assert_eq!(default_profile_dir_in(&root, platform), None);
        }
        fs::remove_dir_all(&root).unwrap();
    }
}
