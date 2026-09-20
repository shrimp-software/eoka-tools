use std::io::Cursor;

use base64::Engine;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Fixture {
    pub url: String,
    pub started: std::sync::Arc<tokio::sync::Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn image_data(img: RgbaImage) -> String {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    )
}

fn images() -> (String, String) {
    let mut bg = RgbaImage::new(320, 200);
    let mut seed = 42u32;
    for (x, y, p) in bg.enumerate_pixels_mut() {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let base = (x * 255 / 320 + y * 128 / 200) as u8;
        let n = (seed >> 16) as u8 % 40;
        *p = Rgba([
            base.wrapping_add(n),
            base.wrapping_add(n / 2),
            255 - base,
            255,
        ]);
    }
    let mut piece = RgbaImage::new(56, 56);
    for y in 4..52 {
        for x in 4..52 {
            let pixel = *bg.get_pixel(150 + x, 60 + y);
            piece.put_pixel(x, y, pixel);
            let rim = x == 4 || x == 51 || y == 4 || y == 51;
            let c = if rim { 230 } else { pixel.0[0] / 4 };
            bg.put_pixel(150 + x, 60 + y, Rgba([c, c, c, 255]));
        }
    }
    (image_data(bg), image_data(piece))
}

impl Fixture {
    pub async fn start(blocked: bool, simple: bool, reblock: bool) -> Self {
        Self::scenario(if blocked {
            "blocked"
        } else if reblock {
            "reblock"
        } else if simple {
            "simple"
        } else {
            "image"
        })
        .await
    }

    pub async fn scenario(mode: &str) -> Self {
        assert!([
            "blocked",
            "reblock",
            "simple",
            "image",
            "absent",
            "banned",
            "unsupported",
            "timeout",
            "top-block",
            "auth-block",
            "mirrored",
            "many-images",
            "shadow-open",
            "shadow-closed",
            "shadow-positive"
        ]
        .contains(&mode));
        let blocked = mode == "blocked";
        let reblock = mode == "reblock";
        let simple = ["simple", "reblock", "timeout", "top-block", "auth-block"].contains(&mode);
        let top_block = mode == "top-block";
        let auth_block = mode == "auth-block";
        let mirrored = mode == "mirrored";
        let many_images = mode == "many-images";
        let shadow = mode.starts_with("shadow-");
        let shadow_closed = mode == "shadow-closed";
        let shadow_reflected = mode != "shadow-positive";
        let absent = mode == "absent";
        let banned = mode == "banned";
        let unsupported = mode == "unsupported";
        let timeout = mode == "timeout";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (mut bg, piece) = images();
        if many_images {
            bg = image_data(RgbaImage::from_pixel(1024, 512, Rgba([80, 90, 100, 255])));
        }
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let request_started = started.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let (bg, piece) = (bg.clone(), piece.clone());
                let request_started = request_started.clone();
                tokio::spawn(async move {
                    let mut buffer = [0u8; 8192];
                    let n = socket.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..n]);
                    let path = request.split_whitespace().nth(1).unwrap_or("/");
                    let body = if path.starts_with("/captcha") {
                        if unsupported {
                            "<h1>Unsupported challenge</h1><canvas></canvas>".to_string()
                        } else if blocked || path.contains("block=1") {
                            "<h1>You have been blocked</h1>".to_string()
                        } else if simple {
                            format!(
                                r#"<style>body{{margin:0}} .sliderContainer{{position:relative;left:10px;top:80px;width:280px;height:40px}} .slider,.sliderTarget{{position:absolute;top:0;width:63px;height:40px}} .slider{{left:0;background:#ccc}} .sliderTarget{{left:222px}}</style>
                            <div id="captcha__frame" class="simple"><div class="sliderContainer"><div class="slider">drag</div><div class="sliderTarget">target</div></div></div>
                            <script>let start;document.querySelector('.slider').onmousedown=e=>{{start=e.clientX;parent.postMessage('fixture-pressed','http://auth.test:{port}')}};
                            document.onmouseup=e=>{{parent.postMessage('fixture-released','http://auth.test:{port}');if(!{timeout} && start!==undefined && Math.abs(e.clientX-start-222)<=4)parent.postMessage('fixture-solved','http://auth.test:{port}')}};</script>"#
                            )
                        } else {
                            format!(
                                r#"<style>body{{margin:0}} img{{position:absolute}} #handle{{position:absolute;left:10px;top:220px;width:40px;height:40px}}</style>
                                <img src="{bg}" style="left:10px;top:10px;width:320px;height:200px">
                                <img src="{piece}" style="left:10px;top:70px;width:56px;height:56px">
                                <button id="handle" class="handle">drag</button>
                                <script>
                                if ({mirrored}) for(const img of document.images) img.style.transform='scaleX(-1)';
                                if ({many_images}) for(let i=0;i<200;i++) document.body.appendChild(document.images[0].cloneNode());
                                if ({shadow}) for(const img of Array.from(document.images)) {{
                                    const host=document.createElement('div');
                                    host.style.cssText=`position:absolute;left:${{img.style.left}};top:${{img.style.top}};width:${{img.style.width}};height:${{img.style.height}}`;
                                    img.before(host); host.appendChild(img); img.style.left='0px'; img.style.top='0px';
                                    const root=host.attachShadow({{mode:{shadow_closed}?'closed':'open'}});
                                    root.innerHTML='<div style="width:100%;height:100%;transform:'+({shadow_reflected}?'scaleX(-1)':'scale(1)')+'"><slot></slot></div>';
                                }}
                                let start;
                                document.getElementById('handle').onmousedown=e=>{{start=e.clientX;document.body.dataset.pressed='true';parent.postMessage('fixture-pressed','http://auth.test:{port}')}};
                                document.onmouseup=e=>{{document.body.dataset.dx=String(e.clientX-start);if(start!==undefined && Math.abs(e.clientX-start-150)<=4)
                                    parent.postMessage('fixture-solved','http://auth.test:{port}');}};
                                </script>"#
                            )
                        }
                    } else if path == "/accepted" {
                        request_started.notify_one();
                        "accepted".to_string()
                    } else if path.starts_with("/login") {
                        let block = if path.contains("block=1") {
                            "&block=1"
                        } else {
                            ""
                        };
                        let challenge_type = if banned { "bv" } else { "fe" };
                        format!(
                            r#"<style>body{{margin:0}} iframe{{position:absolute;left:10px;top:10px;width:400px;height:300px;border:2px solid black}}</style>
                            <iframe src="http://geo.captcha-delivery.com:{port}/captcha/?t={challenge_type}{block}"></iframe>
                            <script>onmessage=e=>{{if(e.origin==='http://geo.captcha-delivery.com:{port}' && ['fixture-solved','fixture-pressed','fixture-released'].includes(e.data)) {{
                                if ({auth_block} && e.data==='fixture-solved') document.body.innerHTML='<h1>You have been blocked</h1>';
                                parent.postMessage(e.data,'http://app.test:{port}');}}}}</script>"#
                        )
                    } else {
                        let frame = if absent {
                            String::new()
                        } else {
                            format!(r#"<iframe src="http://auth.test:{port}/login"></iframe>"#)
                        };
                        format!(
                            r#"<style>iframe{{position:absolute;left:100px;top:100px;width:500px;height:400px;border:3px solid black}}</style>
                            {frame}<button id="continue" style="position:absolute;top:550px" onclick="document.body.dataset.continued='true'">Continue</button>
                            <script>window.fixtureMarker={port};sessionStorage.setItem('fixture','retained');localStorage.setItem('fixture','retained');document.cookie='other=retained; path=/';
                            onmessage=e=>{{document.body.dataset.message=e.data;if(e.origin!=='http://auth.test:{port}')return;if(e.data==='fixture-pressed'){{document.body.dataset.pressed='true';fetch('/accepted').then(()=>document.body.dataset.fetched='true');}}if(e.data==='fixture-released')document.body.dataset.released='true';if(e.data==='fixture-solved'){{
                                if(document.body.dataset.fetched!=='true')return;
                                document.cookie='datadome=fixture-only-{port}; path=/';
                                if (!{auth_block}) document.querySelector('iframe').remove();
                                if ({top_block}) document.body.insertAdjacentHTML('beforeend','<h1>You have been blocked</h1>');
                                document.body.dataset.restored='true';
                                if ({reblock}) setTimeout(()=>{{const f=document.createElement('iframe');f.src='http://auth.test:{port}/login?block=1';document.body.appendChild(f);}},800);
                            }}}}</script>"#
                        )
                    };
                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            url: format!("http://app.test:{port}/"),
            started,
            task,
        }
    }
}
