use super::*;
use eoka::Browser;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    url: String,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn start(isolated: bool) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let middle = if isolated { "b.test" } else { "a.test" };
        let inner = if isolated { "c.test" } else { "a.test" };
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buffer = [0; 4096];
                    let n = stream.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..n]);
                    let body=match request.split_whitespace().nth(1).unwrap_or("/") {
                        "/" => format!(r#"<iframe style="position:absolute;left:30px;top:40px;width:500px;height:400px;border:2px solid" src="http://{middle}:{port}/middle"></iframe>"#),
                        "/middle" => format!(r#"<iframe style="position:absolute;left:20px;top:25px;width:300px;height:200px;border:3px solid" src="http://{inner}:{port}/inner"></iframe>"#),
                        _ => r#"<button id=target style="position:absolute;left:30px;top:35px;width:100px;height:40px"><span>Continue</span></button><script>document.body.dataset.clicks=0;document.body.dataset.downs=0;document.addEventListener('mousedown',()=>document.body.dataset.downs++);target.onclick=e=>{document.body.dataset.clicks++;document.body.dataset.trusted=e.isTrusted};</script>"#.to_owned()
                    };
                    let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            url: format!("http://a.test:{port}/"),
            server,
        }
    }
}

#[tokio::test]
#[ignore = "requires Chrome; disposable local fixtures"]
async fn nested_clicks_reject_ancestor_overlays() {
    for isolated in [false, true] {
        let fixture = Fixture::start(isolated).await;
        let browser = Browser::launch_with(|config| {
            config.extra_args.extend([
                "--host-resolver-rules=MAP *.test 127.0.0.1".into(),
                "--no-proxy-server".into(),
            ]);
            if isolated {
                config.extra_args.extend([
                    "--disable-features=AutomationControlled,EnableAutomation".into(),
                    "--site-per-process".into(),
                ]);
            }
        })
        .await
        .unwrap();
        let page = browser.new_page(&fixture.url).await.unwrap();
        let frames = page.frames().await.unwrap();
        let child = frames.iter().find(|f| f.url.ends_with("/inner")).unwrap();
        click_visible(&page, Some(&child.id), "#target", Some("Continue"))
            .await
            .unwrap();
        let count: String = page
            .evaluate_in_frame_id(&child.id, "document.body.dataset.clicks")
            .await
            .unwrap();
        assert_eq!(count, "1");
        let trusted: String = page
            .evaluate_in_frame_id(&child.id, "document.body.dataset.trusted")
            .await
            .unwrap();
        assert_eq!(trusted, "true");
        for ancestor in frames.iter().filter(|f| f.id != child.id) {
            page.evaluate_in_frame_id::<bool>(&ancestor.id,r#"(() => {const cover=document.createElement('div');cover.id='cover';cover.style='position:fixed;inset:0;z-index:10000';document.body.appendChild(cover);return true})()"#).await.unwrap();
            assert!(
                click_visible(&page, Some(&child.id), "#target", None)
                    .await
                    .is_err(),
                "isolated={isolated}, ancestor={}",
                ancestor.url
            );
            let count: String = page
                .evaluate_in_frame_id(&child.id, "document.body.dataset.downs")
                .await
                .unwrap();
            assert_eq!(count, "1");
            page.evaluate_in_frame_id::<bool>(
                &ancestor.id,
                "(document.querySelector('#cover').remove(),true)",
            )
            .await
            .unwrap();
        }
        click_visible(&page, Some(&child.id), "#target", None)
            .await
            .unwrap();
        browser.close().await.unwrap();
    }
}

async fn top_fixture(browser: &Browser) -> Page {
    let page = browser.new_blank_page().await.unwrap();
    page.execute_sync(r#"document.body.innerHTML='<button id="target" class="pick" style="position:absolute;left:30.25px;top:40.25px;width:2px;height:2px;padding:0;border:0;font-size:0"><span>Continue</span></button>';window.clicks=[];window.downs=0;document.addEventListener('mousedown',()=>downs++);target.onclick=e=>clicks.push({x:e.clientX,y:e.clientY,trusted:e.isTrusted});"#).await.unwrap();
    page
}

#[tokio::test]
#[ignore = "requires Chrome; disposable local fixtures"]
async fn tiny_targets_use_the_checked_point_and_reject_unavailable_matches() {
    let browser = Browser::launch().await.unwrap();
    let page = top_fixture(&browser).await;
    for _ in 0..12 {
        click_visible(&page, None, "#target", Some("Continue"))
            .await
            .unwrap();
    }
    let clicks: Vec<Value> = page.evaluate_sync("clicks").await.unwrap();
    assert_eq!(clicks.len(), 12);
    assert!(clicks.iter().all(|click| click["trusted"] == true
        && (click["x"].as_f64().unwrap() - 31.25).abs() < 0.5
        && (click["y"].as_f64().unwrap() - 41.25).abs() < 0.5));
    for script in [
        "target.disabled=true",
        "target.style.display='none'",
        "target.setAttribute('aria-disabled','true')",
        "document.body.inert=true",
        "const other=target.cloneNode(true);other.id='other';other.style.left='100px';document.body.appendChild(other)",
    ] {
        let page=top_fixture(&browser).await;
        page.execute_sync(script).await.unwrap();
        assert!(click_visible(&page,None,".pick",None).await.is_err(),"{script}");
        assert_eq!(page.evaluate_sync::<u32>("downs").await.unwrap(),0);
    }
    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome; disposable local fixtures"]
async fn hover_mutations_are_rejected_without_pressing_or_retargeting() {
    let browser = Browser::launch().await.unwrap();
    for mutation in [
        "target.style.left='80px'",
        "target.replaceWith(target.cloneNode(true))",
        "const cover=document.createElement('div');cover.style='position:fixed;inset:0;z-index:10000';document.body.appendChild(cover)",
    ] {
        let page=top_fixture(&browser).await;
        page.mouse_move(0.0,0.0).await.unwrap();
        page.execute_sync(&format!("target.addEventListener('mouseenter',()=>{{{mutation}}},{{once:true}})")).await.unwrap();
        assert!(click_visible(&page,None,"#target",None).await.is_err(),"{mutation}");
        assert_eq!(page.evaluate_sync::<u32>("downs").await.unwrap(),0);
        let root=page.frames().await.unwrap().remove(0);
        assert_eq!(page.evaluate_in_frame_id::<usize>(&root.id,"Object.keys(globalThis).filter(key=>key.startsWith('__eoka_click_guard_')).length").await.unwrap(),0);
    }
    browser.close().await.unwrap();
}
