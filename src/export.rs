use crate::draft::PostDraft;
use ab_glyph::{FontArc, PxScale};
use anyhow::{Context, Result, anyhow};
use cranpose::ImageBitmap;
use fontdb::{Database, Family, Query, Style, Weight};
use image::imageops::{FilterType, overlay};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_circle_mut, draw_filled_rect_mut, draw_text_mut, text_size};
use imageproc::rect::Rect;
use std::fs;
use std::path::{Path, PathBuf};

const CANVAS_WIDTH: u32 = 1600;
const CANVAS_HEIGHT: u32 = 900;

#[derive(Clone)]
pub struct PreviewState {
    pub bitmap: ImageBitmap,
    pub preview_png_path: String,
    pub last_saved_webp_path: Option<String>,
}

impl PreviewState {
    pub fn placeholder() -> Self {
        let bitmap = ImageBitmap::from_rgba8(1, 1, vec![8, 12, 18, 255]).expect("placeholder");
        Self {
            bitmap,
            preview_png_path: String::new(),
            last_saved_webp_path: None,
        }
    }
}

pub fn generate_preview(draft: &PostDraft) -> Result<PreviewState> {
    let output_dir = output_dir();
    fs::create_dir_all(&output_dir).context("creating output directory")?;

    let rendered = compose_card(draft)?;
    let preview_png_path = output_dir.join("preview-latest.png");
    rendered
        .save(&preview_png_path)
        .with_context(|| format!("saving preview to {}", preview_png_path.display()))?;

    let bitmap = image_bitmap_from(&rendered)?;

    Ok(PreviewState {
        bitmap,
        preview_png_path: preview_png_path.display().to_string(),
        last_saved_webp_path: None,
    })
}

pub fn save_webp(draft: &PostDraft) -> Result<PreviewState> {
    let output_dir = output_dir();
    fs::create_dir_all(&output_dir).context("creating output directory")?;

    let rendered = compose_card(draft)?;
    let preview_png_path = output_dir.join("preview-latest.png");
    rendered
        .save(&preview_png_path)
        .with_context(|| format!("saving preview to {}", preview_png_path.display()))?;

    let export_path = draft.suggested_export_path();
    DynamicImage::ImageRgba8(rendered.clone())
        .save_with_format(&export_path, ImageFormat::WebP)
        .with_context(|| format!("saving WebP to {}", export_path.display()))?;

    let bitmap = image_bitmap_from(&rendered)?;

    Ok(PreviewState {
        bitmap,
        preview_png_path: preview_png_path.display().to_string(),
        last_saved_webp_path: Some(export_path.display().to_string()),
    })
}

fn compose_card(draft: &PostDraft) -> Result<RgbaImage> {
    let assets = AssetPack::load()?;
    let canvas =
        assets
            .background
            .resize_to_fill(CANVAS_WIDTH, CANVAS_HEIGHT, FilterType::Lanczos3);
    let mut canvas = canvas.to_rgba8();

    let panel = BoxArea::new(104, 112, 1392, 660);
    let left_code = BoxArea::new(panel.x + 54, panel.y + 80, 612, 374);
    let right_code = BoxArea::new(panel.x + 726, panel.y + 80, 612, 374);
    let tldr_box = BoxArea::new(panel.x + 54, panel.y + 486, 1284, 134);

    let mut overlay_layer = RgbaImage::new(CANVAS_WIDTH, CANVAS_HEIGHT);
    fill_rounded_box(&mut overlay_layer, panel, 46, rgba(5, 8, 14, 210));
    fill_rounded_box(&mut overlay_layer, left_code, 28, rgba(16, 22, 34, 185));
    fill_rounded_box(&mut overlay_layer, right_code, 28, rgba(16, 22, 34, 185));
    fill_rounded_box(&mut overlay_layer, tldr_box, 28, rgba(19, 26, 40, 205));
    overlay(&mut canvas, &overlay_layer, 0, 0);

    let qr = tint_alpha(
        &assets
            .qr
            .resize_exact(170, 170, FilterType::Lanczos3)
            .to_rgba8(),
        0.72,
    );
    overlay(&mut canvas, &qr, 26, 26);

    let code_scale = PxScale::from(24.0);
    let label_scale = PxScale::from(28.0);
    let badge_scale = PxScale::from(32.0);
    let tldr_scale = PxScale::from(30.0);

    draw_code_panel(
        &mut canvas,
        &assets,
        left_code,
        "kotlin",
        &draft.kotlin_runtime_ms,
        &draft.kotlin_code,
        code_scale,
        label_scale,
    );
    draw_code_panel(
        &mut canvas,
        &assets,
        right_code,
        "rust",
        &draft.rust_runtime_ms,
        &draft.rust_code,
        code_scale,
        label_scale,
    );

    let badge_color = rgba(169, 255, 232, 255);
    let body_color = rgba(246, 249, 252, 255);
    let accent_color = rgba(255, 180, 78, 255);

    draw_text_mut(
        &mut canvas,
        badge_color,
        tldr_box.x + 28,
        tldr_box.y + 22,
        badge_scale,
        &assets.sans_font,
        &draft.preview_badge(),
    );
    draw_wrapped_lines(
        &mut canvas,
        body_color,
        tldr_box.x + 28,
        tldr_box.y + 66,
        tldr_box.width.saturating_sub(56),
        tldr_scale,
        &assets.sans_font,
        &wrap_paragraph(
            &draft.preview_tldr(),
            &assets.sans_font,
            tldr_scale,
            tldr_box.width.saturating_sub(56),
        ),
        40,
    );
    draw_text_mut(
        &mut canvas,
        accent_color,
        panel.x + 54,
        panel.y + 34,
        PxScale::from(38.0),
        &assets.sans_font,
        &draft.preview_title(),
    );

    Ok(canvas)
}

fn draw_code_panel(
    canvas: &mut RgbaImage,
    assets: &AssetPack,
    area: BoxArea,
    language: &str,
    runtime_ms: &str,
    code: &str,
    code_scale: PxScale,
    label_scale: PxScale,
) {
    let label_color = rgba(148, 229, 255, 255);
    let runtime_color = rgba(255, 180, 78, 255);
    let code_color = rgba(242, 246, 250, 255);

    let title = format!("// {language}");
    let runtime = format!("// {}", runtime_label(runtime_ms));

    draw_text_mut(
        canvas,
        label_color,
        area.x + 28,
        area.y + 22,
        label_scale,
        &assets.sans_font,
        &title,
    );
    draw_text_mut(
        canvas,
        runtime_color,
        area.x + 28,
        area.y + 58,
        label_scale,
        &assets.sans_font,
        &runtime,
    );

    let code_y = area.y + 112;
    let code_width = area.width.saturating_sub(56);
    let max_chars = estimate_monospace_chars(code_width, 24.0);
    let code_lines = wrap_code(code, max_chars);
    draw_wrapped_lines(
        canvas,
        code_color,
        area.x + 28,
        code_y,
        code_width,
        code_scale,
        &assets.mono_font,
        &code_lines,
        34,
    );
}

fn draw_wrapped_lines(
    canvas: &mut RgbaImage,
    color: Rgba<u8>,
    x: i32,
    y: i32,
    _max_width: u32,
    scale: PxScale,
    font: &FontArc,
    lines: &[String],
    line_height: i32,
) {
    for (index, line) in lines.iter().enumerate() {
        draw_text_mut(
            canvas,
            color,
            x,
            y + line_height * index as i32,
            scale,
            font,
            line,
        );
    }
}

fn wrap_paragraph(text: &str, font: &FontArc, scale: PxScale, max_width: u32) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    for paragraph in text.lines() {
        if paragraph.trim().is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            let (width, _) = text_size(scale, font, &candidate);
            if width > max_width && !current.is_empty() {
                lines.push(current);
                current = word.to_string();
            } else {
                current = candidate;
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn wrap_code(code: &str, max_chars: usize) -> Vec<String> {
    if code.trim().is_empty() {
        return vec!["// paste code here".to_string()];
    }

    let mut out = Vec::new();
    for raw_line in code.lines() {
        if raw_line.is_empty() {
            out.push(String::new());
            continue;
        }

        let indent = raw_line.chars().take_while(|ch| ch.is_whitespace()).count();
        let prefix = " ".repeat(indent);
        let trimmed = raw_line.trim_start();
        let allowed = max_chars.saturating_sub(indent.max(1)).max(8);

        if trimmed.chars().count() <= allowed {
            out.push(raw_line.to_string());
            continue;
        }

        let mut current = String::new();
        for chunk in trimmed.chars() {
            current.push(chunk);
            if current.chars().count() >= allowed {
                out.push(format!("{prefix}{current}"));
                current.clear();
            }
        }

        if !current.is_empty() {
            out.push(format!("{prefix}{current}"));
        }
    }
    out
}

fn estimate_monospace_chars(width: u32, font_size: f32) -> usize {
    let avg_char_width = (font_size * 0.60).max(1.0);
    ((width as f32) / avg_char_width).floor().max(8.0) as usize
}

fn fill_rounded_box(image: &mut RgbaImage, area: BoxArea, radius: i32, color: Rgba<u8>) {
    let rect_w = area.width as i32;
    let rect_h = area.height as i32;
    let x = area.x;
    let y = area.y;

    draw_filled_rect_mut(
        image,
        Rect::at(x + radius, y).of_size((rect_w - 2 * radius) as u32, area.height),
        color,
    );
    draw_filled_rect_mut(
        image,
        Rect::at(x, y + radius).of_size(area.width, (rect_h - 2 * radius) as u32),
        color,
    );
    draw_filled_circle_mut(image, (x + radius, y + radius), radius, color);
    draw_filled_circle_mut(image, (x + rect_w - radius, y + radius), radius, color);
    draw_filled_circle_mut(image, (x + radius, y + rect_h - radius), radius, color);
    draw_filled_circle_mut(
        image,
        (x + rect_w - radius, y + rect_h - radius),
        radius,
        color,
    );
}

fn tint_alpha(image: &RgbaImage, opacity: f32) -> RgbaImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        pixel[3] = ((pixel[3] as f32) * opacity.clamp(0.0, 1.0)) as u8;
    }
    out
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba<u8> {
    Rgba([r, g, b, a])
}

fn image_bitmap_from(image: &RgbaImage) -> Result<ImageBitmap> {
    ImageBitmap::from_rgba8(image.width(), image.height(), image.clone().into_raw())
        .map_err(|error| anyhow!("converting preview into ImageBitmap failed: {error}"))
}

fn runtime_label(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "ms".to_string()
    } else if trimmed.ends_with("ms") {
        trimmed.to_string()
    } else {
        format!("{trimmed}ms")
    }
}

fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("output")
}

#[derive(Clone, Copy)]
struct BoxArea {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl BoxArea {
    const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

struct AssetPack {
    background: DynamicImage,
    qr: DynamicImage,
    mono_font: FontArc,
    sans_font: FontArc,
}

impl AssetPack {
    fn load() -> Result<Self> {
        Ok(Self {
            background: image::open(asset_path("assets/background.jpg"))
                .context("loading background image")?,
            qr: image::open(asset_path("assets/qr-overlay.png")).context("loading QR image")?,
            mono_font: load_system_font(
                &[
                    Family::Name("DejaVu Sans Mono"),
                    Family::Name("JetBrains Mono"),
                    Family::Monospace,
                ],
                Weight::NORMAL,
            )
            .context("loading monospace font")?,
            sans_font: load_system_font(
                &[
                    Family::Name("DejaVu Sans"),
                    Family::Name("Arial"),
                    Family::SansSerif,
                ],
                Weight::NORMAL,
            )
            .context("loading sans font")?,
        })
    }
}

fn asset_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn load_system_font(families: &[Family<'_>], weight: Weight) -> Result<FontArc> {
    let mut db = Database::new();
    db.load_system_fonts();
    let query = Query {
        families,
        weight,
        style: Style::Normal,
        ..Query::default()
    };

    let font_id = db
        .query(&query)
        .ok_or_else(|| anyhow!("no matching system font found"))?;

    db.with_face_data(font_id, |data, _index| {
        FontArc::try_from_vec(data.to_vec()).map_err(|_| anyhow!("invalid font data"))
    })
    .ok_or_else(|| anyhow!("font face data unavailable"))?
}

#[cfg(test)]
mod tests {
    use super::{generate_preview, save_webp};
    use crate::draft::PostDraft;
    use std::fs;
    use std::path::Path;

    #[test]
    fn preview_and_webp_export_succeed() {
        let draft = PostDraft {
            date: "22.04.2026".to_string(),
            problem_title: "Words Within Two Edits of Dictionary".to_string(),
            problem_url: "https://leetcode.com/problems/words-within-two-edits-of-dictionary/"
                .to_string(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: String::new(),
            reference_url: "https://dmitrysamoylenko.com/2023/07/14/leetcode_daily.html"
                .to_string(),
            telegram_text: String::new(),
            problem_tldr: "Words in dictionary with 2 edits".to_string(),
            intuition: "Scan and count mismatches".to_string(),
            approach: "Brute force is enough".to_string(),
            time_complexity: "n * m * k".to_string(),
            space_complexity: "1".to_string(),
            kotlin_runtime_ms: "28".to_string(),
            kotlin_code: "fun demo() {}".to_string(),
            rust_runtime_ms: "1".to_string(),
            rust_code: "fn demo() {}".to_string(),
        };

        let preview = generate_preview(&draft).expect("preview generation");
        assert!(!preview.preview_png_path.is_empty());
        assert!(preview.bitmap.width() > 0);
        assert!(preview.bitmap.height() > 0);
        assert!(Path::new(&preview.preview_png_path).exists());

        let saved = save_webp(&draft).expect("webp save");
        let webp_path = saved.last_saved_webp_path.clone().expect("saved webp path");
        assert!(Path::new(&webp_path).exists());

        let _ = fs::remove_file(webp_path);
    }
}
