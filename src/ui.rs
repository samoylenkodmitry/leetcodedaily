#![allow(non_snake_case)]

use crate::draft::{EditorFields, PostDraft};
use crate::export::{PreviewState, generate_preview, save_webp};
use anyhow::Result;
#[cfg(target_arch = "wasm32")]
use anyhow::anyhow;
#[cfg(not(target_arch = "wasm32"))]
use arboard::Clipboard;
use cranpose::Box as ComposeBox;
use cranpose::DEFAULT_ALPHA;
use cranpose::prelude::*;
use cranpose::widgets::BasicTextFieldWithOptions;
use cranpose_core::MutableState;
use cranpose_foundation::text::{TextFieldLineLimits, TextFieldState};

const APP_WIDTH: u32 = 1480;
const APP_HEIGHT: u32 = 1560;
#[cfg(any(test, target_arch = "wasm32"))]
const WEB_SURFACE_MAX_DIM: u32 = 1900;
#[cfg(target_arch = "wasm32")]
const WEB_CANVAS_MARGIN: f64 = 48.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorTab {
    Output,
    Meta,
    Writeup,
    Code,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run() {
    launcher_with_size(APP_WIDTH, APP_HEIGHT).run(App);
}

#[cfg(target_arch = "wasm32")]
pub async fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    let (width, height) = web_canvas_size()?;
    launcher_with_size(width, height)
        .run_web("app-canvas", App)
        .await
}

fn launcher_with_size(width: u32, height: u32) -> AppLauncher {
    AppLauncher::new()
        .with_title("LeetCode Daily Composer")
        .with_size(width, height)
        .with_fonts(crate::assets::APP_FONTS)
}

#[cfg(target_arch = "wasm32")]
fn web_canvas_size() -> Result<(u32, u32), wasm_bindgen::JsValue> {
    let window =
        web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("missing window"))?;
    let viewport_width = js_number(&window.inner_width()?)? - WEB_CANVAS_MARGIN;
    let viewport_height = js_number(&window.inner_height()?)? - WEB_CANVAS_MARGIN;
    let device_pixel_ratio = window.device_pixel_ratio().max(1.0);
    Ok(compute_web_canvas_size(
        viewport_width,
        viewport_height,
        device_pixel_ratio,
    ))
}

#[cfg(target_arch = "wasm32")]
fn js_number(value: &wasm_bindgen::JsValue) -> Result<f64, wasm_bindgen::JsValue> {
    value
        .as_f64()
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("expected numeric window dimension"))
}

#[cfg(any(test, target_arch = "wasm32"))]
fn compute_web_canvas_size(
    viewport_width: f64,
    viewport_height: f64,
    device_pixel_ratio: f64,
) -> (u32, u32) {
    let width = clamp_web_dimension(APP_WIDTH, viewport_width, device_pixel_ratio);
    let height = clamp_web_dimension(APP_HEIGHT, viewport_height, device_pixel_ratio);
    (width, height)
}

#[cfg(any(test, target_arch = "wasm32"))]
fn clamp_web_dimension(target: u32, viewport: f64, device_pixel_ratio: f64) -> u32 {
    let target = f64::from(target);
    let viewport = if viewport.is_finite() {
        viewport.max(1.0)
    } else {
        target
    };
    let dpr = if device_pixel_ratio.is_finite() {
        device_pixel_ratio.max(1.0)
    } else {
        1.0
    };
    let max_logical = (f64::from(WEB_SURFACE_MAX_DIM) / dpr).floor().max(1.0);
    target.min(viewport).min(max_logical).floor().max(1.0) as u32
}

#[composable]
fn App() {
    let scroll_state = remember(|| ScrollState::new(0.0)).with(|state| state.clone());
    let fields = remember(EditorFields::default).with(|fields| fields.clone());
    let active_tab = useState(|| EditorTab::Output);
    let boot = remember({
        let fields = fields.clone();
        move || match generate_preview(&PostDraft::from_fields(&fields)) {
            Ok(preview) => BootState {
                preview,
                status: "Preview ready. Edit the fields and regenerate when needed.".to_string(),
            },
            Err(error) => BootState {
                preview: PreviewState::placeholder(),
                status: format!("Preview generation failed on startup: {error}"),
            },
        }
    })
    .with(|state| state.clone());
    let preview_state = useState({
        let boot = boot.clone();
        move || boot.preview
    });
    let status = useState({
        let boot = boot.clone();
        move || boot.status
    });

    let markdown_preview = PostDraft::from_fields(&fields).markdown();
    let current_tab = active_tab.value();

    Column(
        Modifier::empty()
            .fill_max_size()
            .background(ui_surface())
            .vertical_scroll(scroll_state.clone(), false)
            .padding(28.0),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(22.0)),
        {
            let fields = fields.clone();
            let status = status.clone();
            let preview_state = preview_state.clone();
            let markdown_preview = markdown_preview.clone();
            let active_tab = active_tab.clone();
            let scroll_state = scroll_state.clone();
            move || {
                ActionsCard(fields.clone(), status.clone(), preview_state.clone());
                section_card({
                    let active_tab = active_tab.clone();
                    let scroll_state = scroll_state.clone();
                    move || {
                        Row(
                            Modifier::empty().fill_max_width(),
                            RowSpec::default()
                                .horizontal_arrangement(LinearArrangement::spaced_by(12.0)),
                            {
                                let active_tab = active_tab.clone();
                                let scroll_state = scroll_state.clone();
                                move || {
                                    tab_button(
                                        "Output",
                                        active_tab.value() == EditorTab::Output,
                                        {
                                            let active_tab = active_tab.clone();
                                            let scroll_state = scroll_state.clone();
                                            move || {
                                                active_tab.set(EditorTab::Output);
                                                scroll_state.scroll_to(0.0);
                                            }
                                        },
                                    );
                                    tab_button("Meta", active_tab.value() == EditorTab::Meta, {
                                        let active_tab = active_tab.clone();
                                        let scroll_state = scroll_state.clone();
                                        move || {
                                            active_tab.set(EditorTab::Meta);
                                            scroll_state.scroll_to(0.0);
                                        }
                                    });
                                    tab_button(
                                        "Writeup",
                                        active_tab.value() == EditorTab::Writeup,
                                        {
                                            let active_tab = active_tab.clone();
                                            let scroll_state = scroll_state.clone();
                                            move || {
                                                active_tab.set(EditorTab::Writeup);
                                                scroll_state.scroll_to(0.0);
                                            }
                                        },
                                    );
                                    tab_button("Code", active_tab.value() == EditorTab::Code, {
                                        let active_tab = active_tab.clone();
                                        let scroll_state = scroll_state.clone();
                                        move || {
                                            active_tab.set(EditorTab::Code);
                                            scroll_state.scroll_to(0.0);
                                        }
                                    });
                                }
                            },
                        );
                    }
                });
                ActiveTabContent(
                    current_tab,
                    fields.clone(),
                    preview_state.clone(),
                    markdown_preview.clone(),
                );
            }
        },
    );
}

#[composable]
fn ActiveTabContent(
    active_tab: EditorTab,
    fields: EditorFields,
    preview_state: MutableState<PreviewState>,
    markdown_preview: String,
) {
    match active_tab {
        EditorTab::Output => {
            PreviewCard(preview_state);
            MarkdownCard(markdown_preview);
        }
        EditorTab::Meta => ProblemMetaCard(fields),
        EditorTab::Writeup => WriteupCard(fields),
        EditorTab::Code => CodeCard(fields),
    }
}

#[composable]
fn ActionsCard(
    fields: EditorFields,
    status: MutableState<String>,
    preview_state: MutableState<PreviewState>,
) {
    section_card({
        let fields = fields.clone();
        let status = status.clone();
        let preview_state = preview_state.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let preview_state = preview_state.clone();
                    move || {
                        Text(
                            "LeetCode Daily Composer",
                            Modifier::empty(),
                            heading_style(34.0),
                        );
                        Text(
                            "Fill the template, copy the final markdown, regenerate the code card preview, and export a WebP from the same app.",
                            Modifier::empty(),
                            body_style(),
                        );

                        let row_fields = fields.clone();
                        let row_status = status.clone();
                        let row_preview = preview_state.clone();
                        Row(
                            Modifier::empty().fill_max_width(),
                            RowSpec::default()
                                .horizontal_arrangement(LinearArrangement::spaced_by(12.0)),
                            move || {
                                let copy_fields = row_fields.clone();
                                let copy_status = row_status.clone();
                                primary_button("Copy Markdown", move || {
                                    let draft = PostDraft::from_fields(&copy_fields);
                                    match copy_markdown(&draft.markdown()) {
                                        Ok(_) => copy_status
                                            .set("Markdown copied to the clipboard.".to_string()),
                                        Err(error) => copy_status
                                            .set(format!("Clipboard copy failed: {error}")),
                                    }
                                });

                                let render_fields = row_fields.clone();
                                let render_status = row_status.clone();
                                let render_preview = row_preview.clone();
                                primary_button("Render Preview", move || {
                                    let draft = PostDraft::from_fields(&render_fields);
                                    match generate_preview(&draft) {
                                        Ok(preview) => {
                                            let path = preview.preview_png_path.clone();
                                            render_preview.set(preview);
                                            render_status
                                                .set(format!("Preview regenerated at {path}"));
                                        }
                                        Err(error) => render_status
                                            .set(format!("Preview generation failed: {error}")),
                                    }
                                });

                                let save_fields = row_fields.clone();
                                let save_status = row_status.clone();
                                let save_preview = row_preview.clone();
                                primary_button("Save WebP", move || {
                                    let draft = PostDraft::from_fields(&save_fields);
                                    match save_webp(&draft) {
                                        Ok(preview) => {
                                            let saved_to = preview
                                                .last_saved_webp_path
                                                .clone()
                                                .unwrap_or_else(|| "output directory".to_string());
                                            save_preview.set(preview);
                                            save_status.set(format!("WebP saved to {saved_to}"));
                                        }
                                        Err(error) => {
                                            save_status.set(format!("Saving WebP failed: {error}"))
                                        }
                                    }
                                });
                            },
                        );

                        Text(status.clone(), Modifier::empty(), accent_style());

                        let preview = preview_state.value();
                        if !preview.preview_png_path.is_empty() {
                            Text(
                                format!("Latest preview: {}", preview.preview_png_path),
                                Modifier::empty(),
                                body_style(),
                            );
                        }
                        if let Some(saved_webp) = preview.last_saved_webp_path {
                            Text(
                                format!("Latest WebP: {saved_webp}"),
                                Modifier::empty(),
                                body_style(),
                            );
                        }
                    }
                },
            );
        }
    });
}

#[composable]
fn PreviewCard(preview_state: MutableState<PreviewState>) {
    section_card({
        let preview_state = preview_state.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let preview_state = preview_state.clone();
                    move || {
                        let preview = preview_state.value();
                        Text("Card Preview", Modifier::empty(), heading_style(28.0));
                        ComposeBox(
                            Modifier::empty()
                                .size(Size {
                                    width: 1200.0,
                                    height: 675.0,
                                })
                                .background(panel_surface())
                                .rounded_corners(24.0)
                                .padding(18.0),
                            BoxSpec::default().content_alignment(Alignment::CENTER),
                            move || {
                                Image(
                                    BitmapPainter(preview.bitmap.clone()),
                                    Some("Generated preview".to_string()),
                                    Modifier::empty().fill_max_size().rounded_corners(18.0),
                                    Alignment::CENTER,
                                    ContentScale::Fit,
                                    DEFAULT_ALPHA,
                                    None,
                                );
                            },
                        );
                    }
                },
            );
        }
    });
}

#[composable]
fn MarkdownCard(markdown_preview: String) {
    section_card({
        let markdown_preview = markdown_preview.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(12.0)),
                {
                    let markdown_preview = markdown_preview.clone();
                    move || {
                        let markdown_content = markdown_preview.clone();
                        Text("Markdown Output", Modifier::empty(), heading_style(28.0));
                        ComposeBox(
                            Modifier::empty()
                                .fill_max_width()
                                .background(panel_surface())
                                .rounded_corners(20.0)
                                .padding(18.0),
                            BoxSpec::default(),
                            move || {
                                Text(
                                    markdown_content.clone(),
                                    Modifier::empty().fill_max_width(),
                                    code_text_style(18.0),
                                );
                            },
                        );
                    }
                },
            );
        }
    });
}

#[composable]
fn ProblemMetaCard(fields: EditorFields) {
    section_card({
        let fields = fields.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    move || {
                        let date = fields.date.clone();
                        let problem_title = fields.problem_title.clone();
                        let problem_url = fields.problem_url.clone();
                        let difficulty = fields.difficulty.clone();
                        let blog_post_url = fields.blog_post_url.clone();
                        let substack_url = fields.substack_url.clone();
                        let youtube_url = fields.youtube_url.clone();
                        let reference_url = fields.reference_url.clone();
                        let telegram_text = fields.telegram_text.clone();

                        Text("Problem Meta", Modifier::empty(), heading_style(28.0));
                        labeled_field("Date", date, 1, 1);
                        labeled_field("Problem Title", problem_title, 1, 2);
                        labeled_field("Problem URL", problem_url, 1, 2);
                        labeled_field("Difficulty", difficulty, 1, 1);
                        labeled_field("Blog Post URL", blog_post_url, 1, 2);
                        labeled_field("Substack URL", substack_url, 1, 2);
                        labeled_field("YouTube URL", youtube_url, 1, 2);
                        labeled_field("Reference URL", reference_url, 1, 2);
                        labeled_field("Telegram CTA Text", telegram_text, 3, 5);
                    }
                },
            );
        }
    });
}

#[composable]
fn WriteupCard(fields: EditorFields) {
    section_card({
        let fields = fields.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    move || {
                        let problem_tldr = fields.problem_tldr.clone();
                        let intuition = fields.intuition.clone();
                        let approach = fields.approach.clone();
                        let time_complexity = fields.time_complexity.clone();
                        let space_complexity = fields.space_complexity.clone();

                        Text("Writeup", Modifier::empty(), heading_style(28.0));
                        labeled_field("Problem TLDR", problem_tldr, 3, 6);
                        labeled_field("Intuition", intuition, 6, 14);
                        labeled_field("Approach", approach, 6, 14);
                        labeled_field("Time Complexity Inner Value", time_complexity, 1, 2);
                        labeled_field("Space Complexity Inner Value", space_complexity, 1, 2);
                    }
                },
            );
        }
    });
}

#[composable]
fn CodeCard(fields: EditorFields) {
    section_card({
        let fields = fields.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    move || {
                        let kotlin_runtime_ms = fields.kotlin_runtime_ms.clone();
                        let kotlin_code = fields.kotlin_code.clone();
                        let rust_runtime_ms = fields.rust_runtime_ms.clone();
                        let rust_code = fields.rust_code.clone();

                        Text("Code Blocks", Modifier::empty(), heading_style(28.0));
                        labeled_field("Kotlin Runtime (ms)", kotlin_runtime_ms, 1, 1);
                        labeled_code_field("Kotlin Code", kotlin_code, 10, 18);
                        labeled_field("Rust Runtime (ms)", rust_runtime_ms, 1, 1);
                        labeled_code_field("Rust Code", rust_code, 10, 18);
                    }
                },
            );
        }
    });
}

#[derive(Clone)]
struct BootState {
    preview: PreviewState,
    status: String,
}

#[composable]
fn section_card(content: impl FnMut() + 'static) {
    ComposeBox(
        Modifier::empty()
            .fill_max_width()
            .background(card_surface())
            .rounded_corners(26.0)
            .padding(22.0),
        BoxSpec::default(),
        content,
    );
}

#[composable]
fn primary_button(label: &'static str, on_click: impl FnMut() + 'static) {
    Button(
        Modifier::empty()
            .background(button_surface())
            .rounded_corners(18.0)
            .padding_symmetric(20.0, 14.0),
        on_click,
        move || {
            Text(label, Modifier::empty(), button_text_style());
        },
    );
}

#[composable]
fn tab_button(label: &'static str, selected: bool, on_click: impl FnMut() + 'static) {
    let background = if selected {
        button_surface()
    } else {
        panel_surface()
    };
    Button(
        Modifier::empty()
            .background(background)
            .rounded_corners(18.0)
            .padding_symmetric(20.0, 14.0),
        on_click,
        move || {
            Text(label, Modifier::empty(), tab_text_style(selected));
        },
    );
}

#[composable]
fn labeled_field(label: &'static str, state: TextFieldState, min_lines: usize, max_lines: usize) {
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            Text(label, Modifier::empty(), label_style());

            let field_state = state.clone();
            ComposeBox(
                Modifier::empty()
                    .fill_max_width()
                    .background(panel_surface())
                    .rounded_corners(18.0)
                    .padding(14.0),
                BoxSpec::default(),
                move || {
                    BasicTextFieldWithOptions(
                        field_state.clone(),
                        Modifier::empty().fill_max_width(),
                        BasicTextFieldOptions {
                            text_style: field_text_style(),
                            cursor_color: Color::from_rgb_u8(255, 194, 85),
                            line_limits: if min_lines == 1 && max_lines == 1 {
                                TextFieldLineLimits::SingleLine
                            } else {
                                TextFieldLineLimits::MultiLine {
                                    min_lines,
                                    max_lines,
                                }
                            },
                        },
                    );
                },
            );
        },
    );
}

#[composable]
fn labeled_code_field(
    label: &'static str,
    state: TextFieldState,
    min_lines: usize,
    max_lines: usize,
) {
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            Text(label, Modifier::empty(), label_style());

            let field_state = state.clone();
            ComposeBox(
                Modifier::empty()
                    .fill_max_width()
                    .background(panel_surface())
                    .rounded_corners(18.0)
                    .padding(14.0),
                BoxSpec::default(),
                move || {
                    BasicTextFieldWithOptions(
                        field_state.clone(),
                        Modifier::empty().fill_max_width(),
                        BasicTextFieldOptions {
                            text_style: code_field_style(),
                            cursor_color: Color::from_rgb_u8(255, 194, 85),
                            line_limits: if min_lines == 1 && max_lines == 1 {
                                TextFieldLineLimits::SingleLine
                            } else {
                                TextFieldLineLimits::MultiLine {
                                    min_lines,
                                    max_lines,
                                }
                            },
                        },
                    );
                },
            );
        },
    );
}

fn copy_markdown(markdown: &str) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(markdown.to_string())?;
        return Ok(());
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = markdown;
        Err(anyhow!(
            "clipboard copy is not implemented in the web build yet"
        ))
    }
}

fn heading_style(size: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(244, 247, 250)),
            font_size: cranpose::text::TextUnit::Sp(size),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn body_style() -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(189, 204, 217)),
            font_size: cranpose::text::TextUnit::Sp(18.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn accent_style() -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(255, 195, 90)),
            font_size: cranpose::text::TextUnit::Sp(17.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn code_text_style(size: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(232, 238, 245)),
            font_size: cranpose::text::TextUnit::Sp(size),
            font_family: Some(cranpose::text::FontFamily::Monospace),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn field_text_style() -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(232, 238, 245)),
            font_size: cranpose::text::TextUnit::Sp(18.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn code_field_style() -> TextStyle {
    code_text_style(18.0)
}

fn label_style() -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(132, 226, 255)),
            font_size: cranpose::text::TextUnit::Sp(16.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn button_text_style() -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(14, 18, 24)),
            font_size: cranpose::text::TextUnit::Sp(17.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn tab_text_style(selected: bool) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(if selected {
                Color::from_rgb_u8(14, 18, 24)
            } else {
                Color::from_rgb_u8(215, 224, 233)
            }),
            font_size: cranpose::text::TextUnit::Sp(17.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn ui_surface() -> Color {
    Color::from_rgb_u8(8, 14, 23)
}

fn card_surface() -> Color {
    Color::from_rgb_u8(12, 21, 33)
}

fn panel_surface() -> Color {
    Color::from_rgb_u8(18, 28, 43)
}

fn button_surface() -> Color {
    Color::from_rgb_u8(255, 194, 85)
}

#[cfg(test)]
mod tests {
    use super::{APP_HEIGHT, APP_WIDTH, WEB_SURFACE_MAX_DIM, compute_web_canvas_size};

    #[test]
    fn web_canvas_size_stays_under_surface_limit() {
        let (width, height) = compute_web_canvas_size(APP_WIDTH as f64, APP_HEIGHT as f64, 1.5);
        assert!((width as f64 * 1.5).ceil() <= WEB_SURFACE_MAX_DIM as f64);
        assert!((height as f64 * 1.5).ceil() <= WEB_SURFACE_MAX_DIM as f64);
    }

    #[test]
    fn web_canvas_size_respects_viewport() {
        let (width, height) = compute_web_canvas_size(980.0, 740.0, 1.0);
        assert_eq!((width, height), (980, 740));
    }
}
