use super::params::{self, ParamDef, Params};
use super::Action;
use crate::{Error, Result};
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub name: String,

    #[serde(default)]
    pub params: HashMap<String, ParamDef>,

    #[serde(default)]
    pub browser: BrowserConfig,

    pub target: TargetUrl,

    #[serde(default)]
    pub actions: Vec<Action>,

    pub success: Option<SuccessCondition>,

    pub on_failure: Option<OnFailure>,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::parse_with_params(&content, &Params::new())
    }

    pub fn load_with_params<P: AsRef<Path>>(path: P, params: &Params) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::parse_with_params(&content, params)
    }

    pub fn parse(yaml: &str) -> Result<Self> {
        Self::parse_with_params(yaml, &Params::new())
    }

    pub fn parse_with_params(yaml: &str, params: &Params) -> Result<Self> {
        let mut value: serde_yaml::Value = serde_yaml::from_str(yaml)?;

        let defs: HashMap<String, ParamDef> = value
            .get("params")
            .and_then(|v| serde_yaml::from_value(v.clone()).ok())
            .unwrap_or_default();

        params::substitute_value(&mut value, params, &defs)?;

        let config: Config = serde_yaml::from_value(value)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(Error::Config("name is required".into()));
        }
        if self.target.url.is_empty() {
            return Err(Error::Config("target.url is required".into()));
        }
        if let Some(ref success) = self.success {
            if success.any.is_some() && success.all.is_some() {
                return Err(Error::Config(
                    "success: specify either 'any' or 'all', not both".into(),
                ));
            }
        }
        if let Some(ref on_failure) = self.on_failure {
            if let Some(ref retry) = on_failure.retry {
                if retry.attempts == 0 {
                    return Err(Error::Config(
                        "on_failure.retry.attempts must be at least 1".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BrowserConfig {
    #[serde(default)]
    pub headless: bool,

    pub proxy: Option<String>,

    pub user_agent: Option<String>,

    pub viewport: Option<Viewport>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetUrl {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SuccessCondition {
    pub any: Option<Vec<Condition>>,

    pub all: Option<Vec<Condition>>,
}

#[derive(Debug, Clone)]
pub enum Condition {
    UrlContains(String),
    TextContains(String),
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(ConditionVisitor)
    }
}

struct ConditionVisitor;

impl<'de> Visitor<'de> for ConditionVisitor {
    type Value = Condition;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a condition map with single key (url_contains or text_contains)")
    }

    fn visit_map<M>(self, mut map: M) -> std::result::Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let key: String = map
            .next_key()?
            .ok_or_else(|| de::Error::custom("expected condition type key"))?;

        match key.as_str() {
            "url_contains" => Ok(Condition::UrlContains(map.next_value()?)),
            "text_contains" => Ok(Condition::TextContains(map.next_value()?)),
            other => Err(de::Error::unknown_variant(
                other,
                &["url_contains", "text_contains"],
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OnFailure {
    pub screenshot: Option<String>,

    pub retry: Option<RetryConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub attempts: u32,

    pub delay_ms: u64,
}
