use std::time::Duration;

pub async fn auth_frame(page: &eoka::Page) -> eoka::Result<Option<eoka::FrameInfo>> {
    select_auth_frame(page.frames().await?)
}

fn select_auth_frame(frames: Vec<eoka::FrameInfo>) -> eoka::Result<Option<eoka::FrameInfo>> {
    let mut frames = frames.into_iter().filter(|frame| {
        url::Url::parse(&frame.url)
            .is_ok_and(|url| url.origin().ascii_serialization() == "https://secure.soundcloud.com")
    });
    let auth = frames.next();
    if frames.next().is_some() {
        return Err(eoka::Error::cdp_msg("ambiguous SoundCloud auth frames"));
    }
    Ok(auth)
}

fn password_form_script(profile: &str) -> String {
    let normalized = profile
        .strip_prefix("https://soundcloud.com/")
        .unwrap_or(profile)
        .trim_end_matches('/');
    let accepted = serde_json::to_string(&[profile, normalized]).expect("string serialization");
    format!(
        r#"(() => {{
        const email=document.querySelector('input[name=email]');
        const password=document.querySelector('#enter_password_field[name=password][type=password][autocomplete="current-password"]');
        if (!email?.readOnly || !{accepted}.includes(email.value) || !password || password.disabled || password.readOnly || password.value !== '') return false;
        if (!password.checkVisibility({{checkOpacity:true,checkVisibilityCSS:true}})) return false;
        const r=password.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+r.height/2;
        return r.width>0 && r.height>0 && x>=0 && y>=0 && x<innerWidth && y<innerHeight && document.elementFromPoint(x,y)===password;
    }})()"#
    )
}

pub async fn at_password_stage(
    page: &eoka::Page,
    frame_id: &str,
    profile: &str,
) -> eoka::Result<bool> {
    let point: Option<[f64; 2]> = page
        .evaluate_in_frame_id(
            frame_id,
            &format!(
                r#"(() => {{
        if (location.origin !== 'https://secure.soundcloud.com' || !{}) return null;
        const r=document.querySelector('#enter_password_field').getBoundingClientRect();
        return [r.left+r.width/2,r.top+r.height/2];
    }})()"#,
                password_form_script(profile)
            ),
        )
        .await?;
    let Some([x, y]) = point else {
        return Ok(false);
    };
    let Ok((x, y)) = page.frame_point_to_viewport(frame_id, x, y).await else {
        return Ok(false);
    };
    page.evaluate_sync(&format!(r#"(() => {{
        const hit=document.elementFromPoint({x},{y});
        if (hit?.tagName !== 'IFRAME') return false;
        try {{ return new URL(hit.src,location.href).origin === 'https://secure.soundcloud.com'; }} catch {{ return false; }}
    }})()"#)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_selection_requires_one_exact_secure_origin() {
        let frame = |url: &str| eoka::FrameInfo {
            id: url.into(),
            url: url.into(),
            name: None,
        };
        assert!(select_auth_frame(vec![
            frame("http://secure.soundcloud.com"),
            frame("https://secure.soundcloud.com.evil.test")
        ])
        .unwrap()
        .is_none());
        let valid = "https://secure.soundcloud.com/web-auth";
        assert_eq!(
            select_auth_frame(vec![frame(valid)]).unwrap().unwrap().url,
            valid
        );
        assert!(select_auth_frame(vec![frame(valid), frame(valid)]).is_err());
    }

    #[tokio::test]
    #[ignore = "requires Chrome; local password-stage fixture only"]
    async fn password_gate_matches_reported_stage_and_rejects_lookalikes() {
        let browser = eoka::Browser::launch().await.unwrap();
        let page = browser.new_blank_page().await.unwrap();
        let html = r#"<main><h1>Welcome back!</h1><input name="email" readonly value="fixture-user"><input id="enter_password_field" name="password" type="password" autocomplete="current-password"><button disabled>Continue</button></main>"#;
        let script = password_form_script("https://soundcloud.com/fixture-user");
        for (mutation,expected) in [
            ("",true),
            ("document.querySelector('[name=email]').readOnly=false",false),
            ("document.querySelector('[name=email]').value='another-user'",false),
            ("document.querySelector('[name=password]').type='text'",false),
            ("document.querySelector('[name=password]').readOnly=true",false),
            ("document.querySelector('[name=password]').style.opacity=0",false),
            ("document.body.insertAdjacentHTML('beforeend','<div style=\"position:fixed;inset:0;z-index:999\"></div>')",false),
        ] {
            page.execute_sync(&format!("document.body.innerHTML={};{mutation}",serde_json::to_string(html).unwrap())).await.unwrap();
            assert_eq!(page.evaluate_sync::<bool>(&script).await.unwrap(),expected,"{mutation}");
        }
        browser.close().await.unwrap();
    }
}

pub async fn enter(page: &eoka::Page, profile: &str) -> Result<(), Box<dyn std::error::Error>> {
    let auth = auth_frame(page)
        .await?
        .ok_or("SoundCloud auth frame missing")?;
    if at_password_stage(page, &auth.id, profile).await? {
        return Ok(());
    }
    let initial: bool = page.evaluate_in_frame_id(&auth.id,
        "location.origin === 'https://secure.soundcloud.com' && document.body.innerText.includes('Your email address or profile URL')").await?;
    if !initial {
        return Err("not the initial profile-entry stage".into());
    }
    super::click_visible(page, Some(&auth.id), "input[name=email]", None).await?;
    let focused: bool = page
        .evaluate_in_frame_id(
            &auth.id,
            "document.activeElement?.getAttribute('name') === 'email'",
        )
        .await?;
    if !focused {
        return Err("profile input did not receive focus".into());
    }
    let normalized = profile
        .strip_prefix("https://soundcloud.com/")
        .unwrap_or(profile)
        .trim_end_matches('/');
    let current: String = page
        .evaluate_in_frame_id(
            &auth.id,
            "document.querySelector('input[name=email]').value",
        )
        .await?;
    if current.is_empty() {
        page.type_text(profile).await?;
    } else if current != profile && current != normalized {
        return Err("profile input already contains a different identifier".into());
    }
    let accepted = serde_json::to_string(&[profile, normalized])?;
    let filled: bool = page
        .evaluate_in_frame_id(
            &auth.id,
            &format!("{accepted}.includes(document.querySelector('input[name=email]')?.value)"),
        )
        .await?;
    if !filled {
        return Err("profile input did not retain the supplied identifier".into());
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let initial: bool = page
        .evaluate_in_frame_id(
            &auth.id,
            "document.body.innerText.includes('Your email address or profile URL')",
        )
        .await?;
    if !initial {
        return Err("profile-entry stage changed before Continue".into());
    }
    super::click_visible(page, Some(&auth.id), "button", Some("Continue")).await?;
    Ok(())
}
