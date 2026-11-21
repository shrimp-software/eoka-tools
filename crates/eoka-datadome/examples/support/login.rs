use std::{path::Path, time::Duration};

use eoka::{CapturedFrameResponse, FrameResponseCaptureOptions, Page};
use zeroize::Zeroizing;

pub fn enabled() -> bool {
    std::env::var_os("EOKA_TEST_PASSWORD_FILE").is_some()
}

pub fn validate_configuration() -> Result<(), &'static str> {
    if !enabled() {
        return Ok(());
    }
    super::options::validate_environment()?;
    if !cfg!(unix) {
        return Err("credential mode currently requires Unix file permission checks");
    }
    for name in [
        "EOKA_TEST_PASSWORD_FILE",
        "EOKA_TEST_PROFILE_URL",
        "EOKA_TEST_PROFILE_DIR",
    ] {
        if std::env::var(name)
            .ok()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(
                "credential configuration must contain nonempty UTF-8 paths and profile identifier",
            );
        }
    }
    validate_flags(|name| std::env::var_os(name).is_some())
}

fn validate_flags(has: impl Fn(&str) -> bool) -> Result<(), &'static str> {
    for required in [
        "EOKA_TEST_REQUIRE_PASSWORD_STAGE",
        "EOKA_TEST_PROFILE_URL",
        "EOKA_TEST_PROFILE_DIR",
    ] {
        if !has(required) {
            return Err("credential mode requires password-stage mode, profile URL and a dedicated profile directory");
        }
    }
    Ok(())
}

#[cfg(unix)]
fn read_secret(path: &Path) -> Result<Zeroizing<String>, &'static str> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "cannot open credential file")?;
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect credential file")?;
    if !metadata.is_file()
        || metadata.mode() & 0o7777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
    {
        return Err("credential file must be an owned, regular, single-link mode-0600 file");
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read credential file")?;
    if bytes.len() > 4096 {
        return Err("credential file exceeds the size limit");
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "credential file must be UTF-8")?;
    let text = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err("credential file must contain one nonempty password line");
    }
    Ok(Zeroizing::new(text.to_owned()))
}

#[cfg(not(unix))]
fn read_secret(_: &Path) -> Result<Zeroizing<String>, &'static str> {
    Err("credential file permission validation is currently Unix-only")
}

fn normalized_profile(profile: &str) -> Result<&str, &'static str> {
    let slug = profile
        .strip_prefix("https://soundcloud.com/")
        .unwrap_or(profile)
        .trim_end_matches('/');
    if slug.is_empty()
        || slug.len() > 200
        || !slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("credential mode requires an unambiguous SoundCloud profile slug or URL");
    }
    Ok(slug)
}

fn form_action_script(origin: &str, profile: &str, password: Option<&str>) -> Zeroizing<String> {
    let args = Zeroizing::new(
        serde_json::json!({"origin":origin,"profile":profile,"password":password}).to_string(),
    );
    Zeroizing::new(r#"(() => {
        const a=__ARGS__;
        if (location.origin!==a.origin || document.documentElement.dataset.eokaCredentialSubmitted==='true') return false;
        const identifiers=document.querySelectorAll('input[name=email]');
        const fields=document.querySelectorAll(a.password!==null ? '#enter_password_field[name=password][type=password][autocomplete="current-password"]' : '#enter_password_field[type=password][data-eoka-credential-filled="true"]');
        if (identifiers.length!==1 || fields.length!==1) return false;
        const identifier=identifiers[0], field=fields[0];
        if (!identifier.readOnly || ![a.profile,'https://soundcloud.com/'+a.profile].includes(identifier.value) || field.disabled || field.readOnly) return false;
        const visible=e=>{if(!e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true}))return false;const r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return r.width>0&&r.height>0&&!!hit&&(hit===e||e.contains(hit));};
        if (!visible(field)) return false;
        if (a.password!==null) {
            if (field.value!=='') return false;
            Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,a.password);
            field.dispatchEvent(new Event('input',{bubbles:true}));
            field.dispatchEvent(new Event('change',{bubbles:true}));
            if (!field.isConnected || field.value!==a.password) return false;
            field.dataset.eokaCredentialFilled='true';
            return true;
        }
        if (field.value==='') return false;
        const buttons=[...document.querySelectorAll('button')].filter(b=>b.textContent.trim()==='Continue'&&!b.disabled&&visible(b));
        if(buttons.length!==1)return false;
        document.documentElement.dataset.eokaCredentialSubmitted='true';
        buttons[0].click();
        return true;
    })()"#.replace("__ARGS__", &args))
}

fn matching_identity(response: &CapturedFrameResponse, expected: &str, endpoint: &str) -> bool {
    if response.method != "GET"
        || response.status != Some(200)
        || response.body_truncated
        || response.metadata_truncated
        || response.body_error.is_some()
    {
        return false;
    }
    let Ok(url) = url::Url::parse(&response.url) else {
        return false;
    };
    if format!("{}{}", url.origin().ascii_serialization(), url.path()) != endpoint {
        return false;
    }
    let Some(body) = response.body.as_deref() else {
        return false;
    };
    let Ok(user) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    user["kind"] == "user"
        && user["permalink"] == expected
        && user["id"].as_u64().is_some_and(|id| id > 0)
}

pub async fn attempt(page: &Page, profile: &str) -> Result<(), &'static str> {
    validate_configuration()?;
    let slug = normalized_profile(profile)?;
    let location = page.url().await.map_err(|_| "cannot verify login page")?;
    let location = url::Url::parse(&location).map_err(|_| "invalid login page URL")?;
    if location.origin().ascii_serialization() != "https://soundcloud.com" {
        return Err("credential mode requires the SoundCloud top-level origin");
    }
    let frame = super::profile::auth_frame(page)
        .await
        .map_err(|_| "cannot resolve an unambiguous auth frame")?
        .ok_or("missing SoundCloud auth frame")?;
    let auth = &frame.id;
    if !super::profile::at_password_stage(page, auth, profile)
        .await
        .map_err(|_| "cannot verify password form")?
    {
        return Err("password form verification failed before credential entry");
    }
    let tree: serde_json::Value = page
        .session()
        .send("Page.getFrameTree", &serde_json::json!({}))
        .await
        .map_err(|_| "cannot resolve main frame")?;
    let root = tree["frameTree"]["frame"]["id"]
        .as_str()
        .ok_or("missing main frame")?;
    let capture = page
        .capture_frame_responses(
            root,
            FrameResponseCaptureOptions {
                url_patterns: vec![
                    "https://api-v2.soundcloud.com/me".into(),
                    "https://api-v2.soundcloud.com/me?*".into(),
                ],
                capture_bodies: true,
                max_responses: 8,
                max_body_bytes: 65536,
                max_total_body_bytes: 131072,
                ..Default::default()
            },
        )
        .await
        .map_err(|_| "cannot arm authenticated identity check")?;
    let result = async {
        let path = std::env::var_os("EOKA_TEST_PASSWORD_FILE")
            .ok_or("missing credential file configuration")?;
        let password = read_secret(Path::new(&path))?;
        let script = form_action_script("https://secure.soundcloud.com", slug, Some(&password));
        let filled: bool = page
            .evaluate_in_frame_id(auth, &script)
            .await
            .map_err(|_| "credential entry failed; details suppressed")?;
        drop(script);
        drop(password);
        if !filled {
            return Err("credential entry was rejected by the form guard");
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        if capture
            .snapshot()
            .responses
            .iter()
            .any(|r| matching_identity(r, slug, "https://api-v2.soundcloud.com/me"))
        {
            return Err("account identity was observed before submission; not a fresh login proof");
        }
        let submitted_at = std::time::SystemTime::now();
        let submitted: bool = page
            .evaluate_in_frame_id(
                auth,
                &form_action_script("https://secure.soundcloud.com", slug, None),
            )
            .await
            .map_err(|_| "credential submission failed; no retry will be made")?;
        if !submitted {
            return Err("credential submit guard failed; no retry will be made");
        }
        for _ in 0..80 {
            let receipt = capture.snapshot().responses.iter().any(|r| {
                r.captured_at >= submitted_at
                    && matching_identity(r, slug, "https://api-v2.soundcloud.com/me")
            });
            if receipt {
                let frames = page
                    .frames()
                    .await
                    .map_err(|_| "cannot verify auth dialog closure")?;
                if !frames.iter().any(|f| {
                    &f.id == auth
                        || url::Url::parse(&f.url).is_ok_and(|u| {
                            u.host_str().is_some_and(|h| {
                                h == "captcha-delivery.com" || h.ends_with(".captcha-delivery.com")
                            })
                        })
                }) {
                    let current = page
                        .url()
                        .await
                        .map_err(|_| "cannot verify post-login application origin")?;
                    if !url::Url::parse(&current)
                        .is_ok_and(|u| u.origin().ascii_serialization() == "https://soundcloud.com")
                    {
                        return Err("unexpected application origin after credential submission");
                    }
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err("login unverified: no matching authenticated identity receipt; no retry will be made")
    }
    .await;
    let report = capture.stop().await;
    if report.worker_failed
        || report.continuation_errors > 0
        || report.dropped_events > 0
        || report.dropped_responses > 0
    {
        return Err("login identity capture was incomplete; no success claim");
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    #[test]
    fn credential_mode_requires_explicit_gates() {
        let required = [
            "EOKA_TEST_REQUIRE_PASSWORD_STAGE",
            "EOKA_TEST_PROFILE_URL",
            "EOKA_TEST_PROFILE_DIR",
        ];
        assert!(validate_flags(|name| required.contains(&name)).is_ok());
        for missing in required {
            assert!(validate_flags(|name| required.contains(&name) && name != missing).is_err());
        }
    }

    #[test]
    fn credential_files_are_private_bounded_and_never_guessed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("secret");
        let create = |bytes: &[u8]| {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .unwrap();
            file.write_all(bytes).unwrap();
        };
        create(b" fixture-password \r\n");
        assert_eq!(&**read_secret(&path).unwrap(), " fixture-password ");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_secret(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = directory.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_secret(&link).is_err());
        let hard_link = directory.path().join("hard-link");
        std::fs::hard_link(&path, &hard_link).unwrap();
        assert!(read_secret(&path).is_err());
        std::fs::remove_file(&hard_link).unwrap();
        let fifo = directory.path().join("fifo");
        let fifo_c =
            std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(fifo.as_os_str()))
                .unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        assert!(read_secret(&fifo).is_err());
        std::fs::remove_file(&path).unwrap();
        for bytes in [
            b"".to_vec(),
            vec![b'x'; 4097],
            b"two\nlines".to_vec(),
            vec![0xff],
            b"nul\0byte".to_vec(),
        ] {
            create(&bytes);
            assert!(read_secret(&path).is_err());
            std::fs::remove_file(&path).unwrap();
        }
        assert!(read_secret(directory.path()).is_err());
        assert!(normalized_profile("https://soundcloud.com.evil/fixture").is_err());
    }

    #[tokio::test]
    #[ignore = "requires Chrome; local credential submission and identity receipt fixture"]
    async fn local_login_submission_and_identity_gate() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let header_end = loop {
                        let mut chunk = [0; 1024];
                        let count = stream.read(&mut chunk).await.unwrap();
                        if count == 0 {
                            return;
                        }
                        request.extend_from_slice(&chunk[..count]);
                        assert!(request.len() <= 16384);
                        if let Some(index) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                            break index + 4;
                        }
                    };
                    let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    assert!(length <= 4096);
                    while request.len() < header_end + length {
                        let mut chunk = [0; 1024];
                        let count = stream.read(&mut chunk).await.unwrap();
                        assert!(count > 0);
                        request.extend_from_slice(&chunk[..count]);
                    }
                    let is_me = headers.starts_with("GET /me ");
                    let is_login = headers.starts_with("POST /login ");
                    let session = headers.lines().any(|line| {
                        line.split_once(':').is_some_and(|(key, value)| {
                            key.eq_ignore_ascii_case("cookie")
                                && value.trim() == "fixture-auth=fixture-only"
                        })
                    });
                    let valid_password = is_login
                        && serde_json::from_slice::<serde_json::Value>(
                            &request[header_end..header_end + length],
                        )
                        .is_ok_and(|v| v["password"] == "fixture-password");
                    let status = if (is_me && !session) || (is_login && !valid_password) {
                        "401 Unauthorized"
                    } else {
                        "200 OK"
                    };
                    let body = if is_me && session {
                        r#"{"kind":"user","id":17,"permalink":"fixture-user"}"#
                    } else if is_me || is_login {
                        "{}"
                    } else {
                        r#"<input name=email readonly value=fixture-user><input id=enter_password_field name=password type=password autocomplete=current-password><button disabled><span style="display:block;padding:8px">Continue</span></button><script>document.querySelector('[name=password]').addEventListener('input',e=>{document.querySelector('button').disabled=false;e.target.autocomplete='off';e.target.name='credential';});window.submits=0;document.querySelector('button').onclick=async()=>{window.submits++;const r=await fetch('/login',{method:'POST',body:JSON.stringify({password:document.querySelector('#enter_password_field').value})});if(r.ok)await fetch('/me');};</script>"#
                    };
                    let cookie = if valid_password {
                        "Set-Cookie: fixture-auth=fixture-only; HttpOnly; SameSite=Strict; Path=/\r\n"
                    } else {
                        ""
                    };
                    let response=format!("HTTP/1.1 {status}\r\n{cookie}Content-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",if is_me||is_login{"application/json"}else{"text/html"},body.len(),body);
                    stream.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });
        let browser = eoka::Browser::launch().await.unwrap();
        let page = browser.new_page(&origin).await.unwrap();
        let tree: serde_json::Value = page
            .session()
            .send("Page.getFrameTree", &serde_json::json!({}))
            .await
            .unwrap();
        let capture = page
            .capture_frame_responses(
                tree["frameTree"]["frame"]["id"].as_str().unwrap(),
                FrameResponseCaptureOptions {
                    url_patterns: vec![format!("{origin}/me")],
                    capture_bodies: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            page.evaluate::<u16>("fetch('/me').then(r=>r.status)")
                .await
                .unwrap(),
            401
        );
        let directory = tempfile::tempdir().unwrap();
        let secret_path = directory.path().join("credential");
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&secret_path)
                .unwrap();
            f.write_all(b"fixture-password\n").unwrap();
        }
        let secret = read_secret(&secret_path).unwrap();
        for (expected_origin, profile) in [
            ("https://secure.soundcloud.com", "fixture-user"),
            (origin.as_str(), "wrong-user"),
        ] {
            assert!(!page
                .evaluate_sync::<bool>(&form_action_script(
                    expected_origin,
                    profile,
                    Some("fixture-password")
                ))
                .await
                .unwrap());
            assert!(page
                .evaluate_sync::<bool>("document.querySelector('[name=password]').value===''")
                .await
                .unwrap());
        }
        assert!(!page
            .evaluate_sync::<bool>(&form_action_script(&origin, "fixture-user", None))
            .await
            .unwrap());
        assert!(page
            .evaluate_sync::<bool>(&form_action_script(&origin, "fixture-user", Some(&secret)))
            .await
            .unwrap());
        assert!(!page
            .evaluate_sync::<bool>(&form_action_script(
                &origin,
                "fixture-user",
                Some("replacement")
            ))
            .await
            .unwrap());
        assert!(page
            .evaluate_sync::<bool>(&form_action_script(&origin, "fixture-user", None))
            .await
            .unwrap());
        assert!(!page
            .evaluate_sync::<bool>(&form_action_script(&origin, "fixture-user", None))
            .await
            .unwrap());
        for _ in 0..40 {
            if capture
                .snapshot()
                .responses
                .iter()
                .any(|r| r.status == Some(200))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let report = capture.stop().await;
        assert!(report.responses.iter().any(|r| r.status == Some(401)));
        let mut receipt = report
            .responses
            .into_iter()
            .find(|r| r.status == Some(200))
            .unwrap();
        let endpoint = format!("{origin}/me");
        assert!(matching_identity(&receipt, "fixture-user", &endpoint));
        assert!(!matching_identity(&receipt, "wrong-user", &endpoint));
        assert!(!matching_identity(
            &receipt,
            "fixture-user",
            "https://api-v2.soundcloud.com/me"
        ));
        receipt.status = Some(401);
        assert!(!matching_identity(&receipt, "fixture-user", &endpoint));
        receipt.status = Some(200);
        receipt.body_truncated = true;
        assert!(!matching_identity(&receipt, "fixture-user", &endpoint));
        receipt.body_truncated = false;
        receipt.method = "POST".into();
        assert!(!matching_identity(&receipt, "fixture-user", &endpoint));
        receipt.method = "GET".into();
        receipt.metadata_truncated = true;
        assert!(!matching_identity(&receipt, "fixture-user", &endpoint));
        receipt.metadata_truncated = false;
        for body in [
            b"{}".as_slice(),
            b"not json",
            br#"{"kind":"user","id":0,"permalink":"fixture-user"}"#,
        ] {
            receipt.body = Some(body.to_vec());
            assert!(!matching_identity(&receipt, "fixture-user", &endpoint));
        }
        assert_eq!(
            page.evaluate_sync::<u64>("window.submits").await.unwrap(),
            1
        );
        browser.close().await.unwrap();
        server.abort();
    }
}
