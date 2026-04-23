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
const TEXT_SUPERSAMPLE: u32 = 4;
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
const TEXT_SHADOW_OFFSET: (i32, i32) = (0, 2);
const TEXT_SHADOW_ALPHA: u8 = 112;
const TEXT_EMBOLDEN_OFFSETS: [(i32, i32); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];
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

#[derive(Clone, PartialEq)]
pub(crate) struct ComposePreviewAssets {
    pub background: ImageBitmap,
    pub qr: ImageBitmap,
}

#[derive(Clone, PartialEq)]
pub(crate) struct CardRenderPlan {
    pub panel: BoxArea,
    pub qr: BoxArea,
    pub code_blocks: Vec<CodeRenderPlan>,
    pub tldr: TextRenderPlan,
}

#[derive(Clone, PartialEq)]
pub(crate) struct CodeRenderPlan {
    pub text_x: i32,
    pub title_y: i32,
    pub runtime_y: i32,
    pub code_y: i32,
    pub label_font_size: f32,
    pub code_font_size: f32,
    pub code_line_height: i32,
    pub language: String,
    pub runtime: String,
    pub lines: Vec<String>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct TextRenderPlan {
    pub x: i32,
    pub y: i32,
    pub font_size: f32,
    pub line_height: i32,
    pub lines: Vec<String>,
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

pub(crate) fn compose_preview_assets() -> Result<ComposePreviewAssets> {
    static CACHE: OnceLock<std::result::Result<ComposePreviewAssets, String>> = OnceLock::new();

    match CACHE.get_or_init(|| {
        let assets = AssetPack::load().map_err(|error| error.to_string())?;
        Ok(ComposePreviewAssets {
            background: image_bitmap_from_dynamic(&assets.background)
                .map_err(|error| error.to_string())?,
            qr: image_bitmap_from_dynamic(&assets.qr).map_err(|error| error.to_string())?,
        })
    }) {
        Ok(assets) => Ok(assets.clone()),
        Err(message) => Err(anyhow!(message.clone())),
    }
}

pub(crate) fn compose_preview_plan(draft: &PostDraft) -> Result<CardRenderPlan> {
    let assets = AssetPack::load()?;
    Ok(build_card_render_plan_with_assets(draft, &assets))
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
    let plan = build_card_render_plan_with_assets(draft, &assets);
    let canvas =
        assets
            .background
            .resize_to_fill(CANVAS_WIDTH, CANVAS_HEIGHT, FilterType::Lanczos3);
    let mut canvas = canvas.to_rgba8();
    let mut text_layer = RgbaImage::new(
        CANVAS_WIDTH * TEXT_SUPERSAMPLE,
        CANVAS_HEIGHT * TEXT_SUPERSAMPLE,
    );

    let mut overlay_layer = RgbaImage::new(CANVAS_WIDTH, CANVAS_HEIGHT);
    fill_rounded_box(&mut overlay_layer, plan.panel, 46, rgba(5, 8, 14, 210));
    overlay(&mut canvas, &overlay_layer, 0, 0);

    let qr = tint_alpha(
        &assets
            .qr
            .resize_exact(plan.qr.width, plan.qr.height, FilterType::Lanczos3)
            .to_rgba8(),
        0.72,
    );
    overlay(&mut canvas, &qr, plan.qr.x as i64, plan.qr.y as i64);

    for code_plan in &plan.code_blocks {
        draw_code_panel(&mut text_layer, &assets, code_plan);
    }

    let body_color = rgba(170, 176, 187, 255);
    draw_wrapped_lines(
        &mut text_layer,
        body_color,
        plan.tldr.x,
        plan.tldr.y,
        PxScale::from(plan.tldr.font_size),
        &assets.sans_font,
        &plan.tldr.lines,
        plan.tldr.line_height,
    );

    let text_overlay = DynamicImage::ImageRgba8(text_layer)
        .resize_exact(CANVAS_WIDTH, CANVAS_HEIGHT, FilterType::Lanczos3)
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

fn build_card_render_plan_with_assets(draft: &PostDraft, assets: &AssetPack) -> CardRenderPlan {
    let panel = BoxArea::new(86, 94, 1428, 710);
    let panel_padding = 38;
    let tldr_height = 84u32;
    let tldr_gap = 18u32;
    let qr = BoxArea::new(26, 26, 170, 170);

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

    let mut current_y = code_top + code_group_layout.top_offset as i32;
    let mut code_plans = Vec::with_capacity(code_blocks.len());
    for (index, (block, layout)) in code_blocks
        .iter()
        .zip(code_group_layout.blocks.iter())
        .enumerate()
    {
        let block_height = code_block_height(layout);
        let area = BoxArea::new(inner_x, current_y, inner_width, block_height);
        let centered_offset = ((area
            .width
            .saturating_sub(code_group_layout.shared_text_width))
            / 2) as i32;
        let text_x = area.x + centered_offset.max(CODE_SIDE_PADDING as i32);
        let title_y = area.y + CODE_LABEL_TOP_PADDING;
        let runtime_y = title_y + layout.label_line_height + CODE_LABEL_GAP;
        let code_y = runtime_y + layout.label_line_height + CODE_HEADER_TO_BODY_GAP;

        code_plans.push(CodeRenderPlan {
            text_x,
            title_y,
            runtime_y,
            code_y,
            label_font_size: layout.label_scale.y,
            code_font_size: layout.code_scale.y,
            code_line_height: layout.code_line_height,
            language: block.language.to_string(),
            runtime: runtime_label(block.runtime_ms),
            lines: layout.lines.clone(),
        });

        current_y += block_height as i32;
        if index + 1 < code_group_layout.blocks.len() {
            current_y += code_group_layout.gap as i32;
        }
    }

    let tldr_layout = fit_paragraph_layout(
        &assets.sans_font,
        &draft.preview_tldr(),
        inner_width,
        tldr_height,
        &TLDR_FONT_SIZES,
        1.16,
    );
    let tldr_area_top = panel.y + panel.height as i32 - panel_padding - tldr_height as i32;
    let tldr_area_bottom = panel.y + panel.height as i32 - panel_padding;
    let tldr_text_height = tldr_layout.line_height * tldr_layout.lines.len() as i32;
    let tldr_y = (tldr_area_bottom - tldr_text_height).max(tldr_area_top);

    CardRenderPlan {
        panel,
        qr,
        code_blocks: code_plans,
        tldr: TextRenderPlan {
            x: inner_x,
            y: tldr_y,
            font_size: tldr_layout.scale.y,
            line_height: tldr_layout.line_height,
            lines: tldr_layout.lines,
        },
    }
}

fn draw_code_panel(canvas: &mut RgbaImage, assets: &AssetPack, plan: &CodeRenderPlan) {
    let label_color = rgba(148, 229, 255, 255);
    let runtime_color = rgba(255, 180, 78, 255);
    let code_color = rgba(242, 246, 250, 255);

    draw_text_supersampled(
        canvas,
        label_color,
        plan.text_x,
        plan.title_y,
        PxScale::from(plan.label_font_size),
        &assets.mono_font,
        &format!("// {}", plan.language),
    );
    draw_text_supersampled(
        canvas,
        runtime_color,
        plan.text_x,
        plan.runtime_y,
        PxScale::from(plan.label_font_size),
        &assets.mono_font,
        &format!("// {}", plan.runtime),
    );
    draw_wrapped_lines(
        canvas,
        code_color,
        plan.text_x,
        plan.code_y,
        PxScale::from(plan.code_font_size),
        &assets.mono_font,
        &plan.lines,
        plan.code_line_height,
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
            let plan = plan_code_group_layout(&layouts, height);
            let stack_height = plan_stack_height(&layouts, plan.gap);
            if stack_height <= height {
                return FittedCodeGroupLayout {
                    gap: plan.gap,
                    top_offset: plan.top_offset,
                    shared_text_width: code_group_shared_width(font, blocks, &layouts),
                    blocks: layouts,
                };
            }
        }

        last = Some(layouts);
    }

    let layouts = last.expect("code sizes must not be empty");
    let plan = fallback_code_group_layout(&layouts, height);
    FittedCodeGroupLayout {
        gap: plan.gap,
        top_offset: plan.top_offset,
        shared_text_width: code_group_shared_width(font, blocks, &layouts),
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
    let content_width = code_block_content_width(font, block, layout);
    content_width <= available_width.saturating_sub(TEXT_WIDTH_SAFETY)
}

fn code_block_content_width(
    font: &FontArc,
    block: &CodeBlock<'_>,
    layout: &FittedCodeLayout,
) -> u32 {
    let title_width =
        measured_code_text_width(font, layout.label_scale, &format!("// {}", block.language));
    let runtime_width = measured_code_text_width(
        font,
        layout.label_scale,
        &format!("// {}", runtime_label(block.runtime_ms)),
    );
    max_line_width(font, layout.code_scale, &layout.lines)
        .max(title_width)
        .max(runtime_width)
}

fn code_group_shared_width(
    font: &FontArc,
    blocks: &[CodeBlock<'_>],
    layouts: &[FittedCodeLayout],
) -> u32 {
    blocks
        .iter()
        .zip(layouts.iter())
        .map(|(block, layout)| code_block_content_width(font, block, layout))
        .max()
        .unwrap_or(0)
}

fn max_line_width(font: &FontArc, scale: PxScale, lines: &[String]) -> u32 {
    lines
        .iter()
        .map(|line| measured_code_text_width(font, scale, line))
        .max()
        .unwrap_or(0)
}

fn measured_code_text_width(font: &FontArc, scale: PxScale, text: &str) -> u32 {
    let embolden_extra = TEXT_EMBOLDEN_OFFSETS
        .iter()
        .map(|(x, _)| *x)
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

fn image_bitmap_from_dynamic(image: &DynamicImage) -> Result<ImageBitmap> {
    let rgba = image.to_rgba8();
    ImageBitmap::from_rgba8(rgba.width(), rgba.height(), rgba.into_raw())
        .map_err(|error| anyhow!("converting asset into ImageBitmap failed: {error}"))
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
    let shadow = rgba(0, 0, 0, TEXT_SHADOW_ALPHA);
    draw_text_mut(
        canvas,
        shadow,
        base_x + superscaled_i32(TEXT_SHADOW_OFFSET.0),
        base_y + superscaled_i32(TEXT_SHADOW_OFFSET.1),
        scale,
        font,
        text,
    );
    for (x_offset, y_offset) in TEXT_EMBOLDEN_OFFSETS {
        draw_text_mut(
            canvas,
            color,
            base_x + x_offset,
            base_y + y_offset,
            scale,
            font,
            text,
        );
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
    top_offset: u32,
    shared_text_width: u32,
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

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct BoxArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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

fn block_height_sum(layouts: &[FittedCodeLayout]) -> u32 {
    layouts.iter().map(code_block_height).sum::<u32>()
}

fn plan_stack_height(layouts: &[FittedCodeLayout], gap: u32) -> u32 {
    block_height_sum(layouts) + gap.saturating_mul(layouts.len().saturating_sub(1) as u32)
}

fn plan_code_group_layout(layouts: &[FittedCodeLayout], available_height: u32) -> CodeGroupPlan {
    let block_sum = block_height_sum(layouts);
    if layouts.len() <= 1 {
        let free = available_height.saturating_sub(block_sum);
        return CodeGroupPlan {
            gap: 0,
            top_offset: free / 2,
        };
    }

    let baseline_gap = MIN_CODE_GAP;
    let baseline_stack = block_sum + baseline_gap;
    let extra = available_height.saturating_sub(baseline_stack);
    let gap = baseline_gap + ((extra as u64 * 55) / 100) as u32;
    let remaining = available_height.saturating_sub(block_sum + gap);
    let top_offset = ((remaining as u64 * 60) / 100) as u32;
    CodeGroupPlan { gap, top_offset }
}

fn fallback_code_group_layout(
    layouts: &[FittedCodeLayout],
    available_height: u32,
) -> CodeGroupPlan {
    if layouts.len() <= 1 {
        return CodeGroupPlan {
            gap: 0,
            top_offset: 0,
        };
    }

    let min_gap = min_code_gap(layouts.len());
    let block_sum = block_height_sum(layouts);
    let required_height = block_sum + min_gap;
    if required_height <= available_height {
        plan_code_group_layout(layouts, available_height)
    } else {
        CodeGroupPlan {
            gap: min_gap,
            top_offset: 0,
        }
    }
}

struct CodeGroupPlan {
    gap: u32,
    top_offset: u32,
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
