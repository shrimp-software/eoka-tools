#[cfg(test)]
#[path = "click_tests.rs"]
mod tests;

use std::sync::atomic::{AtomicU64, Ordering};

use eoka::{Error, Page, Result};
use serde_json::{json, Value};

static NEXT_GUARD: AtomicU64 = AtomicU64::new(0);

const SELECT_TARGET: &str = r#"(() => {
    const {selector, text, key} = __ARGS__;
    const matches = document.querySelectorAll(selector);
    if (matches.length > 64) return null;
    const visible = e => !e.matches(':disabled') && e.getAttribute('aria-disabled') !== 'true'
        && !e.closest('[inert]') && e.checkVisibility({checkOpacity:true, checkVisibilityCSS:true})
        && (text === null || e.textContent.trim() === text);
    const candidates = [...matches].filter(e => {
        if (!visible(e)) return false;
        const r = e.getBoundingClientRect(), x = r.left+r.width/2, y = r.top+r.height/2;
        const hit = document.elementFromPoint(x,y);
        return r.width > 0 && r.height > 0 && x >= 0 && y >= 0 && x < innerWidth && y < innerHeight
            && hit && (hit === e || e.contains(hit));
    });
    if (candidates.length !== 1) return null;
    const element = candidates[0], rect = element.getBoundingClientRect();
    const point = [rect.left+rect.width/2, rect.top+rect.height/2];
    const dispose = () => { clearTimeout(timer); delete globalThis[key]; };
    const timer = setTimeout(dispose,5000);
    globalThis[key] = {
        dispose,
        check() {
            try {
                if (!element.isConnected || !visible(element)) return false;
                const r = element.getBoundingClientRect(), hit = document.elementFromPoint(...point);
                return r.left+r.width/2 === point[0] && r.top+r.height/2 === point[1]
                    && hit && (hit === element || element.contains(hit));
            } finally { dispose(); }
        }
    };
    return point;
})()"#;

pub async fn click_visible(
    page: &Page,
    frame_id: Option<&str>,
    selector: &str,
    text: Option<&str>,
) -> Result<()> {
    let unavailable = || Error::ElementNotVisible {
        selector: selector.into(),
    };
    if selector.len() > 4096 {
        return Err(unavailable());
    }
    let root: Value;
    let frame_id = match frame_id {
        Some(id) => id,
        None => {
            root = page.session().send("Page.getFrameTree", &json!({})).await?;
            root["frameTree"]["frame"]["id"]
                .as_str()
                .ok_or_else(unavailable)?
        }
    };
    let key = format!(
        "__eoka_click_guard_{}",
        NEXT_GUARD.fetch_add(1, Ordering::Relaxed)
    );
    let args = json!({"selector":selector,"text":text,"key":key});
    let script = SELECT_TARGET.replace("__ARGS__", &args.to_string());
    let point: Option<[f64; 2]> = page.evaluate_in_frame_id(frame_id, &script).await?;
    let [x, y] = point.ok_or_else(unavailable)?;
    let guard = format!("globalThis[{}]", serde_json::to_string(&key)?);
    let result = async {
        let position = page.frame_point_for_input(frame_id,x,y).await?;
        page.mouse_move(position.0,position.1).await?;
        page.evaluate::<bool>("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve(true))))").await?;
        if page.frame_point_for_input(frame_id,x,y).await? != position {
            return Err(unavailable());
        }
        let ready: bool = page.evaluate_in_frame_id(frame_id, &format!("Boolean({guard}?.check())")).await?;
        if !ready { return Err(unavailable()); }
        page.click_at(position.0,position.1).await
    }.await;
    if result.is_err() {
        let _ = page
            .evaluate_in_frame_id::<bool>(frame_id, &format!("({guard}?.dispose(),true)"))
            .await;
    }
    result
}
