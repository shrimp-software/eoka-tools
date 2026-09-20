use std::env;

const SUPPORTED: &[&str] = &[
    "EOKA_TEST_CHROME_PATH",
    "EOKA_TEST_PROFILE_DIR",
    "EOKA_TEST_VISIBLE",
    "EOKA_TEST_NATIVE_LAUNCH",
    "EOKA_TEST_SOFTWARE_COMPOSITING",
    "EOKA_TEST_PROFILE_URL",
    "EOKA_TEST_PASSWORD_FILE",
    "EOKA_TEST_REQUIRE_PASSWORD_STAGE",
    "EOKA_TEST_REQUIRE_SOLVED",
    "EOKA_TEST_SUCCESS_SELECTOR",
    "EOKA_TEST_CLICK_SELECTOR",
];

pub struct Options {
    pub url: String,
    pub profile: Option<String>,
    pub require_solved: bool,
    pub success_selector: Option<String>,
    pub click_selector: Option<String>,
    chrome_path: Option<String>,
    profile_dir: Option<String>,
    visible: bool,
    native: bool,
    software_compositing: bool,
}

fn optional(name: &str) -> Result<Option<String>, &'static str> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        _ => Err("example options must contain nonempty UTF-8 values"),
    }
}

fn supported(name: &str) -> bool {
    !name.starts_with("EOKA_TEST_") || SUPPORTED.contains(&name)
}

pub fn validate_environment() -> Result<(), &'static str> {
    if env::vars_os().any(|(name, _)| !supported(&name.to_string_lossy())) {
        return Err("unsupported EOKA_TEST option: old diagnostic flags were retired; see crates/datadome/README.md");
    }
    Ok(())
}

fn validate_challenge_proof(required: bool, has_selector: bool) -> Result<(), &'static str> {
    if required != has_selector {
        return Err(
            "EOKA_TEST_REQUIRE_SOLVED and EOKA_TEST_SUCCESS_SELECTOR must be used together",
        );
    }
    Ok(())
}

impl Options {
    pub fn read() -> Result<Self, Box<dyn std::error::Error>> {
        validate_environment()?;
        let url = env::args()
            .nth(1)
            .ok_or("usage: datadome_solve TARGET_URL")?;
        let profile = optional("EOKA_TEST_PROFILE_URL")?;
        let require_solved = env::var_os("EOKA_TEST_REQUIRE_SOLVED").is_some();
        let success_selector = optional("EOKA_TEST_SUCCESS_SELECTOR")?;
        if env::var_os("EOKA_TEST_REQUIRE_PASSWORD_STAGE").is_some() && profile.is_none() {
            return Err("password-stage mode requires EOKA_TEST_PROFILE_URL".into());
        }
        if profile.is_some()
            && url::Url::parse(&url)?.origin().ascii_serialization() != "https://soundcloud.com"
        {
            return Err("SoundCloud profile checks require a SoundCloud target URL".into());
        }
        validate_challenge_proof(require_solved, success_selector.is_some())?;
        let click_selector = optional("EOKA_TEST_CLICK_SELECTOR")?
            .or_else(|| profile.as_ref().map(|_| "button.loginButton".into()));
        Ok(Self {
            url,
            profile,
            require_solved,
            success_selector,
            click_selector,
            chrome_path: optional("EOKA_TEST_CHROME_PATH")?,
            profile_dir: optional("EOKA_TEST_PROFILE_DIR")?,
            visible: env::var_os("EOKA_TEST_VISIBLE").is_some(),
            native: env::var_os("EOKA_TEST_NATIVE_LAUNCH").is_some(),
            software_compositing: env::var_os("EOKA_TEST_SOFTWARE_COMPOSITING").is_some(),
        })
    }

    pub async fn launch(&self) -> eoka::Result<eoka::Browser> {
        eoka::Browser::launch_with(|config| {
            config.headless = !self.visible;
            config.chrome_path = self.chrome_path.clone();
            config.user_data_dir = self.profile_dir.clone();
            if self.software_compositing {
                config.extra_args.extend([
                    "--disable-gpu-compositing".into(),
                    "--use-angle=vulkan".into(),
                ]);
            }
            if self.native {
                config.live_session = true;
                config.strip_x_client_data = false;
                config.extra_args.extend([
                    "--no-first-run".into(),
                    "--no-default-browser-check".into(),
                    "--window-size=1920,1080".into(),
                ]);
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_options_cannot_be_silently_ignored() {
        assert!(validate_challenge_proof(false, false).is_ok());
        assert!(validate_challenge_proof(true, true).is_ok());
        assert!(validate_challenge_proof(false, true).is_err());
        assert!(validate_challenge_proof(true, false).is_err());
    }

    #[test]
    fn retired_diagnostics_are_rejected_instead_of_silently_ignored() {
        for name in SUPPORTED {
            assert!(supported(name));
        }
        for name in [
            "EOKA_TEST_INPUT_BACKEND",
            "EOKA_TEST_ISOLATED_X11",
            "EOKA_TEST_NETWORK_EVIDENCE",
            "EOKA_TEST_WIRE_ONLY",
            "EOKA_TEST_NATIVE_NETLOG",
            "EOKA_TEST_CAPTURE_DIR",
            "EOKA_TEST_SCREENSHOT",
            "EOKA_TEST_STATE_OUT",
            "EOKA_TEST_INSPECT_FRAME",
            "EOKA_TEST_CLICK_TEXT",
        ] {
            assert!(!supported(name));
        }
        assert!(supported("PATH"));
    }
}
