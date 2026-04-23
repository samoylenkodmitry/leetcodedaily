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
const TEXT_SUPERSAMPLE: u32 = 3;
const CODE_FONT_SIZES: [f32; 25] = [
    84.0, 80.0, 76.0, 72.0, 68.0, 64.0, 60.0, 56.0, 52.0, 48.0, 44.0, 40.0, 36.0, 32.0, 28.0, 24.0,
    22.0, 20.0, 18.0, 16.0, 14.0, 12.0, 10.0, 8.0, 6.0,
];
const TLDR_FONT_SIZES: [f32; 6] = [22.0, 20.0, 18.0, 16.0, 14.0, 12.0];
const CODE_SIDE_PADDING: u32 = 16;
const CODE_LABEL_TOP_PADDING: i32 = 14;
const CODE_LABEL_GAP: i32 = 4;
const CODE_HEADER_TO_BODY_GAP: i32 = 14;
const CODE_BOTTOM_PADDING: i32 = 10;
const MIN_CODE_GAP: u32 = 20;
const MAX_EXTRA_CODE_GAP: u32 = 68;
const TEXT_EMBOLDEN_X_OFFSETS: [i32; 2] = [0, 1];
const TEXT_WIDTH_SAFETY: u32 = 10;

#[derive(Clone)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone)]
pub struct PreviewState {
    pub bitmap: ImageBitmap,
    pub last_saved_webp_path: Option<String>,
}

impl PreviewState {
    pub fn placeholder() -> Self {
        placeholder_frame()
            .and_then(Self::from_frame)
            .unwrap_or_else(|_| {
                let bitmap =
                    ImageBitmap::from_rgba8(1, 1, vec![8, 12, 18, 255]).expect("placeholder");
                Self {
                    bitmap,
                    last_saved_webp_path: None,
                }
            })
    }

    pub fn from_frame(frame: PreviewFrame) -> Result<Self> {
        let bitmap = ImageBitmap::from_rgba8(frame.width, frame.height, frame.pixels)
            .map_err(|error| anyhow!("converting preview into ImageBitmap failed: {error}"))?;
        Ok(Self {
            bitmap,
            last_saved_webp_path: None,
        })
    }
}

pub fn render_preview_frame(draft: &PostDraft) -> std::result::Result<PreviewFrame, String> {
    compose_card(draft)
        .map(|rendered| PreviewFrame {
            width: rendered.width(),
            height: rendered.height(),
            pixels: rendered.into_raw(),
        })
        .map_err(|error| error.to_string())
}

pub fn save_webp(draft: &PostDraft) -> Result<PreviewState> {
    let rendered = compose_card(draft)?;
    let bitmap = image_bitmap_from(&rendered)?;

    #[cfg(not(target_arch = "wasm32"))]
    {
        let output_dir = output_dir();
        fs::create_dir_all(&output_dir).context("creating output directory")?;

        let export_path = draft.suggested_export_path();
        DynamicImage::ImageRgba8(rendered)
            .save_with_format(&export_path, ImageFormat::WebP)
            .with_context(|| format!("saving WebP to {}", export_path.display()))?;

        return Ok(PreviewState {
            bitmap,
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
    let mut text_layer = RgbaImage::new(
        CANVAS_WIDTH * TEXT_SUPERSAMPLE,
        CANVAS_HEIGHT * TEXT_SUPERSAMPLE,
    );

    let panel = BoxArea::new(86, 94, 1428, 710);
    let panel_padding = 38;
    let tldr_height = 84u32;
    let tldr_gap = 18u32;

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
    let code_group_layout = fit_code_group_layout(
        &assets.mono_font,
        &code_blocks,
        inner_width,
        code_region_height,
    );
    let stack_offset = code_region_height.saturating_sub(code_group_layout.stack_height) / 2;
    let mut current_y = code_top + stack_offset as i32;
    for (index, (block, layout)) in code_blocks
        .iter()
        .zip(code_group_layout.blocks.iter())
        .enumerate()
    {
        let block_height = code_block_height(layout);
        draw_code_panel(
            &mut text_layer,
            &assets,
            BoxArea::new(inner_x, current_y, inner_width, block_height),
            block.language,
            block.runtime_ms,
            layout,
        );
        current_y += block_height as i32;
        if index + 1 < code_group_layout.blocks.len() {
            current_y += code_group_layout.gap as i32;
        }
    }

    let body_color = rgba(170, 176, 187, 255);
    let tldr_area_top = panel.y + panel.height as i32 - panel_padding - tldr_height as i32;
    let tldr_area_bottom = panel.y + panel.height as i32 - panel_padding;
    let tldr_layout = fit_paragraph_layout(
        &assets.sans_font,
        &draft.preview_tldr(),
        inner_width,
        tldr_height,
        &TLDR_FONT_SIZES,
        1.16,
    );
    let tldr_text_height = tldr_layout.line_height * tldr_layout.lines.len() as i32;
    let tldr_y = (tldr_area_bottom - tldr_text_height).max(tldr_area_top);
    draw_wrapped_lines(
        &mut text_layer,
        body_color,
        inner_x,
        tldr_y,
        tldr_layout.scale,
        &assets.sans_font,
        &tldr_layout.lines,
        tldr_layout.line_height,
    );

    let text_overlay = DynamicImage::ImageRgba8(text_layer)
        .resize_exact(CANVAS_WIDTH, CANVAS_HEIGHT, FilterType::CatmullRom)
        .to_rgba8();
    overlay(&mut canvas, &text_overlay, 0, 0);

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
    layout: &FittedCodeLayout,
) {
    let label_color = rgba(148, 229, 255, 255);
    let runtime_color = rgba(255, 180, 78, 255);
    let code_color = rgba(242, 246, 250, 255);

    let title = format!("// {language}");
    let runtime = format!("// {}", runtime_label(runtime_ms));
    let text_x = area.x + CODE_SIDE_PADDING as i32;
    let title_y = area.y + CODE_LABEL_TOP_PADDING;
    let runtime_y = title_y + layout.label_line_height + CODE_LABEL_GAP;

    draw_text_supersampled(
        canvas,
        label_color,
        text_x,
        title_y,
        layout.label_scale,
        &assets.mono_font,
        &title,
    );
    draw_text_supersampled(
        canvas,
        runtime_color,
        text_x,
        runtime_y,
        layout.label_scale,
        &assets.mono_font,
        &runtime,
    );

    let code_y = runtime_y + layout.label_line_height + CODE_HEADER_TO_BODY_GAP;
    draw_wrapped_lines(
        canvas,
        code_color,
        text_x,
        code_y,
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
    scale: PxScale,
    font: &FontArc,
    lines: &[String],
    line_height: i32,
) {
    for (index, line) in lines.iter().enumerate() {
        draw_text_supersampled(
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

fn code_lines(code: &str) -> Vec<String> {
    if code.trim().is_empty() {
        return vec!["// paste code here".to_string()];
    }
    code.lines().map(|line| line.to_string()).collect()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
fn fit_code_layout(code: &str, width: u32, height: u32) -> FittedCodeLayout {
    let block = [CodeBlock {
        language: "kotlin",
        runtime_ms: "",
        code,
    }];
    fit_code_group_layout(preview_mono_font(), &block, width, height)
        .blocks
        .into_iter()
        .next()
        .expect("single layout")
}

fn fit_code_group_layout(
    font: &FontArc,
    blocks: &[CodeBlock<'_>],
    width: u32,
    height: u32,
) -> FittedCodeGroupLayout {
    let mut last = None;
    let content_width = width.saturating_sub(CODE_SIDE_PADDING * 2);

    for &font_size in &CODE_FONT_SIZES {
        let code_scale = PxScale::from(font_size);
        let label_size = (font_size + 2.0).max(14.0);
        let label_scale = PxScale::from(label_size);
        let label_line_height = (label_size * 1.05).ceil() as i32;
        let code_line_height = (font_size * 1.24).ceil() as i32;

        let layouts = blocks
            .iter()
            .map(|block| FittedCodeLayout {
                code_scale,
                label_scale,
                label_line_height,
                code_line_height,
                lines: code_lines(block.code),
            })
            .collect::<Vec<_>>();

        if layouts
            .iter()
            .zip(blocks.iter())
            .all(|(layout, block)| code_layout_fits(font, block, layout, content_width))
        {
            let stack_height = code_stack_height(&layouts, height);
            if stack_height <= height {
                return FittedCodeGroupLayout {
                    gap: code_group_gap(&layouts, height),
                    stack_height,
                    blocks: layouts,
                };
            }
        }

        last = Some((layouts, content_width, height));
    }

    let (mut layouts, content_width, available_height) =
        last.expect("code sizes must not be empty");
    let gap = min_code_gap(layouts.len());
    let total_gap = gap.saturating_mul(layouts.len().saturating_sub(1) as u32) as i32;
    let shared_height =
        ((available_height as i32 - total_gap).max(1) / layouts.len() as i32).max(1);
    for layout in &mut layouts {
        let available_code_height =
            (shared_height - code_header_height(layout) as i32).max(layout.code_line_height);
        let max_lines = (available_code_height / layout.code_line_height).max(1) as usize;
        layout.lines = truncate_lines(layout.lines.clone(), max_lines);
        while max_line_width(font, layout.code_scale, &layout.lines) > content_width
            && layout.lines.len() > 1
        {
            let new_len = layout.lines.len().saturating_sub(1).max(1);
            layout.lines = truncate_lines(layout.lines.clone(), new_len);
        }
    }
    let stack_height = code_stack_height(&layouts, height);
    FittedCodeGroupLayout {
        gap,
        stack_height,
        blocks: layouts,
    }
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

fn code_layout_fits(
    font: &FontArc,
    block: &CodeBlock<'_>,
    layout: &FittedCodeLayout,
    available_width: u32,
) -> bool {
    let title_width =
        measured_code_text_width(font, layout.label_scale, &format!("// {}", block.language));
    let runtime_width = measured_code_text_width(
        font,
        layout.label_scale,
        &format!("// {}", runtime_label(block.runtime_ms)),
    );
    let content_width = max_line_width(font, layout.code_scale, &layout.lines)
        .max(title_width)
        .max(runtime_width);
    content_width <= available_width.saturating_sub(TEXT_WIDTH_SAFETY)
}

fn max_line_width(font: &FontArc, scale: PxScale, lines: &[String]) -> u32 {
    lines
        .iter()
        .map(|line| measured_code_text_width(font, scale, line))
        .max()
        .unwrap_or(0)
}

fn measured_code_text_width(font: &FontArc, scale: PxScale, text: &str) -> u32 {
    let embolden_extra = TEXT_EMBOLDEN_X_OFFSETS
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .max(0) as u32;
    text_size(scale, font, text)
        .0
        .saturating_add(embolden_extra)
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

fn placeholder_frame() -> Result<PreviewFrame> {
    static CACHE: OnceLock<std::result::Result<PreviewFrame, String>> = OnceLock::new();

    match CACHE.get_or_init(|| {
        let assets = AssetPack::load().map_err(|error| error.to_string())?;
        let image = assets
            .background
            .resize_to_fill(CANVAS_WIDTH, CANVAS_HEIGHT, FilterType::Lanczos3)
            .to_rgba8();
        Ok(PreviewFrame {
            width: image.width(),
            height: image.height(),
            pixels: image.into_raw(),
        })
    }) {
        Ok(frame) => Ok(frame.clone()),
        Err(message) => Err(anyhow!(message.clone())),
    }
}

fn draw_text_supersampled(
    canvas: &mut RgbaImage,
    color: Rgba<u8>,
    x: i32,
    y: i32,
    scale: PxScale,
    font: &FontArc,
    text: &str,
) {
    let base_x = superscaled_i32(x);
    let base_y = superscaled_i32(y);
    let scale = superscaled_scale(scale);
    for x_offset in TEXT_EMBOLDEN_X_OFFSETS {
        draw_text_mut(canvas, color, base_x + x_offset, base_y, scale, font, text);
    }
}

fn superscaled_i32(value: i32) -> i32 {
    value * TEXT_SUPERSAMPLE as i32
}

fn superscaled_scale(scale: PxScale) -> PxScale {
    PxScale {
        x: scale.x * TEXT_SUPERSAMPLE as f32,
        y: scale.y * TEXT_SUPERSAMPLE as f32,
    }
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

struct FittedCodeGroupLayout {
    gap: u32,
    stack_height: u32,
    blocks: Vec<FittedCodeLayout>,
}

struct FittedTextLayout {
    scale: PxScale,
    line_height: i32,
    lines: Vec<String>,
}

#[cfg(all(not(target_arch = "wasm32"), test))]
fn output_dir() -> PathBuf {
    std::env::temp_dir().join("leetcodedaily-tests")
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn output_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Downloads"))
        .unwrap_or_else(|| PathBuf::from("."))
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

fn code_header_height(layout: &FittedCodeLayout) -> u32 {
    (CODE_LABEL_TOP_PADDING
        + layout.label_line_height
        + CODE_LABEL_GAP
        + layout.label_line_height
        + CODE_HEADER_TO_BODY_GAP) as u32
}

fn code_block_height(layout: &FittedCodeLayout) -> u32 {
    code_header_height(layout)
        .saturating_add((layout.lines.len() as i32 * layout.code_line_height) as u32)
        .saturating_add(CODE_BOTTOM_PADDING as u32)
}

fn min_code_gap(block_count: usize) -> u32 {
    if block_count > 1 { MIN_CODE_GAP } else { 0 }
}

fn preferred_code_gap(
    available_height: u32,
    baseline_stack_height: u32,
    block_count: usize,
) -> u32 {
    if block_count <= 1 {
        return 0;
    }

    let extra_gap =
        (available_height.saturating_sub(baseline_stack_height) / 4).min(MAX_EXTRA_CODE_GAP);
    MIN_CODE_GAP + extra_gap
}

fn code_group_gap(layouts: &[FittedCodeLayout], available_height: u32) -> u32 {
    let baseline_gap = min_code_gap(layouts.len());
    if baseline_gap == 0 {
        return 0;
    }

    let block_height_sum = layouts.iter().map(code_block_height).sum::<u32>();
    let baseline_stack_height =
        block_height_sum + baseline_gap.saturating_mul(layouts.len().saturating_sub(1) as u32);
    if baseline_stack_height > available_height {
        baseline_gap
    } else {
        preferred_code_gap(available_height, baseline_stack_height, layouts.len())
    }
}

fn code_stack_height(layouts: &[FittedCodeLayout], available_height: u32) -> u32 {
    let block_height_sum = layouts.iter().map(code_block_height).sum::<u32>();
    let gap = code_group_gap(layouts, available_height);
    let baseline_stack_height =
        block_height_sum + gap.saturating_mul(layouts.len().saturating_sub(1) as u32);
    if baseline_stack_height > available_height {
        baseline_stack_height
    } else {
        baseline_stack_height
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
                mono_font: load_font(assets::MONASPACE_KRYPTON_TTF)
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
fn preview_mono_font() -> &'static FontArc {
    static FONT: OnceLock<FontArc> = OnceLock::new();
    FONT.get_or_init(|| load_font(assets::MONASPACE_KRYPTON_TTF).expect("preview mono font"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{
        PreviewState, compose_card, fit_code_layout, image_bitmap_from, render_preview_frame,
        save_webp,
    };
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

        let preview =
            PreviewState::from_frame(render_preview_frame(&draft).expect("preview frame"))
                .expect("preview generation");
        assert!(preview.bitmap.width() > 0);
        assert!(preview.bitmap.height() > 0);

        let saved = save_webp(&draft).expect("webp save");
        let webp_path = saved.last_saved_webp_path.clone().expect("saved webp path");
        assert!(Path::new(&webp_path).exists());

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

    #[test]
    fn fit_code_layout_keeps_long_line_unwrapped() {
        let code = "let hyper_verbose_variable_name = call_super_long_method_name(with_many_arguments, more_arguments, even_more_arguments);";

        let layout = fit_code_layout(code, 520, 180);

        assert_eq!(layout.lines, vec![code.to_string()]);
        assert!(layout.code_scale.x < 24.0);
    }
}
