use base64::Engine;
use eoka::Page;
use image::RgbaImage;
use serde::Deserialize;

use crate::SolveError;

type Result<T> = std::result::Result<T, SolveError>;

#[derive(Clone, Copy, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

pub struct ExtractedPuzzle {
    pub background: RgbaImage,
    pub piece: RgbaImage,
    pub bg_rect: Rect,
    pub piece_rect: Rect,
    pub handle_rect: Option<Rect>,
}

#[derive(Deserialize)]
struct ImgInfo {
    index: usize,
    data_url: String,
    alpha_frac: f64,
    rect: Rect,
}

#[derive(Deserialize)]
struct ExtractPayload {
    imgs: Vec<ImgInfo>,
    handle: Option<Rect>,
    canvas_count: usize,
    tainted_count: usize,
    oversized_count: usize,
    error: Option<String>,
}

const EXTRACT_JS: &str = r#"(() => {
  let tainted_count = 0, oversized_count = 0;
  const toData = (img) => {
    const c = document.createElement('canvas');
    c.width = img.naturalWidth; c.height = img.naturalHeight;
    const ctx = c.getContext('2d');
    try {
      ctx.drawImage(img, 0, 0);
      const d = ctx.getImageData(0, 0, c.width, c.height).data;
      let transparent = 0;
      for (let i = 3; i < d.length; i += 16) if (d[i] < 40) transparent++;
      return { data_url: c.toDataURL('image/png'), alpha_frac: transparent / Math.ceil(d.length / 16) };
    } catch (e) { tainted_count++; return null; }
  };
  const rectOf = (el) => { const r = el.getBoundingClientRect(); return { x:r.x, y:r.y, width:r.width, height:r.height }; };
  const fail = error => ({imgs:[],handle:null,canvas_count:0,tainted_count,oversized_count,error});
  const supported = img => {
    const style = getComputedStyle(img);
    if (style.objectFit !== 'fill' || style.objectPosition !== '50% 50%' ||
        ['borderLeftWidth','borderRightWidth','borderTopWidth','borderBottomWidth','paddingLeft','paddingRight','paddingTop','paddingBottom'].some(k => parseFloat(style[k]) !== 0)) return false;
    let depth = 0;
    for (let e = img; e; e = e.assignedSlot || e.parentElement || e.getRootNode().host) {
      if (++depth > 64) return false;
      const s = getComputedStyle(e);
      if (s.perspective !== 'none' || s.rotate !== 'none' || s.offsetPath !== 'none') return false;
      if (s.scale !== 'none' && !s.scale.split(/\s+/).every(n => Number(n) > 0)) return false;
      if (s.transform !== 'none') {
        const m = new DOMMatrixReadOnly(s.transform);
        if (!m.is2D || m.a <= 0 || m.d <= 0 || Math.abs(m.b) > 1e-8 || Math.abs(m.c) > 1e-8) return false;
      }
    }
    return true;
  };
  const elements = document.querySelectorAll('img');
  if (elements.length > 64) return fail('image element limit exceeded');
  const readable = [];
  let pixels = 0;
  for (const i of elements) {
    if (i.naturalWidth > 1024 || i.naturalHeight > 512) { oversized_count++; continue; }
    const r = i.getBoundingClientRect();
    if (!i.complete || i.naturalWidth < 40 || i.naturalHeight < 40 || r.width <= 10 || r.height <= 10) continue;
    pixels += i.naturalWidth * i.naturalHeight;
    readable.push(i);
    if (readable.length > 8 || pixels > 1048576) return fail('image extraction budget exceeded');
    if (!supported(i)) return fail('unsupported image placement');
  }
  const imgs = [];
  let encoded = 0;
  for (const i of readable) {
    const data = toData(i);
    if (!data) continue;
    encoded += data.data_url.length;
    if (data.data_url.length > 3145728 || encoded > 6291456) return fail('encoded image budget exceeded');
    imgs.push({...data, rect:rectOf(i), index:Array.prototype.indexOf.call(elements,i)});
  }
  const candidates = [...document.querySelectorAll('[role="slider"], [class*="handle"], [class*="slider"], [class*="drag"], [class*="knob"]')]
    .filter(e => { const r = e.getBoundingClientRect(); const s = getComputedStyle(e);
      return r.width >= 10 && r.width <= 80 && r.height >= 10 && r.height <= 80 && s.visibility === 'visible'; });
  const handle = candidates.length === 1 ? rectOf(candidates[0]) : null;
  return { imgs, handle, canvas_count: document.querySelectorAll('canvas').length, tainted_count, oversized_count };
})()"#;

pub async fn extract(page: &Page, frame_id: &str) -> Result<Option<ExtractedPuzzle>> {
    let payload: ExtractPayload = page.evaluate_in_frame_id(frame_id, EXTRACT_JS).await?;
    tracing::debug!(
        images = payload.imgs.len(),
        canvases = payload.canvas_count,
        tainted = payload.tainted_count,
        oversized = payload.oversized_count,
        handle = payload.handle.is_some(),
        "datadome: frame extraction"
    );
    if let Some(error) = payload.error {
        return Err(SolveError::Puzzle(error));
    }
    if payload.imgs.len() > 8
        || payload
            .imgs
            .iter()
            .any(|i| i.data_url.len() > 3 * 1024 * 1024)
        || payload.imgs.iter().map(|i| i.data_url.len()).sum::<usize>() > 6 * 1024 * 1024
    {
        return Err(SolveError::Puzzle("image payload budget exceeded".into()));
    }
    let mut pieces: Vec<&ImgInfo> = payload
        .imgs
        .iter()
        .filter(|i| i.alpha_frac > 0.10 && i.alpha_frac < 0.95)
        .collect();
    let mut bgs: Vec<&ImgInfo> = payload
        .imgs
        .iter()
        .filter(|i| i.alpha_frac <= 0.10)
        .collect();
    pieces.sort_by(|a, b| area(b).total_cmp(&area(a)));
    bgs.sort_by(|a, b| area(b).total_cmp(&area(a)));
    let (Some(piece), Some(bg)) = (pieces.first(), bgs.first()) else {
        return Ok(None);
    };
    for image in [*bg, *piece] {
        let quad = page
            .frame_element_content_quad(frame_id, "img", image.index)
            .await?;
        if !positive_axis_aligned_quad(&quad) {
            return Err(SolveError::Puzzle("unsupported image placement".into()));
        }
    }
    Ok(Some(ExtractedPuzzle {
        background: decode_image(bg)?,
        piece: decode_image(piece)?,
        bg_rect: bg.rect,
        piece_rect: piece.rect,
        handle_rect: payload.handle,
    }))
}

fn positive_axis_aligned_quad(q: &[f64; 8]) -> bool {
    q.iter().all(|v| v.is_finite())
        && q[2] > q[0]
        && q[5] > q[3]
        && (q[1] - q[3]).abs() < 1e-6
        && (q[2] - q[4]).abs() < 1e-6
        && (q[5] - q[7]).abs() < 1e-6
        && (q[0] - q[6]).abs() < 1e-6
}

fn area(i: &ImgInfo) -> f64 {
    i.rect.width * i.rect.height
}

fn decode_image(info: &ImgInfo) -> Result<RgbaImage> {
    if info.data_url.len() > 3 * 1024 * 1024 {
        return Err(SolveError::Puzzle("encoded image limit exceeded".into()));
    }
    let b64 = info
        .data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| SolveError::Puzzle("unsupported image encoding".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| SolveError::Puzzle(format!("base64 decode failed: {e}")))?;
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader
        .decode()
        .map_err(|e| SolveError::Puzzle(format!("image decode failed: {e}")))?
        .to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(data_url: String) -> ImgInfo {
        ImgInfo {
            index: 0,
            data_url,
            alpha_frac: 0.0,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        }
    }

    #[test]
    fn encoded_and_decoded_image_limits_are_enforced() {
        assert!(decode_image(&info("x".repeat(3 * 1024 * 1024 + 1))).is_err());
        for (width, height, accepted) in [(1024, 512, true), (1025, 1, false), (1, 513, false)] {
            let image = RgbaImage::new(width, height);
            let mut bytes = std::io::Cursor::new(Vec::new());
            image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
            let data = format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
            );
            assert_eq!(decode_image(&info(data)).is_ok(), accepted);
        }
        assert!(decode_image(&info("data:image/jpeg;base64,AA==".into())).is_err());
    }
}
