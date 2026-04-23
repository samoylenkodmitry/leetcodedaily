use crate::{assets, draft::PostDraft};
use ab_glyph::{FontArc, PxScale};
use anyhow::{Context, Result, anyhow};
use cranpose::ImageBitmap;
use image::ImageFormat;
use image::imageops::{FilterType, overlay};
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_circle_mut, draw_filled_rect_mut, draw_text_mut, text_size};
use imageproc::rect::Rect;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(target_arch = "wasm32")]
use std::io::Cursor;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

const CANVAS_WIDTH: u32 = 1600;
const CANVAS_HEIGHT: u32 = 900;
const CODE_FONT_SIZES: [f32; 7] = [24.0, 22.0, 20.0, 18.0, 16.0, 14.0, 12.0];
const TLDR_FONT_SIZES: [f32; 7] = [30.0, 28.0, 26.0, 24.0, 22.0, 20.0, 18.0];

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
    let rendered = compose_card(draft)?;
    let bitmap = image_bitmap_from(&rendered)?;
    Ok(PreviewState {
        bitmap,
        preview_png_path: String::new(),
        last_saved_webp_path: None,
    })
}

pub fn save_webp(draft: &PostDraft) -> Result<PreviewState> {
    let rendered = compose_card(draft)?;
    let bitmap = image_bitmap_from(&rendered)?;

    #[cfg(not(target_arch = "wasm32"))]
    {
        let output_dir = output_dir();
        fs::create_dir_all(&output_dir).context("creating output directory")?;

        let preview_png_path = save_preview_png(&rendered)?;
        let export_path = draft.suggested_export_path();
        DynamicImage::ImageRgba8(rendered)
            .save_with_format(&export_path, ImageFormat::WebP)
            .with_context(|| format!("saving WebP to {}", export_path.display()))?;

        return Ok(PreviewState {
            bitmap,
            preview_png_path: preview_png_path.display().to_string(),
            last_saved_webp_path: Some(export_path.display().to_string()),
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        let bytes = encode_webp_bytes(&rendered)?;
        let filename = draft.suggested_export_filename();
        download_webp_bytes(&filename, &bytes)?;
        Ok(PreviewState {
            bitmap,
            preview_png_path: String::new(),
            last_saved_webp_path: Some(filename),
        })
    }
}

fn compose_card(draft: &PostDraft) -> Result<RgbaImage> {
    let assets = AssetPack::load()?;
    let canvas =
        assets
            .background
            .resize_to_fill(CANVAS_WIDTH, CANVAS_HEIGHT, FilterType::Lanczos3);
    let mut canvas = canvas.to_rgba8();

    let panel = BoxArea::new(104, 112, 1392, 660);
    let panel_padding = 56;
    let code_gap = 24u32;
    let tldr_height = 108u32;
    let tldr_gap = 26u32;

    let mut overlay_layer = RgbaImage::new(CANVAS_WIDTH, CANVAS_HEIGHT);
    fill_rounded_box(&mut overlay_layer, panel, 46, rgba(5, 8, 14, 210));
    overlay(&mut canvas, &overlay_layer, 0, 0);

    let qr = tint_alpha(
        &assets
            .qr
            .resize_exact(170, 170, FilterType::Lanczos3)
            .to_rgba8(),
        0.72,
    );
    overlay(&mut canvas, &qr, 26, 26);

    let inner_x = panel.x + panel_padding;
    let inner_width = panel.width.saturating_sub((panel_padding * 2) as u32);
    let code_top = panel.y + panel_padding;
    let code_region_height = panel
        .height
        .saturating_sub((panel_padding * 2) as u32 + tldr_height + tldr_gap);
    let code_blocks = visible_code_blocks(draft);

    if code_blocks.len() == 1 {
        let block_height = code_region_height.min(320).max(220);
        let offset = code_region_height.saturating_sub(block_height) / 2;
        let block = &code_blocks[0];
        draw_code_panel(
            &mut canvas,
            &assets,
            BoxArea::new(inner_x, code_top + offset as i32, inner_width, block_height),
            block.language,
            block.runtime_ms,
            block.code,
        );
    } else {
        let block_height = code_region_height.saturating_sub(code_gap) / 2;
        for (index, block) in code_blocks.iter().enumerate() {
            let y = code_top + index as i32 * (block_height as i32 + code_gap as i32);
            draw_code_panel(
                &mut canvas,
                &assets,
                BoxArea::new(inner_x, y, inner_width, block_height),
                block.language,
                block.runtime_ms,
                block.code,
            );
        }
    }

    let tldr_y = panel.y + panel.height as i32 - panel_padding - tldr_height as i32;
    let body_color = rgba(170, 176, 187, 255);
    let tldr_layout = fit_paragraph_layout(
        &assets.sans_font,
        &draft.preview_tldr(),
        inner_width,
        tldr_height,
        &TLDR_FONT_SIZES,
        1.28,
    );
    draw_wrapped_lines(
        &mut canvas,
        body_color,
        inner_x,
        tldr_y,
        inner_width,
        tldr_layout.scale,
        &assets.sans_font,
        &tldr_layout.lines,
        tldr_layout.line_height,
    );

    Ok(canvas)
}

fn visible_code_blocks<'a>(draft: &'a PostDraft) -> Vec<CodeBlock<'a>> {
    let mut blocks = Vec::new();
    if !draft.kotlin_code.trim().is_empty() {
        blocks.push(CodeBlock {
            language: "kotlin",
            runtime_ms: &draft.kotlin_runtime_ms,
            code: &draft.kotlin_code,
        });
    }
    if !draft.rust_code.trim().is_empty() {
        blocks.push(CodeBlock {
            language: "rust",
            runtime_ms: &draft.rust_runtime_ms,
            code: &draft.rust_code,
        });
    }
    if blocks.is_empty() {
        blocks.push(CodeBlock {
            language: "kotlin",
            runtime_ms: &draft.kotlin_runtime_ms,
            code: &draft.kotlin_code,
        });
    }
    blocks
}

fn draw_code_panel(
    canvas: &mut RgbaImage,
    assets: &AssetPack,
    area: BoxArea,
    language: &str,
    runtime_ms: &str,
    code: &str,
) {
    let label_color = rgba(148, 229, 255, 255);
    let runtime_color = rgba(255, 180, 78, 255);
    let code_color = rgba(242, 246, 250, 255);
    let content_width = area.width.saturating_sub(56);
    let layout = fit_code_layout(code, content_width, area.height);

    let title = format!("// {language}");
    let runtime = format!("// {}", runtime_label(runtime_ms));
    let title_y = area.y + 22;
    let runtime_y = title_y + layout.label_line_height + 6;

    draw_text_mut(
        canvas,
        label_color,
        area.x + 28,
        title_y,
        layout.label_scale,
        &assets.sans_font,
        &title,
    );
    draw_text_mut(
        canvas,
        runtime_color,
        area.x + 28,
        runtime_y,
        layout.label_scale,
        &assets.sans_font,
        &runtime,
    );

    let code_y = runtime_y + layout.label_line_height + 22;
    draw_wrapped_lines(
        canvas,
        code_color,
        area.x + 28,
        code_y,
        content_width,
        layout.code_scale,
        &assets.mono_font,
        &layout.lines,
        layout.code_line_height,
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

fn fit_code_layout(code: &str, width: u32, height: u32) -> FittedCodeLayout {
    let mut last = None;

    for &font_size in &CODE_FONT_SIZES {
        let code_scale = PxScale::from(font_size);
        let label_size = (font_size + 4.0).max(16.0);
        let label_scale = PxScale::from(label_size);
        let label_line_height = (label_size * 1.08).ceil() as i32;
        let code_line_height = (font_size * 1.36).ceil() as i32;
        let max_chars = estimate_monospace_chars(width, font_size);
        let lines = wrap_code(code, max_chars);
        let header_height = 50 + label_line_height * 2;
        let available_height = (height as i32 - header_height).max(code_line_height);
        let total_height = lines.len() as i32 * code_line_height;
        let layout = FittedCodeLayout {
            code_scale,
            label_scale,
            label_line_height,
            code_line_height,
            lines,
        };

        if total_height <= available_height {
            return layout;
        }
        last = Some((layout, available_height));
    }

    let (mut layout, available_height) = last.expect("code sizes must not be empty");
    let max_lines = (available_height / layout.code_line_height).max(1) as usize;
    layout.lines = truncate_lines(layout.lines, max_lines);
    layout
}

fn fit_paragraph_layout(
    font: &FontArc,
    text: &str,
    width: u32,
    height: u32,
    sizes: &[f32],
    line_height_factor: f32,
) -> FittedTextLayout {
    let mut last = None;

    for &font_size in sizes {
        let scale = PxScale::from(font_size);
        let line_height = (font_size * line_height_factor).ceil() as i32;
        let lines = wrap_paragraph(text, font, scale, width);
        let available_height = (height as i32).max(line_height);
        let total_height = lines.len() as i32 * line_height;
        let layout = FittedTextLayout {
            scale,
            line_height,
            lines,
        };

        if total_height <= available_height {
            return layout;
        }
        last = Some((layout, available_height));
    }

    let (mut layout, available_height) = last.expect("paragraph sizes must not be empty");
    let max_lines = (available_height / layout.line_height).max(1) as usize;
    layout.lines = truncate_lines(layout.lines, max_lines);
    layout
}

fn truncate_lines(mut lines: Vec<String>, max_lines: usize) -> Vec<String> {
    if lines.len() <= max_lines {
        return lines;
    }

    lines.truncate(max_lines.max(1));
    if let Some(last) = lines.last_mut() {
        if last.trim().is_empty() {
            *last = "...".to_string();
        } else {
            last.push_str(" ...");
        }
    }
    lines
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

#[cfg(target_arch = "wasm32")]
fn encode_webp_bytes(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut cursor, ImageFormat::WebP)
        .context("encoding preview as WebP")?;
    Ok(cursor.into_inner())
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

struct CodeBlock<'a> {
    language: &'static str,
    runtime_ms: &'a str,
    code: &'a str,
}

struct FittedCodeLayout {
    code_scale: PxScale,
    label_scale: PxScale,
    label_line_height: i32,
    code_line_height: i32,
    lines: Vec<String>,
}

struct FittedTextLayout {
    scale: PxScale,
    line_height: i32,
    lines: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("output")
}

#[cfg(not(target_arch = "wasm32"))]
fn save_preview_png(rendered: &RgbaImage) -> Result<PathBuf> {
    let preview_png_path = output_dir().join("preview-latest.png");
    rendered
        .save(&preview_png_path)
        .with_context(|| format!("saving preview to {}", preview_png_path.display()))?;
    Ok(preview_png_path)
}

#[cfg(target_arch = "wasm32")]
fn download_webp_bytes(filename: &str, bytes: &[u8]) -> Result<()> {
    let window = web_sys::window().ok_or_else(|| anyhow!("missing window"))?;
    let document = window
        .document()
        .ok_or_else(|| anyhow!("missing document"))?;
    let body = document
        .body()
        .ok_or_else(|| anyhow!("missing document body"))?;

    let options = BlobPropertyBag::new();
    options.set_type("image/webp");
    let parts = js_sys::Array::new();
    let byte_array = js_sys::Uint8Array::from(bytes);
    parts.push(byte_array.as_ref());

    let blob = Blob::new_with_u8_array_sequence_and_options(&parts.into(), &options)
        .map_err(|error| anyhow!("creating WebP blob failed: {error:?}"))?;
    let object_url = Url::create_object_url_with_blob(&blob)
        .map_err(|error| anyhow!("creating object URL failed: {error:?}"))?;

    let anchor = document
        .create_element("a")
        .map_err(|error| anyhow!("creating download link failed: {error:?}"))?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|_| anyhow!("casting download link failed"))?;
    anchor.set_href(&object_url);
    anchor.set_download(filename);

    let anchor_html = anchor
        .clone()
        .dyn_into::<web_sys::HtmlElement>()
        .map_err(|_| anyhow!("casting anchor element failed"))?;
    body.append_child(&anchor_html)
        .map_err(|error| anyhow!("adding download link failed: {error:?}"))?;
    anchor_html.click();
    body.remove_child(&anchor_html)
        .map_err(|error| anyhow!("removing download link failed: {error:?}"))?;
    Url::revoke_object_url(&object_url)
        .map_err(|error| anyhow!("releasing object URL failed: {error:?}"))?;
    Ok(())
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

#[derive(Clone)]
struct AssetPack {
    background: Arc<DynamicImage>,
    qr: Arc<DynamicImage>,
    mono_font: FontArc,
    sans_font: FontArc,
}

impl AssetPack {
    fn load() -> Result<Self> {
        static CACHE: OnceLock<Result<AssetPack, String>> = OnceLock::new();

        match CACHE.get_or_init(|| {
            Ok(AssetPack {
                background: Arc::new(
                    image::load_from_memory(assets::BACKGROUND_JPG)
                        .context("loading background image from embedded bytes")
                        .map_err(|error| error.to_string())?,
                ),
                qr: Arc::new(
                    image::load_from_memory(assets::QR_OVERLAY_PNG)
                        .context("loading QR image from embedded bytes")
                        .map_err(|error| error.to_string())?,
                ),
                mono_font: load_font(assets::DEJAVU_SANS_MONO_TTF)
                    .context("loading embedded monospace font")
                    .map_err(|error| error.to_string())?,
                sans_font: load_font(assets::DEJAVU_SANS_TTF)
                    .context("loading embedded sans font")
                    .map_err(|error| error.to_string())?,
            })
        }) {
            Ok(assets) => Ok(assets.clone()),
            Err(message) => Err(anyhow!(message.clone())),
        }
    }
}

fn load_font(bytes: &[u8]) -> Result<FontArc> {
    FontArc::try_from_vec(bytes.to_vec()).map_err(|_| anyhow!("invalid font data"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{compose_card, fit_code_layout, generate_preview, image_bitmap_from, save_webp};
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

        let rendered = compose_card(&draft).expect("compose card");
        let bitmap = image_bitmap_from(&rendered).expect("bitmap");
        assert!(bitmap.width() > 0);
        assert!(bitmap.height() > 0);

        let preview = generate_preview(&draft).expect("preview generation");
        assert!(preview.preview_png_path.is_empty());
        assert!(preview.bitmap.width() > 0);
        assert!(preview.bitmap.height() > 0);

        let saved = save_webp(&draft).expect("webp save");
        assert!(Path::new(&saved.preview_png_path).exists());
        let webp_path = saved.last_saved_webp_path.clone().expect("saved webp path");
        assert!(Path::new(&webp_path).exists());

        let _ = fs::remove_file(saved.preview_png_path);
        let _ = fs::remove_file(webp_path);
    }

    #[test]
    fn fit_code_layout_shrinks_when_code_is_large() {
        let code = (0..40)
            .map(|index| format!("let value_{index} = {index};"))
            .collect::<Vec<_>>()
            .join("\n");

        let layout = fit_code_layout(&code, 340, 180);

        assert!(layout.code_scale.x <= 18.0);
        assert!(!layout.lines.is_empty());
    }
}
