use eoka::Page;

use serde::Deserialize;

use crate::extract::{ExtractedPuzzle, Rect};
use crate::solve::GapMatch;
use crate::{SliderDrag, SolveError};

#[derive(Deserialize)]
struct SimpleSlider {
    handle: Rect,
    target: Rect,
}

pub async fn simple_geometry(
    page: &Page,
    frame_id: &str,
) -> Result<Option<SliderDrag>, SolveError> {
    let geometry: Option<SimpleSlider> = page.evaluate_in_frame_id(frame_id, r#"(() => {
        const widget = document.querySelector('#captcha__frame.simple');
        if (!widget || !widget.checkVisibility({checkOpacity:true, checkVisibilityCSS:true})) return null;
        const handles = widget.querySelectorAll('.sliderContainer > .slider');
        const targets = widget.querySelectorAll('.sliderContainer > .sliderTarget');
        if (handles.length !== 1 || targets.length !== 1) return null;
        const rect = e => { const r=e.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height}; };
        if (![handles[0], targets[0]].every(e => e.checkVisibility({checkOpacity:true, checkVisibilityCSS:true}))) return null;
        return {handle:rect(handles[0]), target:rect(targets[0])};
    })()"#).await?;
    let Some(geometry) = geometry else {
        return Ok(None);
    };
    let (x, y) = geometry.handle.center();
    let (tx, ty) = geometry.target.center();
    if geometry.handle.width <= 0.0
        || geometry.target.width <= 0.0
        || tx <= x
        || (ty - y).abs() > 5.0
    {
        return Err(SolveError::Puzzle("invalid simple-slider geometry".into()));
    }
    let start = page.frame_point_to_viewport(frame_id, x, y).await?;
    let end = page.frame_point_to_viewport(frame_id, tx, y).await?;
    tracing::debug!(
        dx = end.0 - start.0,
        "datadome: dragging simple slider to its visible target"
    );
    Ok(Some(SliderDrag {
        x: start.0,
        y: start.1,
        dx: end.0 - start.0,
        horizontal_only: true,
    }))
}

pub fn drag_distance(puzzle: &ExtractedPuzzle, gap: &GapMatch) -> f64 {
    let scale = puzzle.bg_rect.width / puzzle.background.width().max(1) as f64;
    gap.x as f64 * scale - (puzzle.piece_rect.x - puzzle.bg_rect.x)
}

pub async fn image_geometry(
    page: &Page,
    frame_id: &str,
    puzzle: &ExtractedPuzzle,
    gap: &GapMatch,
) -> Result<SliderDrag, SolveError> {
    let dx = drag_distance(puzzle, gap);
    if !dx.is_finite() || dx <= 0.0 || dx >= puzzle.bg_rect.width {
        return Err(SolveError::Puzzle(
            "gap implies an invalid drag distance".into(),
        ));
    }
    let handle = puzzle
        .handle_rect
        .as_ref()
        .ok_or_else(|| SolveError::Puzzle("no unambiguous slider handle found".into()))?;
    let (x, y) = handle.center();
    let start = page.frame_point_to_viewport(frame_id, x, y).await?;
    let end = page.frame_point_to_viewport(frame_id, x + dx, y).await?;
    if (start.1 - end.1).abs() > 0.5 {
        return Err(SolveError::Puzzle("non-horizontal frame mapping".into()));
    }
    tracing::debug!(
        dx,
        grab_x = start.0,
        grab_y = start.1,
        gap_x = gap.x,
        score = gap.score,
        "datadome: executing frame-local slider drag"
    );
    Ok(SliderDrag {
        x: start.0,
        y: start.1,
        dx: end.0 - start.0,
        horizontal_only: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puzzle(rendered_w: f64, natural_w: u32, piece_x: f64) -> ExtractedPuzzle {
        ExtractedPuzzle {
            background: image::RgbaImage::new(natural_w, 100),
            piece: image::RgbaImage::new(50, 50),
            bg_rect: Rect {
                x: 100.0,
                y: 50.0,
                width: rendered_w,
                height: 100.0,
            },
            piece_rect: Rect {
                x: piece_x,
                y: 60.0,
                width: 25.0,
                height: 25.0,
            },
            handle_rect: None,
        }
    }

    #[test]
    fn scales_image_pixels_to_css() {
        let p = puzzle(170.0, 340, 100.0);
        let gap = GapMatch {
            x: 120,
            y: 40,
            score: 0.9,
        };
        assert!((drag_distance(&p, &gap) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn subtracts_piece_offset() {
        let p = puzzle(340.0, 340, 110.0);
        let gap = GapMatch {
            x: 120,
            y: 40,
            score: 0.9,
        };
        assert!((drag_distance(&p, &gap) - 110.0).abs() < 1e-9);
    }

    #[test]
    fn preserves_invalid_distance_for_validation_instead_of_clamping() {
        let p = puzzle(340.0, 340, 200.0);
        let gap = GapMatch {
            x: 50,
            y: 40,
            score: 0.9,
        };
        assert_eq!(drag_distance(&p, &gap), -50.0);
    }
}
