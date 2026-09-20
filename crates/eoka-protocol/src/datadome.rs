use serde::{Deserialize, Serialize};

pub const DATADOME_DEFAULT_ATTEMPTS: u32 = 1;
pub const DATADOME_MAX_ATTEMPTS: u32 = 4;
pub const DATADOME_DEFAULT_TIMEOUT_MS: u64 = 60_000;
pub const DATADOME_MIN_TIMEOUT_MS: u64 = 5_000;
pub const DATADOME_MAX_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CaptchaDatadomeArgs {
    #[serde(deserialize_with = "deserialize_attempts")]
    #[schemars(range(min = 1, max = DATADOME_MAX_ATTEMPTS))]
    pub max_attempts: u32,
    #[serde(deserialize_with = "deserialize_timeout")]
    #[schemars(range(min = DATADOME_MIN_TIMEOUT_MS, max = DATADOME_MAX_TIMEOUT_MS))]
    pub timeout_ms: u64,
}

fn deserialize_attempts<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(d)?;
    validate_attempts(value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

fn deserialize_timeout<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let value = u64::deserialize(d)?;
    validate_timeout(value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

fn validate_attempts(value: u32) -> Result<(), &'static str> {
    if !(1..=DATADOME_MAX_ATTEMPTS).contains(&value) {
        return Err("max_attempts must be between 1 and 4");
    }
    Ok(())
}

fn validate_timeout(value: u64) -> Result<(), &'static str> {
    if !(DATADOME_MIN_TIMEOUT_MS..=DATADOME_MAX_TIMEOUT_MS).contains(&value) {
        return Err("timeout_ms must be between 5000 and 120000");
    }
    Ok(())
}

impl Default for CaptchaDatadomeArgs {
    fn default() -> Self {
        Self {
            max_attempts: DATADOME_DEFAULT_ATTEMPTS,
            timeout_ms: DATADOME_DEFAULT_TIMEOUT_MS,
        }
    }
}

impl CaptchaDatadomeArgs {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_attempts(self.max_attempts)?;
        validate_timeout(self.timeout_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataDomeOutcome {
    ChallengeCleared,
    NotPresent,
    Blocked,
    IpBanned,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CaptchaDatadomeResult {
    pub outcome: DataDomeOutcome,
    pub tab_id: String,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{input_schema_for_cmd, request_from_cmd, Request};
    use serde_json::json;

    #[test]
    fn defaults_ranges_and_roundtrip_are_canonical() {
        let request = request_from_cmd("captcha_datadome", json!({})).unwrap();
        assert_eq!(
            request.args_json(),
            json!({"max_attempts":1,"timeout_ms":60000})
        );
        let decoded: Request =
            serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
        assert_eq!(decoded.cmd(), "captcha_datadome");
        assert_eq!(decoded.args_json(), request.args_json());
        for input in [
            json!({"max_attempts":0}),
            json!({"max_attempts":5}),
            json!({"timeout_ms":4999}),
            json!({"timeout_ms":120001}),
            json!({"api_key":"not-used"}),
            json!({"max_attempts":null}),
            json!({"max_attempts":-1}),
            json!({"max_attempts":1.5}),
            json!({"timeout_ms":"60000"}),
            json!({"timeout_ms":null}),
        ] {
            assert!(request_from_cmd("captcha_datadome", input).is_err());
        }
        for (input, expected) in [
            (
                json!({"max_attempts":2}),
                json!({"max_attempts":2,"timeout_ms":60000}),
            ),
            (
                json!({"timeout_ms":5000}),
                json!({"max_attempts":1,"timeout_ms":5000}),
            ),
        ] {
            assert_eq!(
                request_from_cmd("captcha_datadome", input)
                    .unwrap()
                    .args_json(),
                expected
            );
        }
        let schema = input_schema_for_cmd("captcha_datadome");
        assert_eq!(
            schema["properties"]["max_attempts"]["default"],
            DATADOME_DEFAULT_ATTEMPTS
        );
        assert_eq!(
            schema["properties"]["timeout_ms"]["default"],
            DATADOME_DEFAULT_TIMEOUT_MS
        );
        assert_eq!(schema["properties"]["max_attempts"]["minimum"], 1);
        assert_eq!(
            schema["properties"]["max_attempts"]["maximum"],
            DATADOME_MAX_ATTEMPTS
        );
        assert_eq!(
            schema["properties"]["timeout_ms"]["minimum"],
            DATADOME_MIN_TIMEOUT_MS
        );
        assert_eq!(
            schema["properties"]["timeout_ms"]["maximum"],
            DATADOME_MAX_TIMEOUT_MS
        );
        for timeout_ms in [DATADOME_MIN_TIMEOUT_MS, DATADOME_MAX_TIMEOUT_MS] {
            let args = CaptchaDatadomeArgs {
                max_attempts: DATADOME_MAX_ATTEMPTS,
                timeout_ms,
            };
            assert!(args.validate().is_ok());
            assert!(
                request_from_cmd("captcha_datadome", serde_json::to_value(args).unwrap()).is_ok()
            );
        }
        assert_eq!(schema["additionalProperties"], false);
        assert!(CaptchaDatadomeArgs {
            max_attempts: 0,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
