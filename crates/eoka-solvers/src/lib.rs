mod injection;

pub use injection::{build_captcha_inject_js, parse_captcha_inject_kind, CaptchaInjectionKind};

#[cfg(feature = "anti-captcha")]
mod anti_captcha;

#[cfg(feature = "anti-captcha")]
pub use anti_captcha::{AntiCaptcha, CaptchaSolution};

#[cfg(feature = "datadome")]
pub mod datadome;
