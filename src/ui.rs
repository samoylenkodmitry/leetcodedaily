#![allow(non_snake_case)]

use crate::draft::{
    EditorFields, PostDraft, autosave_destination_label, persist_autosave, startup_status_message,
};
use crate::export::{
    CardRenderPlan, ComposePreviewAssets, PreviewState, compose_preview_assets,
    compose_preview_plan, render_preview_frame, save_webp,
};
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
#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Object, Promise, Reflect};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::{JsFuture, spawn_local};
#[cfg(target_arch = "wasm32")]
use web_sys::{Blob, BlobPropertyBag, ClipboardItem};

const APP_WIDTH: u32 = 1480;
const APP_HEIGHT: u32 = 1560;
#[cfg(any(test, target_arch = "wasm32"))]
const WEB_SURFACE_MAX_DIM: u32 = 1900;
#[cfg(target_arch = "wasm32")]
const WEB_CANVAS_MARGIN: f64 = 48.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum EditorTab {
    Output,
    Compose,
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
    let fields = remember(EditorFields::load_initial).with(|fields| fields.clone());
    let autosave_destination = remember(autosave_destination_label).with(|label| label.clone());
    let compose_assets = remember(|| compose_preview_assets().ok()).with(|assets| assets.clone());
    let active_tab = useState(|| EditorTab::Meta);
    let preview_state = useState(PreviewState::placeholder);
    let preview_loading = useState(|| false);
    let compose_plan = useState(|| None::<CardRenderPlan>);
    let compose_loading = useState(|| false);
    let status = useState(startup_status_message);
    let current_draft = PostDraft::from_fields(&fields);
    let markdown_preview = current_draft.blog_template();
    let current_tab = active_tab.value();

    cranpose_core::LaunchedEffect!(current_draft.clone(), {
        let draft = current_draft.clone();
        let status = status.clone();
        move |_scope| {
            if let Err(error) = persist_autosave(&draft) {
                status.set(format!("Autosave failed: {error}"));
            }
        }
    });

    cranpose_core::LaunchedEffect!(current_tab, {
        let draft = current_draft.clone();
        let current_tab = current_tab;
        let preview_state = preview_state.clone();
        let preview_loading = preview_loading.clone();
        let status = status.clone();
        move |scope| {
            if current_tab != EditorTab::Output {
                return;
            }
            preview_loading.set(true);
            preview_state.set(PreviewState::placeholder());
            status.set("Generating preview...".to_string());

            let preview_state = preview_state.clone();
            let preview_loading = preview_loading.clone();
            let status = status.clone();
            scope.launch_background(
                move |_| async move { render_preview_frame(&draft) },
                move |result| {
                    preview_loading.set(false);
                    match result {
                        Ok(frame) => match PreviewState::from_frame(frame) {
                            Ok(preview) => {
                                preview_state.set(preview);
                                status.set("Preview refreshed.".to_string());
                            }
                            Err(error) => {
                                status.set(format!("Preview generation failed: {error}"));
                            }
                        },
                        Err(error) => status.set(format!("Preview generation failed: {error}")),
                    }
                },
            );
        }
    });

    cranpose_core::LaunchedEffect!((current_tab, current_draft.clone()), {
        let draft = current_draft.clone();
        let current_tab = current_tab;
        let compose_loading = compose_loading.clone();
        let compose_plan = compose_plan.clone();
        let status = status.clone();
        move |scope| {
            if current_tab != EditorTab::Compose {
                return;
            }
            compose_loading.set(true);
            compose_plan.set(None);

            let compose_loading = compose_loading.clone();
            let compose_plan = compose_plan.clone();
            let status = status.clone();
            scope.launch_background(
                move |_| async move { compose_preview_plan(&draft) },
                move |result| {
                    compose_loading.set(false);
                    match result {
                        Ok(plan) => compose_plan.set(Some(plan)),
                        Err(error) => status.set(format!("Cranpose preview failed: {error}")),
                    }
                },
            );
        }
    });

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
            let preview_loading = preview_loading.clone();
            let compose_plan = compose_plan.clone();
            let compose_loading = compose_loading.clone();
            let markdown_preview = markdown_preview.clone();
            let active_tab = active_tab.clone();
            let scroll_state = scroll_state.clone();
            let autosave_destination = autosave_destination.clone();
            let compose_assets = compose_assets.clone();
            move || {
                ActionsCard(
                    fields.clone(),
                    status.clone(),
                    preview_state.clone(),
                    autosave_destination.clone(),
                );
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
                                    tab_button(
                                        "Cranpose",
                                        active_tab.value() == EditorTab::Compose,
                                        {
                                            let active_tab = active_tab.clone();
                                            let scroll_state = scroll_state.clone();
                                            move || {
                                                active_tab.set(EditorTab::Compose);
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
                    current_draft.clone(),
                    fields.clone(),
                    preview_state.clone(),
                    preview_loading.clone(),
                    compose_assets.clone(),
                    compose_plan.clone(),
                    compose_loading.clone(),
                    markdown_preview.clone(),
                    status.clone(),
                );
            }
        },
    );
}

#[composable]
fn ActiveTabContent(
    active_tab: EditorTab,
    draft: PostDraft,
    fields: EditorFields,
    preview_state: MutableState<PreviewState>,
    preview_loading: MutableState<bool>,
    compose_assets: Option<ComposePreviewAssets>,
    compose_plan: MutableState<Option<CardRenderPlan>>,
    compose_loading: MutableState<bool>,
    markdown_preview: String,
    status: MutableState<String>,
) {
    match active_tab {
        EditorTab::Output => {
            PreviewCard(preview_state, preview_loading);
            MarkdownCard(markdown_preview);
        }
        EditorTab::Compose => {
            ComposePreviewCard(draft, compose_assets, compose_plan, compose_loading)
        }
        EditorTab::Meta => ProblemMetaCard(fields, status),
        EditorTab::Writeup => WriteupCard(fields, status),
        EditorTab::Code => CodeCard(fields, status),
    }
}

#[composable]
fn ActionsCard(
    fields: EditorFields,
    status: MutableState<String>,
    preview_state: MutableState<PreviewState>,
    autosave_destination: String,
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
                    let autosave_destination = autosave_destination.clone();
                    move || {
                        Text(
                            "LeetCode Daily Composer",
                            Modifier::empty(),
                            heading_style(34.0),
                        );
                        Text(
                            "Fill the template, open Output for the raster export or Cranpose for the live text-rendered comparison, then copy the platform-specific template you need or save the card as WebP.",
                            Modifier::empty(),
                            body_style(),
                        );
                        Text(
                            autosave_destination.clone(),
                            Modifier::empty(),
                            muted_style(),
                        );

                        let action_fields = fields.clone();
                        let action_status = status.clone();
                        let action_preview = preview_state.clone();
                        Column(
                            Modifier::empty().fill_max_width(),
                            ColumnSpec::default()
                                .vertical_arrangement(LinearArrangement::spaced_by(12.0)),
                            move || {
                                let row_fields = action_fields.clone();
                                let row_status = action_status.clone();
                                Row(
                                    Modifier::empty().fill_max_width(),
                                    RowSpec::default()
                                        .horizontal_arrangement(LinearArrangement::spaced_by(12.0)),
                                    move || {
                                        let leetcode_fields = row_fields.clone();
                                        let leetcode_status = row_status.clone();
                                        primary_button("Copy LeetCode", move || {
                                            let draft = PostDraft::from_fields(&leetcode_fields);
                                            copy_text_to_clipboard(
                                                draft.leetcode_template(),
                                                "LeetCode template copied.".to_string(),
                                                leetcode_status.clone(),
                                            );
                                        });

                                        let youtube_fields = row_fields.clone();
                                        let youtube_status = row_status.clone();
                                        primary_button("Copy YouTube", move || {
                                            let draft = PostDraft::from_fields(&youtube_fields);
                                            copy_text_to_clipboard(
                                                draft.youtube_template(),
                                                "YouTube template copied.".to_string(),
                                                youtube_status.clone(),
                                            );
                                        });

                                        let blog_fields = row_fields.clone();
                                        let blog_status = row_status.clone();
                                        primary_button("Copy Blog", move || {
                                            let draft = PostDraft::from_fields(&blog_fields);
                                            copy_text_to_clipboard(
                                                draft.blog_template(),
                                                "Blog template copied.".to_string(),
                                                blog_status.clone(),
                                            );
                                        });

                                        let telegram_fields = row_fields.clone();
                                        let telegram_status = row_status.clone();
                                        primary_button("Copy Telegram", move || {
                                            let draft = PostDraft::from_fields(&telegram_fields);
                                            copy_text_to_clipboard(
                                                draft.telegram_template(),
                                                "Telegram template copied.".to_string(),
                                                telegram_status.clone(),
                                            );
                                        });
                                    },
                                );

                                let row_fields = action_fields.clone();
                                let row_status = action_status.clone();
                                let row_preview = action_preview.clone();
                                Row(
                                    Modifier::empty().fill_max_width(),
                                    RowSpec::default()
                                        .horizontal_arrangement(LinearArrangement::spaced_by(12.0)),
                                    move || {
                                        let rich_fields = row_fields.clone();
                                        let rich_status = row_status.clone();
                                        primary_button("Copy Rich Text", move || {
                                            let draft = PostDraft::from_fields(&rich_fields);
                                            copy_rich_text_to_clipboard(draft, rich_status.clone());
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
                                                        .unwrap_or_else(|| {
                                                            "~/Downloads".to_string()
                                                        });
                                                    save_preview.set(preview);
                                                    save_status
                                                        .set(format!("WebP saved to {saved_to}"));
                                                }
                                                Err(error) => save_status
                                                    .set(format!("Saving WebP failed: {error}")),
                                            }
                                        });
                                    },
                                );
                            },
                        );

                        Text(status.clone(), Modifier::empty(), accent_style());

                        if let Some(saved_webp) = preview_state.value().last_saved_webp_path {
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
fn PreviewCard(preview_state: MutableState<PreviewState>, preview_loading: MutableState<bool>) {
    section_card({
        let preview_state = preview_state.clone();
        let preview_loading = preview_loading.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let preview_state = preview_state.clone();
                    let preview_loading = preview_loading.clone();
                    move || {
                        let preview = preview_state.value();
                        Text("Card Preview", Modifier::empty(), heading_style(28.0));
                        if preview_loading.value() {
                            Text(
                                "Rendering preview in the background...",
                                Modifier::empty(),
                                accent_style(),
                            );
                        }
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
fn ComposePreviewCard(
    draft: PostDraft,
    compose_assets: Option<ComposePreviewAssets>,
    compose_plan: MutableState<Option<CardRenderPlan>>,
    compose_loading: MutableState<bool>,
) {
    section_card({
        let compose_assets = compose_assets.clone();
        let compose_plan = compose_plan.clone();
        let compose_loading = compose_loading.clone();
        let draft = draft.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let compose_assets = compose_assets.clone();
                    let compose_plan = compose_plan.clone();
                    let compose_loading = compose_loading.clone();
                    let draft = draft.clone();
                    move || {
                        Text("Cranpose Preview", Modifier::empty(), heading_style(28.0));
                        Text(
                            "This tab uses Cranpose text rendering directly so you can compare it against the raster export.",
                            Modifier::empty(),
                            body_style(),
                        );
                        if compose_loading.value() {
                            Text(
                                "Preparing Cranpose preview in the background...",
                                Modifier::empty(),
                                accent_style(),
                            );
                        }

                        let plan = compose_plan.value();
                        let preview_draft = draft.clone();
                        let preview_assets = compose_assets.clone();
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
                                CranposeCardSurface(
                                    preview_draft.clone(),
                                    preview_assets.clone(),
                                    plan.clone(),
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
fn CranposeCardSurface(
    draft: PostDraft,
    compose_assets: Option<ComposePreviewAssets>,
    compose_plan: Option<CardRenderPlan>,
) {
    let scale = 1200.0 / 1600.0;
    let background = compose_assets
        .as_ref()
        .map(|assets| assets.background.clone());
    let qr = compose_assets.as_ref().map(|assets| assets.qr.clone());

    ComposeBox(
        Modifier::empty().fill_max_size(),
        BoxSpec::default(),
        move || {
            if let Some(background) = background.clone() {
                Image(
                    BitmapPainter(background),
                    Some("Cranpose card background".to_string()),
                    Modifier::empty().fill_max_size().rounded_corners(18.0),
                    Alignment::CENTER,
                    ContentScale::Crop,
                    DEFAULT_ALPHA,
                    None,
                );
            }

            if let Some(plan) = compose_plan.clone() {
                ComposeBox(
                    Modifier::empty()
                        .absolute_offset(scale_x(plan.panel.x, scale), scale_y(plan.panel.y, scale))
                        .size(scaled_size(plan.panel.width, plan.panel.height, scale))
                        .background(Color::from_rgba_u8(5, 8, 14, 210))
                        .rounded_corners(34.0),
                    BoxSpec::default(),
                    || {},
                );

                if let Some(qr) = qr.clone() {
                    Image(
                        BitmapPainter(qr),
                        Some("QR overlay".to_string()),
                        Modifier::empty()
                            .absolute_offset(scale_x(plan.qr.x, scale), scale_y(plan.qr.y, scale))
                            .size(scaled_size(plan.qr.width, plan.qr.height, scale))
                            .rounded_corners(18.0),
                        Alignment::CENTER,
                        ContentScale::Fit,
                        DEFAULT_ALPHA * 0.72,
                        None,
                    );
                }

                for code_block in plan.code_blocks.clone() {
                    BasicText(
                        format!("// {}", code_block.language),
                        Modifier::empty().absolute_offset(
                            scale_x(code_block.text_x, scale),
                            scale_y(code_block.title_y, scale),
                        ),
                        preview_code_label_style(code_block.label_font_size * scale),
                        cranpose::text::TextOverflow::Visible,
                        false,
                        1,
                        1,
                    );
                    BasicText(
                        format!("// {}", code_block.runtime),
                        Modifier::empty().absolute_offset(
                            scale_x(code_block.text_x, scale),
                            scale_y(code_block.runtime_y, scale),
                        ),
                        preview_runtime_style(code_block.label_font_size * scale),
                        cranpose::text::TextOverflow::Visible,
                        false,
                        1,
                        1,
                    );
                    BasicText(
                        code_block.lines.join("\n"),
                        Modifier::empty().absolute_offset(
                            scale_x(code_block.text_x, scale),
                            scale_y(code_block.code_y, scale),
                        ),
                        preview_code_style(
                            code_block.code_font_size * scale,
                            code_block.code_line_height as f32 * scale,
                        ),
                        cranpose::text::TextOverflow::Visible,
                        false,
                        usize::MAX,
                        1,
                    );
                }

                BasicText(
                    plan.tldr.lines.join("\n"),
                    Modifier::empty()
                        .absolute_offset(scale_x(plan.tldr.x, scale), scale_y(plan.tldr.y, scale)),
                    preview_tldr_style(
                        plan.tldr.font_size * scale,
                        plan.tldr.line_height as f32 * scale,
                    ),
                    cranpose::text::TextOverflow::Visible,
                    false,
                    usize::MAX,
                    1,
                );
            } else {
                Text(
                    draft.preview_tldr(),
                    Modifier::empty()
                        .absolute_offset(54.0, 572.0)
                        .fill_max_width(),
                    preview_tldr_style(16.0, 22.0),
                );
            }
        },
    );
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
                        Text(
                            "Blog Template Preview",
                            Modifier::empty(),
                            heading_style(28.0),
                        );
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
fn ProblemMetaCard(fields: EditorFields, status: MutableState<String>) {
    section_card({
        let fields = fields.clone();
        let status = status.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
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
                        labeled_field("Date", date, 1, 1, status.clone(), true);
                        labeled_field("Problem Title", problem_title, 1, 2, status.clone(), true);
                        labeled_field("Problem URL", problem_url, 1, 2, status.clone(), true);
                        labeled_field("Difficulty", difficulty, 1, 1, status.clone(), true);
                        labeled_field("Blog Post URL", blog_post_url, 1, 2, status.clone(), true);
                        labeled_field("Substack URL", substack_url, 1, 2, status.clone(), true);
                        labeled_field("YouTube URL", youtube_url, 1, 2, status.clone(), true);
                        labeled_field("Reference URL", reference_url, 1, 2, status.clone(), true);
                        labeled_field(
                            "Telegram CTA Text",
                            telegram_text,
                            3,
                            5,
                            status.clone(),
                            true,
                        );
                    }
                },
            );
        }
    });
}

#[composable]
fn WriteupCard(fields: EditorFields, status: MutableState<String>) {
    section_card({
        let fields = fields.clone();
        let status = status.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    move || {
                        let problem_tldr = fields.problem_tldr.clone();
                        let intuition = fields.intuition.clone();
                        let approach = fields.approach.clone();
                        let time_complexity = fields.time_complexity.clone();
                        let space_complexity = fields.space_complexity.clone();

                        Text("Writeup", Modifier::empty(), heading_style(28.0));
                        labeled_field("Problem TLDR", problem_tldr, 3, 6, status.clone(), true);
                        labeled_field("Intuition", intuition, 6, 14, status.clone(), true);
                        labeled_field("Approach", approach, 6, 14, status.clone(), true);
                        labeled_field(
                            "Time Complexity Inner Value",
                            time_complexity,
                            1,
                            2,
                            status.clone(),
                            false,
                        );
                        labeled_field(
                            "Space Complexity Inner Value",
                            space_complexity,
                            1,
                            2,
                            status.clone(),
                            false,
                        );
                    }
                },
            );
        }
    });
}

#[composable]
fn CodeCard(fields: EditorFields, status: MutableState<String>) {
    section_card({
        let fields = fields.clone();
        let status = status.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    move || {
                        let kotlin_runtime_ms = fields.kotlin_runtime_ms.clone();
                        let kotlin_code = fields.kotlin_code.clone();
                        let rust_runtime_ms = fields.rust_runtime_ms.clone();
                        let rust_code = fields.rust_code.clone();

                        Text("Code Blocks", Modifier::empty(), heading_style(28.0));
                        labeled_field(
                            "Kotlin Runtime (ms)",
                            kotlin_runtime_ms,
                            1,
                            1,
                            status.clone(),
                            false,
                        );
                        labeled_code_field("Kotlin Code", kotlin_code, 10, 18, status.clone());
                        labeled_field(
                            "Rust Runtime (ms)",
                            rust_runtime_ms,
                            1,
                            1,
                            status.clone(),
                            false,
                        );
                        labeled_code_field("Rust Code", rust_code, 10, 18, status.clone());
                    }
                },
            );
        }
    });
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
fn subtle_button(label: &'static str, on_click: impl FnMut() + 'static) {
    Button(
        Modifier::empty()
            .background(panel_surface())
            .rounded_corners(14.0)
            .padding_symmetric(14.0, 10.0),
        on_click,
        move || {
            Text(label, Modifier::empty(), subtle_button_text_style());
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
fn labeled_field(
    label: &'static str,
    state: TextFieldState,
    min_lines: usize,
    max_lines: usize,
    status: MutableState<String>,
    allow_paste: bool,
) {
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            field_header(label, state.clone(), status.clone(), allow_paste);

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
    status: MutableState<String>,
) {
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            field_header(label, state.clone(), status.clone(), true);

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

#[composable]
fn field_header(
    label: &'static str,
    state: TextFieldState,
    status: MutableState<String>,
    allow_paste: bool,
) {
    Row(
        Modifier::empty().fill_max_width(),
        RowSpec::default().horizontal_arrangement(LinearArrangement::SpaceBetween),
        move || {
            Text(label, Modifier::empty(), label_style());
            if allow_paste {
                let paste_state = state.clone();
                let paste_status = status.clone();
                subtle_button("Paste", move || {
                    paste_text_from_clipboard(paste_state.clone(), paste_status.clone(), label);
                });
            }
        },
    );
}

fn copy_text_to_clipboard(text: String, success_message: String, status: MutableState<String>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match copy_text(&text) {
            Ok(_) => status.set(success_message),
            Err(error) => status.set(format!("Clipboard copy failed: {error}")),
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        match web_write_text_promise(&text) {
            Ok(promise) => {
                track_web_promise(
                    promise,
                    success_message,
                    "Clipboard copy failed".to_string(),
                    status,
                );
            }
            Err(error) => status.set(format!("Clipboard copy failed: {error}")),
        }
    }
}

fn copy_rich_text_to_clipboard(draft: PostDraft, status: MutableState<String>) {
    let html = draft.rich_html();
    let fallback = draft.blog_template();

    #[cfg(not(target_arch = "wasm32"))]
    {
        match copy_rich_text(&html, &fallback) {
            Ok(_) => status.set("Rich text copied to the clipboard.".to_string()),
            Err(error) => status.set(format!("Rich text copy failed: {error}")),
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        match web_write_rich_text_promise(&html, &fallback) {
            Ok(promise) => {
                track_web_promise(
                    promise,
                    "Rich text copied to the clipboard.".to_string(),
                    "Rich text copy failed".to_string(),
                    status,
                );
            }
            Err(error) => status.set(format!("Rich text copy failed: {error}")),
        }
    }
}

fn paste_text_from_clipboard(
    state: TextFieldState,
    status: MutableState<String>,
    label: &'static str,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match read_text_from_clipboard() {
            Ok(text) => {
                state.set_text(text);
                status.set(format!("{label} replaced from clipboard."));
            }
            Err(error) => status.set(format!("Clipboard paste failed: {error}")),
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        match web_read_text_promise() {
            Ok(promise) => {
                spawn_local(async move {
                    match JsFuture::from(promise).await {
                        Ok(value) => match value.as_string() {
                            Some(text) => {
                                state.set_text(text);
                                status.set(format!("{label} replaced from clipboard."));
                            }
                            None => status.set(
                                "Clipboard paste failed: browser returned non-text data."
                                    .to_string(),
                            ),
                        },
                        Err(error) => {
                            status.set(format!("Clipboard paste failed: {error:?}"));
                        }
                    }
                });
            }
            Err(error) => status.set(format!("Clipboard paste failed: {error}")),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_text(markdown: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(markdown.to_string())?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_rich_text(html: &str, fallback: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_html(html.to_string(), Some(fallback.to_string()))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn read_text_from_clipboard() -> Result<String> {
    let mut clipboard = Clipboard::new()?;
    clipboard.get_text().map_err(Into::into)
}

#[cfg(target_arch = "wasm32")]
fn web_write_text_promise(markdown: &str) -> Result<js_sys::Promise> {
    let window = web_sys::window().ok_or_else(|| anyhow!("missing window"))?;
    Ok(window.navigator().clipboard().write_text(markdown))
}

#[cfg(target_arch = "wasm32")]
fn web_read_text_promise() -> Result<js_sys::Promise> {
    let window = web_sys::window().ok_or_else(|| anyhow!("missing window"))?;
    Ok(window.navigator().clipboard().read_text())
}

#[cfg(target_arch = "wasm32")]
fn web_write_rich_text_promise(html: &str, fallback: &str) -> Result<js_sys::Promise> {
    let window = web_sys::window().ok_or_else(|| anyhow!("missing window"))?;
    let clipboard = window.navigator().clipboard();
    let record = Object::new();

    let html_blob = text_blob(html, "text/html")?;
    let fallback_blob = text_blob(fallback, "text/plain")?;
    let html_promise = Promise::resolve(&JsValue::from(html_blob));
    let fallback_promise = Promise::resolve(&JsValue::from(fallback_blob));

    Reflect::set(
        &record,
        &JsValue::from_str("text/html"),
        html_promise.as_ref(),
    )
    .map_err(|error| anyhow!("registering HTML clipboard data failed: {error:?}"))?;
    Reflect::set(
        &record,
        &JsValue::from_str("text/plain"),
        fallback_promise.as_ref(),
    )
    .map_err(|error| anyhow!("registering text clipboard data failed: {error:?}"))?;

    let item = ClipboardItem::new_with_record_from_str_to_blob_promise(&record)
        .map_err(|error| anyhow!("creating clipboard item failed: {error:?}"))?;
    let items = Array::new();
    items.push(item.as_ref());
    Ok(clipboard.write(items.as_ref()))
}

#[cfg(target_arch = "wasm32")]
fn text_blob(contents: &str, mime_type: &str) -> Result<Blob> {
    let parts = Array::new();
    parts.push(&JsValue::from_str(contents));
    let options = BlobPropertyBag::new();
    options.set_type(mime_type);
    Blob::new_with_str_sequence_and_options(parts.as_ref(), &options)
        .map_err(|error| anyhow!("creating {mime_type} blob failed: {error:?}"))
}

#[cfg(target_arch = "wasm32")]
fn track_web_promise(
    promise: js_sys::Promise,
    success_message: String,
    failure_prefix: String,
    status: MutableState<String>,
) {
    spawn_local(async move {
        match JsFuture::from(promise).await {
            Ok(_) => status.set(success_message),
            Err(error) => status.set(format!("{failure_prefix}: {error:?}")),
        }
    });
}

fn scale_x(value: i32, scale: f32) -> f32 {
    value as f32 * scale
}

fn scale_y(value: i32, scale: f32) -> f32 {
    value as f32 * scale
}

fn scaled_size(width: u32, height: u32, scale: f32) -> Size {
    Size {
        width: width as f32 * scale,
        height: height as f32 * scale,
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

fn muted_style() -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(126, 142, 158)),
            font_size: cranpose::text::TextUnit::Sp(15.0),
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

fn subtle_button_text_style() -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(132, 226, 255)),
            font_size: cranpose::text::TextUnit::Sp(15.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
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

fn preview_code_label_style(size: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(148, 229, 255)),
            font_size: cranpose::text::TextUnit::Sp(size.max(10.0)),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            font_family: Some(cranpose::text::FontFamily::Monospace),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle {
            line_height: cranpose::text::TextUnit::Sp((size * 1.04).max(size)),
            ..ParagraphStyle::default()
        },
    }
}

fn preview_runtime_style(size: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(255, 180, 78)),
            font_size: cranpose::text::TextUnit::Sp(size.max(10.0)),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            font_family: Some(cranpose::text::FontFamily::Monospace),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle {
            line_height: cranpose::text::TextUnit::Sp((size * 1.04).max(size)),
            ..ParagraphStyle::default()
        },
    }
}

fn preview_code_style(size: f32, line_height: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(242, 246, 250)),
            font_size: cranpose::text::TextUnit::Sp(size.max(8.0)),
            font_weight: Some(cranpose::text::FontWeight::MEDIUM),
            font_family: Some(cranpose::text::FontFamily::Monospace),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle {
            line_height: cranpose::text::TextUnit::Sp(line_height.max(size)),
            ..ParagraphStyle::default()
        },
    }
}

fn preview_tldr_style(size: f32, line_height: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(170, 176, 187)),
            font_size: cranpose::text::TextUnit::Sp(size.max(10.0)),
            font_weight: Some(cranpose::text::FontWeight::MEDIUM),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle {
            line_height: cranpose::text::TextUnit::Sp(line_height.max(size)),
            ..ParagraphStyle::default()
        },
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
