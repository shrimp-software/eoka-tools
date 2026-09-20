use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{extract::ExtractedPuzzle, SolveError};
use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GapMatch {
    pub x: u32,
    pub y: u32,
    pub score: f64,
}

const EDGE_THRESHOLD: i32 = 60;
const ALPHA_THRESHOLD: u8 = 40;

pub fn crop_to_alpha(img: &RgbaImage) -> Option<(RgbaImage, u32, u32)> {
    let (w, h) = img.dimensions();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if p.0[3] > ALPHA_THRESHOLD {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x == w || min_y == h {
        return None;
    }
    let cw = max_x - min_x + 1;
    let ch = max_y - min_y + 1;
    Some((
        image::imageops::crop_imm(img, min_x, min_y, cw, ch).to_image(),
        min_x,
        min_y,
    ))
}

fn edge_map(img: &RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let (w_us, h_us) = (w as usize, h as usize);
    let mut gray = vec![0u8; w_us * h_us];
    for (x, y, p) in img.enumerate_pixels() {
        let l = if p.0[3] > ALPHA_THRESHOLD {
            (299u32 * p.0[0] as u32 + 587 * p.0[1] as u32 + 114 * p.0[2] as u32) / 1000
        } else {
            0
        };
        gray[y as usize * w_us + x as usize] = l as u8;
    }
    let mut edges = vec![0u8; w_us * h_us];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y as usize * w_us + x as usize;
            let gx = gray[i - w_us + 1] as i32 + 2 * gray[i + 1] as i32 + gray[i + w_us + 1] as i32
                - gray[i - w_us - 1] as i32
                - 2 * gray[i - 1] as i32
                - gray[i + w_us - 1] as i32;
            let gy =
                gray[i + w_us - 1] as i32 + 2 * gray[i + w_us] as i32 + gray[i + w_us + 1] as i32
                    - gray[i - w_us - 1] as i32
                    - 2 * gray[i - w_us] as i32
                    - gray[i - w_us + 1] as i32;
            let mag = ((gx * gx + gy * gy) as f64).sqrt();
            edges[i] = if mag > EDGE_THRESHOLD as f64 {
                mag.min(255.0) as u8
            } else {
                0
            };
        }
    }
    edges
}

fn integral_squares(data: &[u8], w: u32, h: u32) -> Vec<f64> {
    let (w_us, h_us) = (w as usize, h as usize);
    let mut sat = vec![0f64; (w_us + 1) * (h_us + 1)];
    for y in 0..h_us {
        let mut row = 0f64;
        for x in 0..w_us {
            let v = data[y * w_us + x] as f64;
            row += v * v;
            sat[(y + 1) * (w_us + 1) + x + 1] = sat[y * (w_us + 1) + x + 1] + row;
        }
    }
    sat
}

fn window_energy(sat: &[f64], stride: usize, x: u32, y: u32, w: u32, h: u32) -> f64 {
    let (x2, y2) = (x as usize + w as usize, y as usize + h as usize);
    let (x1, y1) = (x as usize, y as usize);
    sat[y2 * stride + x2] - sat[y1 * stride + x2] - sat[y2 * stride + x1] + sat[y1 * stride + x1]
}

static MATCHER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub(crate) async fn match_puzzle(
    puzzle: ExtractedPuzzle,
) -> Result<(ExtractedPuzzle, Option<GapMatch>), SolveError> {
    let permit = MATCHER
        .acquire()
        .await
        .map_err(|_| SolveError::Puzzle("image matcher unavailable".into()))?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel_on_drop = CancelOnDrop(cancelled.clone());
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let gap = find_gap_until_cancelled(&puzzle.background, &puzzle.piece, &cancelled);
        (puzzle, gap)
    })
    .await
    .map_err(|_| SolveError::Puzzle("image matcher failed".into()))
}

pub fn find_gap(background: &RgbaImage, piece: &RgbaImage) -> Option<GapMatch> {
    find_gap_until_cancelled(background, piece, &AtomicBool::new(false))
}

fn find_gap_until_cancelled(
    background: &RgbaImage,
    piece: &RgbaImage,
    cancelled: &AtomicBool,
) -> Option<GapMatch> {
    if cancelled.load(Ordering::Relaxed) {
        return None;
    }
    let (pw, ph) = piece.dimensions();
    let (bw, bh) = background.dimensions();
    if pw >= bw || ph >= bh || pw == 0 || ph == 0 {
        return None;
    }

    let bg_edges = edge_map(background);
    let piece_edges = edge_map(piece);

    let pw_us = pw as usize;
    let sparse: Vec<(u32, u32, f64)> = piece_edges
        .iter()
        .enumerate()
        .filter(|(_, &v)| v > 0)
        .map(|(i, &v)| ((i % pw_us) as u32, (i / pw_us) as u32, v as f64))
        .collect();
    if sparse.len() < 8 {
        return None;
    }

    let piece_energy: f64 = sparse.iter().map(|&(_, _, v)| v * v).sum();
    let sat = integral_squares(&bg_edges, bw, bh);
    let sat_stride = bw as usize + 1;
    let bw_us = bw as usize;

    let mut best: Option<GapMatch> = None;
    for y in 0..=(bh - ph) {
        for x in 0..=(bw - pw) {
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            let mut num = 0f64;
            for &(px, py, v) in &sparse {
                num += v * bg_edges[(y + py) as usize * bw_us + (x + px) as usize] as f64;
            }
            if num <= 0.0 {
                continue;
            }
            let denom = (piece_energy * window_energy(&sat, sat_stride, x, y, pw, ph)).sqrt();
            if denom <= 0.0 {
                continue;
            }
            let score = num / denom;
            if best.is_none_or(|b| score > b.score) {
                best = Some(GapMatch { x, y, score });
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[tokio::test]
    async fn cancelling_a_match_releases_the_bounded_worker_slot() {
        let background = image::RgbaImage::from_fn(1024, 512, |x, y| {
            image::Rgba([
                (x.wrapping_mul(31) ^ y.wrapping_mul(17)) as u8,
                (x ^ y) as u8,
                y as u8,
                255,
            ])
        });
        let piece = image::imageops::crop_imm(&background, 0, 0, 400, 300).to_image();
        let rect = crate::extract::Rect {
            x: 0.0,
            y: 0.0,
            width: 1024.0,
            height: 512.0,
        };
        let task = tokio::spawn(match_puzzle(ExtractedPuzzle {
            background,
            piece,
            bg_rect: rect,
            piece_rect: rect,
            handle_rect: None,
        }));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while MATCHER.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        task.abort();
        assert!(matches!(task.await, Err(error) if error.is_cancelled()));
        let permit = tokio::time::timeout(std::time::Duration::from_secs(2), MATCHER.acquire())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
    }

    #[test]
    fn cancellation_is_signalled_on_drop() {
        let cancelled = Arc::new(AtomicBool::new(false));
        drop(CancelOnDrop(cancelled.clone()));
        assert!(cancelled.load(Ordering::Relaxed));
        let image = image::RgbaImage::new(100, 100);
        let piece = image::RgbaImage::new(10, 10);
        assert!(find_gap_until_cancelled(&image, &piece, &cancelled).is_none());
    }

    fn noisy_bg(w: u32, h: u32, seed: u32) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        let mut s = seed;
        let mut rand = move || {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            (s >> 16) as u8
        };
        for (x, y, p) in img.enumerate_pixels_mut() {
            let base = ((x * 255 / w.max(1)) as u16 + (y * 128 / h.max(1)) as u16) as u8;
            let n = rand() % 40;
            *p = Rgba([
                base.wrapping_add(n),
                base.wrapping_add(n / 2),
                255 - base,
                255,
            ]);
        }
        img
    }

    fn make_fixture(
        w: u32,
        h: u32,
        gx: u32,
        gy: u32,
        size: u32,
        seed: u32,
    ) -> (RgbaImage, RgbaImage) {
        let mut bg = noisy_bg(w, h, seed);
        let mut piece = RgbaImage::new(size + 8, size + 8);

        let in_piece =
            |dx: u32, dy: u32| -> bool { dx >= 4 && dx < size + 4 && dy >= 4 && dy < size + 4 };

        for dy in 0..size + 8 {
            for dx in 0..size + 8 {
                let bx = gx + dx;
                let by = gy + dy;
                if bx >= w || by >= h {
                    continue;
                }
                if in_piece(dx, dy) {
                    let src = *bg.get_pixel(bx, by);
                    piece.put_pixel(dx, dy, src);
                    let rim = dx == 4 || dx == size + 3 || dy == 4 || dy == size + 3;
                    let c = if rim { 230 } else { src.0[0] / 4 };
                    bg.put_pixel(bx, by, Rgba([c, c, c, 255]));
                }
            }
        }
        (bg, piece)
    }

    fn assert_gap_found(gx: u32, gy: u32, seed: u32) {
        let (bg, piece) = make_fixture(320, 200, gx, gy, 48, seed);
        let m = find_gap(&bg, &piece).expect("gap should be found");
        assert!(
            (m.x as i32 - gx as i32).abs() <= 4,
            "x: got {}, expected {} (score {:.3})",
            m.x,
            gx,
            m.score
        );
        assert!(
            (m.y as i32 - gy as i32).abs() <= 4,
            "y: got {}, expected {} (score {:.3})",
            m.y,
            gy,
            m.score
        );
    }

    #[test]
    fn finds_gap_at_various_positions() {
        for (gx, gy, seed) in [
            (60u32, 70u32, 7u32),
            (150, 40, 42),
            (220, 120, 1337),
            (30, 100, 99),
        ] {
            assert_gap_found(gx, gy, seed);
        }
    }

    #[test]
    fn alpha_crop_rejects_empty_and_transparent_images() {
        for (width, height) in [(0, 0), (0, 8), (8, 0), (8, 8)] {
            assert!(crop_to_alpha(&RgbaImage::new(width, height)).is_none());
        }
    }

    #[test]
    fn alpha_crop_preserves_pixels_and_offsets_at_image_edges() {
        for (x, y) in [(0, 0), (7, 0), (0, 7), (7, 7), (3, 4)] {
            let mut image = RgbaImage::new(8, 8);
            image.put_pixel(x, y, Rgba([10, 20, 30, ALPHA_THRESHOLD + 1]));
            image.put_pixel(1, 1, Rgba([40, 50, 60, ALPHA_THRESHOLD]));
            let (cropped, offset_x, offset_y) = crop_to_alpha(&image).unwrap();
            assert_eq!((offset_x, offset_y), (x, y));
            assert_eq!(cropped.dimensions(), (1, 1));
            assert_eq!(cropped.get_pixel(0, 0), image.get_pixel(x, y));
        }
    }

    #[test]
    fn returns_none_for_fully_transparent_piece() {
        let bg = noisy_bg(200, 120, 1);
        let piece = RgbaImage::new(50, 50);
        assert!(find_gap(&bg, &piece).is_none());
    }

    #[test]
    fn returns_none_when_piece_larger_than_bg() {
        let bg = noisy_bg(60, 60, 1);
        let piece = noisy_bg(100, 100, 2);
        assert!(find_gap(&bg, &piece).is_none());
    }
}
