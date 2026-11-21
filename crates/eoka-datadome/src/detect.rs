use eoka::{Page, Result};
use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub enum Detection {
    NotPresent,
    Slider,
    EmbeddedChallenge,
    IpBanned,
}

pub async fn detect(page: &Page) -> Result<Detection> {
    let top = classify_url(&page.url().await?);
    if top != Detection::NotPresent {
        return Ok(top);
    }
    let frames = page.frames().await?;
    Ok(classify_embedded(
        frames.iter().map(|frame| frame.url.as_str()),
    ))
}

fn classify_embedded<'a>(urls: impl Iterator<Item = &'a str>) -> Detection {
    let mut result = Detection::NotPresent;
    for url in urls {
        match classify_url(url) {
            Detection::IpBanned => return Detection::IpBanned,
            Detection::Slider => result = Detection::EmbeddedChallenge,
            _ => {}
        }
    }
    result
}

pub fn classify_url(url: &str) -> Detection {
    let Ok(url) = Url::parse(url) else {
        return Detection::NotPresent;
    };
    let host = url.host_str().unwrap_or_default();
    if !matches!(url.scheme(), "http" | "https")
        || !(host == "captcha-delivery.com" || host.ends_with(".captcha-delivery.com"))
        || !matches!(url.path(), "/captcha" | "/captcha/")
    {
        return Detection::NotPresent;
    }
    if url
        .query_pairs()
        .any(|(key, value)| key == "t" && value == "bv")
    {
        Detection::IpBanned
    } else {
        Detection::Slider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_interstitial() {
        assert_eq!(
            classify_url("https://geo.captcha-delivery.com/captcha/?t=fe"),
            Detection::Slider
        );
    }

    #[test]
    fn classifies_banned_ip() {
        assert_eq!(
            classify_url("https://geo.captcha-delivery.com/captcha/?t=bv"),
            Detection::IpBanned
        );
    }

    #[test]
    fn ignores_normal_pages_and_host_lookalikes() {
        for url in [
            "https://soundcloud.com/",
            "https://geo.captcha-delivery.com/interstitial/?t=fe",
            "https://captcha-delivery.com.example.com/captcha/?t=fe",
            "https://example.com/?next=https://geo.captcha-delivery.com/captcha/?t=fe",
            "https://example.com/#https://geo.captcha-delivery.com/captcha/?t=bv",
        ] {
            assert_eq!(classify_url(url), Detection::NotPresent, "{url}");
        }
    }

    #[test]
    fn soundcloud_nested_challenge_is_not_absent() {
        assert_eq!(
            classify_embedded(
                [
                    "https://soundcloud.com/",
                    "https://secure.soundcloud.com/web-auth",
                    "https://geo.captcha-delivery.com/captcha/?t=fe",
                ]
                .into_iter()
            ),
            Detection::EmbeddedChallenge
        );
    }

    #[test]
    fn embedded_ban_takes_precedence() {
        assert_eq!(
            classify_embedded(
                [
                    "https://geo.captcha-delivery.com/captcha/?t=fe",
                    "https://geo.captcha-delivery.com/captcha/?t=bv",
                ]
                .into_iter()
            ),
            Detection::IpBanned
        );
    }
}
