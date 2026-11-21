#[cfg(feature = "anti-captcha")]
mod anti_captcha;
mod injection;

#[cfg(feature = "anti-captcha")]
pub use anti_captcha::{AntiCaptcha, CaptchaSolution};
pub use injection::{build_captcha_inject_js, parse_captcha_inject_kind, CaptchaInjectionKind};
