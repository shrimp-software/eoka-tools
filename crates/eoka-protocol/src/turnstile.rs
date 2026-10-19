use serde::{Deserialize, Serialize};

pub const TURNSTILE_DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const TURNSTILE_MAX_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CaptchaTurnstileArgs {
    #[serde(deserialize_with = "deserialize_timeout")]
    #[schemars(range(min = 1, max = TURNSTILE_MAX_TIMEOUT_MS))]
    pub timeout_ms: u64,
}

impl Default for CaptchaTurnstileArgs {
    fn default() -> Self {
        Self {
            timeout_ms: TURNSTILE_DEFAULT_TIMEOUT_MS,
        }
    }
}

fn deserialize_timeout<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let value = u64::deserialize(d)?;
    if !(1..=TURNSTILE_MAX_TIMEOUT_MS).contains(&value) {
        return Err(serde::de::Error::custom(
            "timeout_ms must be between 1 and 120000",
        ));
    }
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CaptchaTurnstileResult {
    pub tab_id: String,
    pub clicks: u32,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_defaults_and_is_bounded() {
        let args: CaptchaTurnstileArgs = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(args.timeout_ms, TURNSTILE_DEFAULT_TIMEOUT_MS);
        for timeout in [0, TURNSTILE_MAX_TIMEOUT_MS + 1] {
            assert!(serde_json::from_value::<CaptchaTurnstileArgs>(
                serde_json::json!({ "timeout_ms": timeout })
            )
            .is_err());
        }
    }
}
