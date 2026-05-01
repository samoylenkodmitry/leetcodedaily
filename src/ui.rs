#![allow(non_snake_case)]

use crate::draft::{
    EditorFields, PostDraft, ThemeMode, UiPreferences, autosave_destination_label,
    load_initial_draft, load_ui_preferences, persist_autosave, persist_ui_preferences,
    startup_status_message,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::draft::{read_draft_snapshot, write_draft_snapshot};
#[cfg(not(target_arch = "wasm32"))]
use crate::export::{
    CardRenderPlan, CodeRenderPlan, ComposePreviewAssets, compose_preview_assets,
    compose_preview_plan,
};
use crate::export::{
    PreviewFrame, PreviewState, render_preview_frame, save_preview_frame_as_webp, save_webp,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::publish::{ArchiveEdit, publish_blog_post};
#[cfg(not(target_arch = "wasm32"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(target_arch = "wasm32")]
use anyhow::anyhow;
#[cfg(not(target_arch = "wasm32"))]
use arboard::Clipboard;
use cranpose::Box as ComposeBox;
use cranpose::DEFAULT_ALPHA;
use cranpose::prelude::*;
use cranpose::widgets::BasicTextFieldWithOptions;
use cranpose_animation::{
    AnimationSpec, RepeatMode, StartOffset, infiniteRepeatable, rememberInfiniteTransition,
};
use cranpose_core::MutableState;
use cranpose_foundation::text::{TextFieldLineLimits, TextFieldState};
#[cfg(not(target_arch = "wasm32"))]
use image::{ImageFormat, RgbaImage};
#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Object, Promise, Reflect};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ActionButtonId {
    RefreshRasterPreview,
    RefreshCranposePreview,
    CopyLeetcode,
    CopyYoutube,
    CopyBlog,
    CopyTelegram,
    CopyTitle,
    CopySubtitle,
    CopyRichText,
    SaveRasterWebp,
    SaveCranposeWebp,
    PublishBlog,
    PostTelegram,
    PostTelegramComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum LongAction {
    RefreshRasterPreview,
    RefreshCranposePreview,
    SaveRasterWebp,
    SaveCranposeWebp,
    PublishBlog,
    PostTelegram,
    PostTelegramComment,
}

#[derive(Clone, Debug, PartialEq, Hash)]
struct PendingAction {
    action: LongAction,
    request_id: u64,
    draft: PostDraft,
    telegram_post_link: String,
}

#[derive(Clone)]
enum LongActionResult {
    RefreshRasterPreview(std::result::Result<PreviewState, String>),
    RefreshCranposePreview(std::result::Result<PreviewState, String>),
    SaveRasterWebp(std::result::Result<PreviewState, String>),
    SaveCranposeWebp(std::result::Result<PreviewState, String>),
    PublishBlog(std::result::Result<PublishBlogOutcome, String>),
    PostTelegram(std::result::Result<TelegramPostOutcome, String>),
    PostTelegramComment(std::result::Result<String, String>),
}

#[derive(Clone)]
struct PublishBlogOutcome {
    preview: PreviewState,
    edit: BlogArchiveEdit,
    commit_sha: Option<String>,
}

#[derive(Clone)]
struct TelegramPostOutcome {
    preview: PreviewState,
    link: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
enum BlogArchiveEdit {
    Inserted,
    Replaced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EditorFieldId {
    Date,
    ProblemTitle,
    ProblemUrl,
    Difficulty,
    BlogPostUrl,
    SubstackUrl,
    YoutubeUrl,
    ReferenceUrl,
    TelegramText,
    ProblemTldr,
    Intuition,
    Approach,
    TimeComplexity,
    SpaceComplexity,
    KotlinRuntimeMs,
    KotlinCode,
    RustRuntimeMs,
    RustCode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WorkStage {
    Prepare,
    Write,
    Code,
    Review,
    Ship,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum NextWorkItem {
    Field(EditorFieldId),
    Action(ActionButtonId),
}

const ACTION_BUTTONS: [ActionButtonId; 14] = [
    ActionButtonId::CopyLeetcode,
    ActionButtonId::CopyYoutube,
    ActionButtonId::CopyBlog,
    ActionButtonId::CopyTelegram,
    ActionButtonId::CopyTitle,
    ActionButtonId::CopySubtitle,
    ActionButtonId::CopyRichText,
    ActionButtonId::RefreshRasterPreview,
    ActionButtonId::RefreshCranposePreview,
    ActionButtonId::SaveRasterWebp,
    ActionButtonId::SaveCranposeWebp,
    ActionButtonId::PublishBlog,
    ActionButtonId::PostTelegram,
    ActionButtonId::PostTelegramComment,
];

const META_FIELDS: [EditorFieldId; 9] = [
    EditorFieldId::Date,
    EditorFieldId::ProblemTitle,
    EditorFieldId::ProblemUrl,
    EditorFieldId::Difficulty,
    EditorFieldId::BlogPostUrl,
    EditorFieldId::SubstackUrl,
    EditorFieldId::YoutubeUrl,
    EditorFieldId::ReferenceUrl,
    EditorFieldId::TelegramText,
];

const WRITEUP_FIELDS: [EditorFieldId; 5] = [
    EditorFieldId::ProblemTldr,
    EditorFieldId::Intuition,
    EditorFieldId::Approach,
    EditorFieldId::TimeComplexity,
    EditorFieldId::SpaceComplexity,
];

const CODE_FIELDS: [EditorFieldId; 4] = [
    EditorFieldId::KotlinRuntimeMs,
    EditorFieldId::KotlinCode,
    EditorFieldId::RustRuntimeMs,
    EditorFieldId::RustCode,
];

const WORKFLOW_FIELDS: [EditorFieldId; 18] = [
    EditorFieldId::ProblemTitle,
    EditorFieldId::ProblemUrl,
    EditorFieldId::Difficulty,
    EditorFieldId::ProblemTldr,
    EditorFieldId::Intuition,
    EditorFieldId::Approach,
    EditorFieldId::TimeComplexity,
    EditorFieldId::SpaceComplexity,
    EditorFieldId::KotlinRuntimeMs,
    EditorFieldId::KotlinCode,
    EditorFieldId::RustRuntimeMs,
    EditorFieldId::RustCode,
    EditorFieldId::BlogPostUrl,
    EditorFieldId::SubstackUrl,
    EditorFieldId::YoutubeUrl,
    EditorFieldId::ReferenceUrl,
    EditorFieldId::TelegramText,
    EditorFieldId::Date,
];

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
    let saved_draft = remember(load_initial_draft).with(|draft| draft.clone());
    let fields = remember({
        let saved_draft = saved_draft.clone();
        move || EditorFields::from_draft(&saved_draft)
    })
    .with(|fields| fields.clone());
    let ui_preferences = useState(load_ui_preferences);
    let layout_preferences = remember({
        let initial_preferences = ui_preferences.value();
        move || initial_preferences
    })
    .with(|preferences| preferences.clone());
    let autosave_destination = remember(autosave_destination_label).with(|label| label.clone());
    let preview_state = useState(PreviewState::placeholder);
    let preview_loading = useState(|| false);
    let compose_preview_state = useState(PreviewState::placeholder);
    let compose_loading = useState(|| false);
    let compose_error = useState(String::new);
    let telegram_post_link = useState(String::new);
    let status = useState(startup_status_message);
    let pending_action = useState(|| None::<PendingAction>);
    let action_request_counter = useState(|| 0u64);
    let busy_action = useState(|| None::<LongAction>);
    let current_draft = PostDraft::from_fields(&fields);
    let markdown_preview = current_draft.blog_template();
    let queued_action = pending_action.value();
    let theme = ui_preferences.value().theme;

    cranpose_core::LaunchedEffect!(current_draft.clone(), {
        let draft = current_draft.clone();
        let status = status.clone();
        move |_scope| {
            if let Err(error) = persist_autosave(&draft) {
                status.set(format!("Autosave failed: {error}"));
            }
        }
    });

    cranpose_core::LaunchedEffect!(queued_action.clone(), {
        let preview_state = preview_state.clone();
        let compose_preview_state = compose_preview_state.clone();
        let compose_error = compose_error.clone();
        let busy_action = busy_action.clone();
        let pending_action = pending_action.clone();
        let status = status.clone();
        let telegram_post_link = telegram_post_link.clone();
        move |scope| {
            let Some(action) = queued_action.clone() else {
                return;
            };

            scope.launch_background(
                move |_| async move { run_long_action(action) },
                move |result| {
                    finish_long_action(
                        result,
                        preview_state.clone(),
                        compose_preview_state.clone(),
                        compose_error.clone(),
                        busy_action.clone(),
                        pending_action.clone(),
                        status.clone(),
                        telegram_post_link.clone(),
                    );
                },
            );
        }
    });

    Column(
        Modifier::empty()
            .fill_max_size()
            .background(ui_surface(theme))
            .padding(28.0),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(22.0)),
        {
            let scroll_state = scroll_state.clone();
            let fields = fields.clone();
            let status = status.clone();
            let preview_state = preview_state.clone();
            let preview_loading = preview_loading.clone();
            let compose_preview_state = compose_preview_state.clone();
            let compose_loading = compose_loading.clone();
            let compose_error = compose_error.clone();
            let telegram_post_link = telegram_post_link.clone();
            let markdown_preview = markdown_preview.clone();
            let autosave_destination = autosave_destination.clone();
            let saved_draft = saved_draft.clone();
            let ui_preferences = ui_preferences.clone();
            let layout_preferences = layout_preferences.clone();
            let pending_action = pending_action.clone();
            let action_request_counter = action_request_counter.clone();
            let busy_action = busy_action.clone();
            move || {
                ActionsCard(
                    fields.clone(),
                    status.clone(),
                    preview_state.clone(),
                    autosave_destination.clone(),
                    telegram_post_link.clone(),
                    ui_preferences.clone(),
                    layout_preferences.clone(),
                    pending_action.clone(),
                    action_request_counter.clone(),
                    busy_action.clone(),
                    theme,
                );
                Column(
                    Modifier::empty()
                        .fill_max_width()
                        .weight(1.0)
                        .vertical_scroll(scroll_state.clone(), false),
                    ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(22.0)),
                    {
                        let fields = fields.clone();
                        let preview_state = preview_state.clone();
                        let preview_loading = preview_loading.clone();
                        let compose_preview_state = compose_preview_state.clone();
                        let compose_loading = compose_loading.clone();
                        let compose_error = compose_error.clone();
                        let markdown_preview = markdown_preview.clone();
                        let status = status.clone();
                        let saved_draft = saved_draft.clone();
                        let ui_preferences = ui_preferences.clone();
                        let layout_preferences = layout_preferences.clone();
                        move || {
                            GuidedWorkspace(
                                fields.clone(),
                                preview_state.clone(),
                                preview_loading.clone(),
                                compose_preview_state.clone(),
                                compose_loading.clone(),
                                compose_error.clone(),
                                markdown_preview.clone(),
                                status.clone(),
                                saved_draft.clone(),
                                ui_preferences.clone(),
                                layout_preferences.clone(),
                                theme,
                            );
                        }
                    },
                );
            }
        },
    );
}

#[composable]
fn GuidedWorkspace(
    fields: EditorFields,
    preview_state: MutableState<PreviewState>,
    preview_loading: MutableState<bool>,
    compose_preview_state: MutableState<PreviewState>,
    compose_loading: MutableState<bool>,
    compose_error: MutableState<String>,
    markdown_preview: String,
    status: MutableState<String>,
    saved_draft: PostDraft,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    theme: ThemeMode,
) {
    ProblemMetaCard(
        fields.clone(),
        status.clone(),
        saved_draft.clone(),
        ui_preferences.clone(),
        layout_preferences.clone(),
        theme,
    );
    WriteupCard(
        fields.clone(),
        status.clone(),
        saved_draft.clone(),
        ui_preferences.clone(),
        layout_preferences.clone(),
        theme,
    );
    CodeCard(
        fields,
        status,
        saved_draft,
        ui_preferences,
        layout_preferences,
        theme,
    );
    PreviewCard(preview_state, preview_loading, theme);
    ComposePreviewCard(compose_preview_state, compose_loading, compose_error, theme);
    MarkdownCard(markdown_preview, theme);
}

#[composable]
fn ActionsCard(
    fields: EditorFields,
    status: MutableState<String>,
    preview_state: MutableState<PreviewState>,
    autosave_destination: String,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    theme: ThemeMode,
) {
    section_card(theme, {
        let fields = fields.clone();
        let status = status.clone();
        let preview_state = preview_state.clone();
        let telegram_post_link = telegram_post_link.clone();
        let ui_preferences = ui_preferences.clone();
        let layout_preferences = layout_preferences.clone();
        let pending_action = pending_action.clone();
        let action_request_counter = action_request_counter.clone();
        let busy_action = busy_action.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let preview_state = preview_state.clone();
                    let telegram_post_link = telegram_post_link.clone();
                    let autosave_destination = autosave_destination.clone();
                    let ui_preferences = ui_preferences.clone();
                    let layout_preferences = layout_preferences.clone();
                    let pending_action = pending_action.clone();
                    let action_request_counter = action_request_counter.clone();
                    let busy_action = busy_action.clone();
                    move || {
                        Row(
                            Modifier::empty().fill_max_width(),
                            RowSpec::default()
                                .horizontal_arrangement(LinearArrangement::SpaceBetween),
                            {
                                let ui_preferences = ui_preferences.clone();
                                let status = status.clone();
                                move || {
                                    Text(
                                        "LeetCode Daily Composer",
                                        Modifier::empty(),
                                        heading_style(34.0, theme),
                                    );
                                    let next_theme = theme.toggled();
                                    subtle_button(
                                        format!("Theme: {}", next_theme.label()),
                                        "theme.toggle".to_string(),
                                        ui_preferences.clone(),
                                        theme,
                                        move || {
                                            set_theme_preference(
                                                ui_preferences.clone(),
                                                next_theme,
                                                status.clone(),
                                            );
                                        },
                                    );
                                }
                            },
                        );
                        Text(
                            autosave_destination.clone(),
                            Modifier::empty(),
                            muted_style(theme),
                        );

                        let draft = PostDraft::from_fields(&fields);
                        let preview = preview_state.value();
                        let latest_telegram_link = telegram_post_link.value();
                        let next_item = recommended_next_work(
                            &draft,
                            &preview,
                            &latest_telegram_link,
                            &layout_preferences,
                        );
                        Row(
                            Modifier::empty().fill_max_width(),
                            RowSpec::default()
                                .horizontal_arrangement(LinearArrangement::spaced_by(14.0)),
                            {
                                let fields = fields.clone();
                                let status = status.clone();
                                let telegram_post_link = telegram_post_link.clone();
                                let ui_preferences = ui_preferences.clone();
                                let layout_preferences = layout_preferences.clone();
                                let pending_action = pending_action.clone();
                                let action_request_counter = action_request_counter.clone();
                                let busy_action = busy_action.clone();
                                move || {
                                    NextWorkPanel(
                                        next_item,
                                        fields.clone(),
                                        status.clone(),
                                        telegram_post_link.clone(),
                                        ui_preferences.clone(),
                                        pending_action.clone(),
                                        action_request_counter.clone(),
                                        busy_action.clone(),
                                        theme,
                                    );
                                    Column(
                                        Modifier::empty().weight(3.0),
                                        ColumnSpec::default().vertical_arrangement(
                                            LinearArrangement::spaced_by(10.0),
                                        ),
                                        {
                                            let fields = fields.clone();
                                            let status = status.clone();
                                            let telegram_post_link = telegram_post_link.clone();
                                            let ui_preferences = ui_preferences.clone();
                                            let layout_preferences = layout_preferences.clone();
                                            let pending_action = pending_action.clone();
                                            let action_request_counter =
                                                action_request_counter.clone();
                                            let busy_action = busy_action.clone();
                                            move || {
                                                ActionButtons(
                                                    fields.clone(),
                                                    status.clone(),
                                                    telegram_post_link.clone(),
                                                    ui_preferences.clone(),
                                                    layout_preferences.clone(),
                                                    pending_action.clone(),
                                                    action_request_counter.clone(),
                                                    busy_action.clone(),
                                                    theme,
                                                );
                                            }
                                        },
                                    );
                                }
                            },
                        );

                        WorkflowRail(
                            draft.clone(),
                            preview.last_saved_webp_path.is_some(),
                            latest_telegram_link.clone(),
                            next_item,
                            theme,
                        );
                        WorkQueue(
                            work_queue(
                                &draft,
                                &preview,
                                &latest_telegram_link,
                                &layout_preferences,
                            ),
                            theme,
                        );

                        Text(status.clone(), Modifier::empty(), accent_style(theme));

                        if let Some(saved_webp) = preview_state.value().last_saved_webp_path {
                            Text(
                                format!("Latest WebP: {saved_webp}"),
                                Modifier::empty(),
                                body_style(theme),
                            );
                        }
                        if !latest_telegram_link.is_empty() {
                            Text(
                                format!("Latest Telegram post: {latest_telegram_link}"),
                                Modifier::empty(),
                                body_style(theme),
                            );
                        }
                    }
                },
            );
        }
    });
}

#[composable]
fn NextWorkPanel(
    next_item: NextWorkItem,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    theme: ThemeMode,
) {
    ComposeBox(
        Modifier::empty()
            .weight(1.0)
            .background(next_panel_surface(theme))
            .rounded_corners(8.0)
            .padding(18.0),
        BoxSpec::default(),
        {
            let fields = fields.clone();
            let status = status.clone();
            let telegram_post_link = telegram_post_link.clone();
            let ui_preferences = ui_preferences.clone();
            let pending_action = pending_action.clone();
            let action_request_counter = action_request_counter.clone();
            let busy_action = busy_action.clone();
            move || {
                Column(
                    Modifier::empty().fill_max_width(),
                    ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(12.0)),
                    {
                        let fields = fields.clone();
                        let status = status.clone();
                        let telegram_post_link = telegram_post_link.clone();
                        let ui_preferences = ui_preferences.clone();
                        let pending_action = pending_action.clone();
                        let action_request_counter = action_request_counter.clone();
                        let busy_action = busy_action.clone();
                        move || {
                            Row(
                                Modifier::empty().fill_max_width(),
                                RowSpec::default()
                                    .horizontal_arrangement(LinearArrangement::SpaceBetween),
                                {
                                    move || {
                                        Text("Now", Modifier::empty(), eyebrow_style(theme));
                                        Text(
                                            next_item.stage().label(),
                                            Modifier::empty(),
                                            stage_label_style(theme),
                                        );
                                    }
                                },
                            );
                            Text(
                                next_item.title(),
                                Modifier::empty(),
                                heading_style(26.0, theme),
                            );
                            match next_item {
                                NextWorkItem::Field(field) => {
                                    ComposeBox(
                                        Modifier::empty()
                                            .fill_max_width()
                                            .background(stage_surface(theme, false))
                                            .rounded_corners(8.0)
                                            .padding_symmetric(12.0, 10.0),
                                        BoxSpec::default(),
                                        move || {
                                            Text(
                                                field.label(),
                                                Modifier::empty(),
                                                queue_text_style(theme),
                                            );
                                        },
                                    );
                                }
                                NextWorkItem::Action(action) => {
                                    focus_action_button(
                                        action,
                                        fields.clone(),
                                        status.clone(),
                                        telegram_post_link.clone(),
                                        ui_preferences.clone(),
                                        pending_action.clone(),
                                        action_request_counter.clone(),
                                        busy_action.clone(),
                                        theme,
                                    );
                                }
                            }
                        }
                    },
                );
            }
        },
    );
}

#[composable]
fn WorkflowRail(
    draft: PostDraft,
    preview_saved: bool,
    telegram_link: String,
    next_item: NextWorkItem,
    theme: ThemeMode,
) {
    let stages = [
        WorkStage::Prepare,
        WorkStage::Write,
        WorkStage::Code,
        WorkStage::Review,
        WorkStage::Ship,
    ];
    Row(
        Modifier::empty().fill_max_width(),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(10.0)),
        move || {
            for stage in stages {
                workflow_stage_chip(
                    stage,
                    stage_status(stage, &draft, preview_saved, &telegram_link),
                    stage == next_item.stage(),
                    theme,
                );
            }
        },
    );
}

#[composable]
fn workflow_stage_chip(stage: WorkStage, status: &'static str, active: bool, theme: ThemeMode) {
    ComposeBox(
        Modifier::empty()
            .weight(1.0)
            .background(stage_surface(theme, active))
            .rounded_corners(8.0)
            .padding_symmetric(12.0, 10.0),
        BoxSpec::default(),
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(4.0)),
                move || {
                    Text(stage.label(), Modifier::empty(), label_style(theme, active));
                    Text(status.to_string(), Modifier::empty(), muted_style(theme));
                },
            );
        },
    );
}

#[composable]
fn WorkQueue(queue: Vec<NextWorkItem>, theme: ThemeMode) {
    Row(
        Modifier::empty().fill_max_width(),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            for (index, item) in queue.iter().take(6).enumerate() {
                queue_chip(index + 1, *item, theme);
            }
        },
    );
}

#[composable]
fn queue_chip(index: usize, item: NextWorkItem, theme: ThemeMode) {
    ComposeBox(
        Modifier::empty()
            .background(panel_surface(theme))
            .rounded_corners(8.0)
            .padding_symmetric(12.0, 8.0),
        BoxSpec::default(),
        move || {
            Text(
                format!("{index}. {}", item.short_label()),
                Modifier::empty(),
                queue_text_style(theme),
            );
        },
    );
}

#[composable]
fn ActionButtons(
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    theme: ThemeMode,
) {
    let ordered_actions = ordered_action_buttons(&layout_preferences);
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(12.0)),
        {
            let fields = fields.clone();
            let status = status.clone();
            let telegram_post_link = telegram_post_link.clone();
            let ui_preferences = ui_preferences.clone();
            let pending_action = pending_action.clone();
            let action_request_counter = action_request_counter.clone();
            let busy_action = busy_action.clone();
            move || {
                for row in ordered_actions.chunks(5) {
                    let row_actions = row.to_vec();
                    let fields = fields.clone();
                    let status = status.clone();
                    let telegram_post_link = telegram_post_link.clone();
                    let ui_preferences = ui_preferences.clone();
                    let pending_action = pending_action.clone();
                    let action_request_counter = action_request_counter.clone();
                    let busy_action = busy_action.clone();
                    Row(
                        Modifier::empty().fill_max_width(),
                        RowSpec::default()
                            .horizontal_arrangement(LinearArrangement::spaced_by(12.0)),
                        move || {
                            let fields = fields.clone();
                            let status = status.clone();
                            let telegram_post_link = telegram_post_link.clone();
                            let ui_preferences = ui_preferences.clone();
                            let pending_action = pending_action.clone();
                            let action_request_counter = action_request_counter.clone();
                            let busy_action = busy_action.clone();
                            ForEach(&row_actions, move |action| {
                                ActionButton(
                                    *action,
                                    fields.clone(),
                                    status.clone(),
                                    telegram_post_link.clone(),
                                    ui_preferences.clone(),
                                    pending_action.clone(),
                                    action_request_counter.clone(),
                                    busy_action.clone(),
                                    theme,
                                );
                            });
                        },
                    );
                }
            }
        },
    );
}

#[composable]
fn ActionButton(
    action: ActionButtonId,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    theme: ThemeMode,
) {
    let action_busy = busy_action.value();
    let long_action = action.long_action();
    let is_busy = long_action.is_some() && action_busy == long_action;
    let disabled = long_action.is_some() && action_busy.is_some();
    primary_button(
        action.label(),
        action.count_key(),
        ui_preferences.clone(),
        theme,
        disabled,
        is_busy,
        move || {
            handle_action_button(
                action,
                fields.clone(),
                status.clone(),
                telegram_post_link.clone(),
                pending_action.clone(),
                action_request_counter.clone(),
                busy_action.clone(),
            );
        },
    );
}

#[composable]
fn focus_action_button(
    action: ActionButtonId,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    theme: ThemeMode,
) {
    let action_busy = busy_action.value();
    let long_action = action.long_action();
    let is_busy = long_action.is_some() && action_busy == long_action;
    let disabled = long_action.is_some() && action_busy.is_some();
    let count_key = action.count_key().to_string();
    let count = ui_preferences.value().button_count(&count_key);
    let busy_pulse = if is_busy { busy_pulse() } else { 0.0 };
    let background = if is_busy {
        button_surface(theme).with_alpha(0.72 + 0.24 * busy_pulse)
    } else if disabled {
        disabled_button_surface(theme)
    } else {
        button_surface(theme)
    };
    let style = if disabled {
        disabled_button_text_style(theme)
    } else {
        focus_button_text_style(theme, busy_pulse)
    };
    Button(
        Modifier::empty()
            .fill_max_width()
            .background(background)
            .rounded_corners(8.0)
            .padding_symmetric(24.0, 18.0),
        move || {
            if disabled {
                return;
            }
            record_button_press(ui_preferences.clone(), &count_key);
            handle_action_button(
                action,
                fields.clone(),
                status.clone(),
                telegram_post_link.clone(),
                pending_action.clone(),
                action_request_counter.clone(),
                busy_action.clone(),
            );
        },
        move || {
            button_content(
                action.label().to_string(),
                count,
                style.clone(),
                theme,
                is_busy,
            );
        },
    );
}

fn handle_action_button(
    action: ActionButtonId,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
) {
    let draft = PostDraft::from_fields(&fields);
    if let Some(long_action) = action.long_action() {
        enqueue_long_action(
            long_action,
            draft,
            telegram_post_link.value(),
            pending_action,
            action_request_counter,
            busy_action,
            status,
        );
        return;
    }

    match action {
        ActionButtonId::CopyLeetcode => copy_text_to_clipboard(
            draft.leetcode_template(),
            "LeetCode template copied.".to_string(),
            status,
        ),
        ActionButtonId::CopyYoutube => copy_text_to_clipboard(
            draft.youtube_template(),
            "YouTube template copied.".to_string(),
            status,
        ),
        ActionButtonId::CopyBlog => copy_text_to_clipboard(
            draft.blog_template(),
            "Blog template copied.".to_string(),
            status,
        ),
        ActionButtonId::CopyTelegram => copy_text_to_clipboard(
            draft.telegram_template(),
            "Telegram template copied.".to_string(),
            status,
        ),
        ActionButtonId::CopyTitle => {
            copy_text_to_clipboard(draft.title_text(), "Title copied.".to_string(), status)
        }
        ActionButtonId::CopySubtitle => copy_text_to_clipboard(
            draft.subtitle_text(),
            "Subtitle copied.".to_string(),
            status,
        ),
        ActionButtonId::CopyRichText => copy_rich_text_to_clipboard(draft, status),
        ActionButtonId::RefreshRasterPreview
        | ActionButtonId::RefreshCranposePreview
        | ActionButtonId::SaveRasterWebp
        | ActionButtonId::SaveCranposeWebp
        | ActionButtonId::PublishBlog
        | ActionButtonId::PostTelegram
        | ActionButtonId::PostTelegramComment => {}
    }
}

fn enqueue_long_action(
    action: LongAction,
    draft: PostDraft,
    telegram_post_link: String,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    status: MutableState<String>,
) {
    if busy_action.value().is_some() {
        return;
    }

    let request_id = action_request_counter.update(|value| {
        *value = value.wrapping_add(1);
        *value
    });
    busy_action.set(Some(action));
    pending_action.set(Some(PendingAction {
        action,
        request_id,
        draft,
        telegram_post_link,
    }));
    status.set(format!("{} started...", action.label()));
}

impl ActionButtonId {
    fn label(self) -> &'static str {
        match self {
            Self::RefreshRasterPreview => "Refresh Raster",
            Self::RefreshCranposePreview => "Refresh Cranpose",
            Self::CopyLeetcode => "Copy LeetCode",
            Self::CopyYoutube => "Copy YouTube",
            Self::CopyBlog => "Copy Blog",
            Self::CopyTelegram => "Copy Telegram",
            Self::CopyTitle => "Copy Title",
            Self::CopySubtitle => "Copy Subtitle",
            Self::CopyRichText => "Copy Rich Text",
            Self::SaveRasterWebp => "Save Raster WebP",
            Self::SaveCranposeWebp => "Save Cranpose WebP",
            Self::PublishBlog => "Publish Blog",
            Self::PostTelegram => "Post Telegram",
            Self::PostTelegramComment => "Post TG Comment",
        }
    }

    fn count_key(self) -> &'static str {
        match self {
            Self::RefreshRasterPreview => "preview.raster",
            Self::RefreshCranposePreview => "preview.cranpose",
            Self::CopyLeetcode => "copy.leetcode",
            Self::CopyYoutube => "copy.youtube",
            Self::CopyBlog => "copy.blog",
            Self::CopyTelegram => "copy.telegram",
            Self::CopyTitle => "copy.title",
            Self::CopySubtitle => "copy.subtitle",
            Self::CopyRichText => "copy.rich_text",
            Self::SaveRasterWebp => "save.raster_webp",
            Self::SaveCranposeWebp => "save.cranpose_webp",
            Self::PublishBlog => "publish.blog",
            Self::PostTelegram => "post.telegram",
            Self::PostTelegramComment => "post.telegram_comment",
        }
    }

    fn long_action(self) -> Option<LongAction> {
        match self {
            Self::RefreshRasterPreview => Some(LongAction::RefreshRasterPreview),
            Self::RefreshCranposePreview => Some(LongAction::RefreshCranposePreview),
            Self::SaveRasterWebp => Some(LongAction::SaveRasterWebp),
            Self::SaveCranposeWebp => Some(LongAction::SaveCranposeWebp),
            Self::PublishBlog => Some(LongAction::PublishBlog),
            Self::PostTelegram => Some(LongAction::PostTelegram),
            Self::PostTelegramComment => Some(LongAction::PostTelegramComment),
            _ => None,
        }
    }
}

impl LongAction {
    fn label(self) -> &'static str {
        match self {
            Self::RefreshRasterPreview => "Refresh Raster",
            Self::RefreshCranposePreview => "Refresh Cranpose",
            Self::SaveRasterWebp => "Save Raster WebP",
            Self::SaveCranposeWebp => "Save Cranpose WebP",
            Self::PublishBlog => "Publish Blog",
            Self::PostTelegram => "Post Telegram",
            Self::PostTelegramComment => "Post TG Comment",
        }
    }
}

fn ordered_action_buttons(preferences: &UiPreferences) -> Vec<ActionButtonId> {
    let mut actions = ACTION_BUTTONS.to_vec();
    actions.sort_by_key(|action| {
        component_sort_key(
            preferences,
            action.count_key(),
            ACTION_BUTTONS
                .iter()
                .position(|candidate| candidate == action)
                .unwrap_or(usize::MAX),
        )
    });
    actions
}

fn component_sort_key(
    preferences: &UiPreferences,
    component_key: &str,
    default_index: usize,
) -> (u8, u64, usize) {
    let usage_order = preferences.component_order(component_key);
    if usage_order == 0 {
        (1, 0, default_index)
    } else {
        (0, usage_order, default_index)
    }
}

fn recommended_next_work(
    draft: &PostDraft,
    preview: &PreviewState,
    telegram_link: &str,
    preferences: &UiPreferences,
) -> NextWorkItem {
    work_queue(draft, preview, telegram_link, preferences)
        .into_iter()
        .next()
        .unwrap_or(NextWorkItem::Action(ActionButtonId::CopyBlog))
}

fn work_queue(
    draft: &PostDraft,
    preview: &PreviewState,
    telegram_link: &str,
    preferences: &UiPreferences,
) -> Vec<NextWorkItem> {
    let mut queue = Vec::new();
    for field in ordered_workflow_fields(preferences) {
        if field_needs_attention(field, draft) {
            queue.push(NextWorkItem::Field(field));
        }
    }

    if preview.last_saved_webp_path.is_none() {
        queue.push(NextWorkItem::Action(ActionButtonId::SaveRasterWebp));
    }
    if draft.blog_post_url.trim().is_empty() {
        queue.push(NextWorkItem::Action(ActionButtonId::CopyBlog));
    } else {
        queue.push(NextWorkItem::Action(ActionButtonId::PublishBlog));
    }
    if telegram_link.trim().is_empty() {
        queue.push(NextWorkItem::Action(ActionButtonId::PostTelegram));
    } else {
        queue.push(NextWorkItem::Action(ActionButtonId::PostTelegramComment));
    }

    for action in ordered_action_buttons(preferences) {
        let item = NextWorkItem::Action(action);
        if !queue.contains(&item) {
            queue.push(item);
        }
    }

    queue
}

fn ordered_workflow_fields(preferences: &UiPreferences) -> Vec<EditorFieldId> {
    let mut fields = WORKFLOW_FIELDS.to_vec();
    fields.sort_by_key(|field| {
        (
            field_stage(*field).sort_index(),
            component_sort_key(
                preferences,
                &field.component_key(),
                WORKFLOW_FIELDS
                    .iter()
                    .position(|candidate| candidate == field)
                    .unwrap_or(usize::MAX),
            ),
        )
    });
    fields
}

fn field_needs_attention(field: EditorFieldId, draft: &PostDraft) -> bool {
    match field {
        EditorFieldId::ProblemTitle => draft.problem_title.trim().is_empty(),
        EditorFieldId::ProblemUrl => draft.problem_url.trim().is_empty(),
        EditorFieldId::Difficulty => draft.difficulty.trim().is_empty(),
        EditorFieldId::ProblemTldr => draft.problem_tldr.trim().is_empty(),
        EditorFieldId::Intuition => draft.intuition.trim().is_empty(),
        EditorFieldId::Approach => draft.approach.trim().is_empty(),
        EditorFieldId::TimeComplexity => draft.time_complexity.trim().is_empty(),
        EditorFieldId::SpaceComplexity => draft.space_complexity.trim().is_empty(),
        EditorFieldId::KotlinRuntimeMs => draft.kotlin_runtime_ms.trim().is_empty(),
        EditorFieldId::KotlinCode => draft.kotlin_code.trim().is_empty(),
        EditorFieldId::RustRuntimeMs => draft.rust_runtime_ms.trim().is_empty(),
        EditorFieldId::RustCode => draft.rust_code.trim().is_empty(),
        EditorFieldId::Date
        | EditorFieldId::BlogPostUrl
        | EditorFieldId::SubstackUrl
        | EditorFieldId::YoutubeUrl
        | EditorFieldId::ReferenceUrl
        | EditorFieldId::TelegramText => false,
    }
}

fn field_stage(field: EditorFieldId) -> WorkStage {
    match field {
        EditorFieldId::Date
        | EditorFieldId::ProblemTitle
        | EditorFieldId::ProblemUrl
        | EditorFieldId::Difficulty => WorkStage::Prepare,
        EditorFieldId::ProblemTldr
        | EditorFieldId::Intuition
        | EditorFieldId::Approach
        | EditorFieldId::TimeComplexity
        | EditorFieldId::SpaceComplexity => WorkStage::Write,
        EditorFieldId::KotlinRuntimeMs
        | EditorFieldId::KotlinCode
        | EditorFieldId::RustRuntimeMs
        | EditorFieldId::RustCode => WorkStage::Code,
        EditorFieldId::BlogPostUrl
        | EditorFieldId::SubstackUrl
        | EditorFieldId::YoutubeUrl
        | EditorFieldId::ReferenceUrl
        | EditorFieldId::TelegramText => WorkStage::Ship,
    }
}

fn action_stage(action: ActionButtonId) -> WorkStage {
    match action {
        ActionButtonId::RefreshRasterPreview
        | ActionButtonId::RefreshCranposePreview
        | ActionButtonId::SaveRasterWebp
        | ActionButtonId::SaveCranposeWebp => WorkStage::Review,
        ActionButtonId::PublishBlog
        | ActionButtonId::PostTelegram
        | ActionButtonId::PostTelegramComment => WorkStage::Ship,
        ActionButtonId::CopyLeetcode
        | ActionButtonId::CopyYoutube
        | ActionButtonId::CopyBlog
        | ActionButtonId::CopyTelegram
        | ActionButtonId::CopyTitle
        | ActionButtonId::CopySubtitle
        | ActionButtonId::CopyRichText => WorkStage::Ship,
    }
}

fn stage_status(
    stage: WorkStage,
    draft: &PostDraft,
    preview_saved: bool,
    telegram_link: &str,
) -> &'static str {
    match stage {
        WorkStage::Prepare => {
            if [EditorFieldId::ProblemTitle, EditorFieldId::ProblemUrl]
                .into_iter()
                .any(|field| field_needs_attention(field, draft))
            {
                "Needs basics"
            } else {
                "Ready"
            }
        }
        WorkStage::Write => {
            if [
                EditorFieldId::ProblemTldr,
                EditorFieldId::Intuition,
                EditorFieldId::Approach,
            ]
            .into_iter()
            .any(|field| field_needs_attention(field, draft))
            {
                "Drafting"
            } else {
                "Ready"
            }
        }
        WorkStage::Code => {
            if [
                EditorFieldId::KotlinCode,
                EditorFieldId::RustCode,
                EditorFieldId::KotlinRuntimeMs,
                EditorFieldId::RustRuntimeMs,
            ]
            .into_iter()
            .any(|field| field_needs_attention(field, draft))
            {
                "Needs code"
            } else {
                "Ready"
            }
        }
        WorkStage::Review => {
            if preview_saved {
                "Saved"
            } else {
                "Needs image"
            }
        }
        WorkStage::Ship => {
            if telegram_link.trim().is_empty() {
                "Pending"
            } else {
                "Posted"
            }
        }
    }
}

impl WorkStage {
    fn label(self) -> &'static str {
        match self {
            Self::Prepare => "Prepare",
            Self::Write => "Write",
            Self::Code => "Code",
            Self::Review => "Review",
            Self::Ship => "Ship",
        }
    }

    fn sort_index(self) -> u8 {
        match self {
            Self::Prepare => 0,
            Self::Write => 1,
            Self::Code => 2,
            Self::Review => 3,
            Self::Ship => 4,
        }
    }
}

impl NextWorkItem {
    fn stage(self) -> WorkStage {
        match self {
            Self::Field(field) => field_stage(field),
            Self::Action(action) => action_stage(action),
        }
    }

    fn title(self) -> String {
        match self {
            Self::Field(field) => format!("Fill {}", field.label()),
            Self::Action(action) => action.label().to_string(),
        }
    }

    fn short_label(self) -> String {
        match self {
            Self::Field(field) => field.label().to_string(),
            Self::Action(action) => action.label().to_string(),
        }
    }
}

#[composable]
fn PreviewCard(
    preview_state: MutableState<PreviewState>,
    preview_loading: MutableState<bool>,
    theme: ThemeMode,
) {
    section_card(theme, {
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
                        Text(
                            "Card Preview",
                            Modifier::empty(),
                            heading_style(28.0, theme),
                        );
                        if preview_loading.value() {
                            Text(
                                "Rendering preview in the background...",
                                Modifier::empty(),
                                accent_style(theme),
                            );
                        }
                        ComposeBox(
                            Modifier::empty()
                                .size(Size {
                                    width: 1200.0,
                                    height: 675.0,
                                })
                                .background(panel_surface(theme))
                                .rounded_corners(8.0)
                                .padding(18.0),
                            BoxSpec::default().content_alignment(Alignment::CENTER),
                            move || {
                                Image(
                                    BitmapPainter(preview.bitmap.clone()),
                                    Some("Generated preview".to_string()),
                                    Modifier::empty().fill_max_size().rounded_corners(8.0),
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
    compose_preview_state: MutableState<PreviewState>,
    compose_loading: MutableState<bool>,
    compose_error: MutableState<String>,
    theme: ThemeMode,
) {
    section_card(theme, {
        let compose_preview_state = compose_preview_state.clone();
        let compose_loading = compose_loading.clone();
        let compose_error = compose_error.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let compose_preview_state = compose_preview_state.clone();
                    let compose_loading = compose_loading.clone();
                    let compose_error = compose_error.clone();
                    move || {
                        let preview = compose_preview_state.value();
                        let error = compose_error.value();
                        Text(
                            "Cranpose Preview",
                            Modifier::empty(),
                            heading_style(28.0, theme),
                        );
                        if compose_loading.value() {
                            Text(
                                "Preparing Cranpose preview in the background...",
                                Modifier::empty(),
                                accent_style(theme),
                            );
                        } else if !error.is_empty() {
                            Text(error.clone(), Modifier::empty(), body_style(theme));
                        }
                        ComposeBox(
                            Modifier::empty()
                                .size(Size {
                                    width: 1200.0,
                                    height: 675.0,
                                })
                                .background(panel_surface(theme))
                                .rounded_corners(8.0)
                                .padding(18.0),
                            BoxSpec::default().content_alignment(Alignment::CENTER),
                            move || {
                                if !compose_loading.value() && !error.is_empty() {
                                    Text(
                                        error.clone(),
                                        Modifier::empty().fill_max_width(),
                                        body_style(theme),
                                    );
                                } else {
                                    Image(
                                        BitmapPainter(preview.bitmap.clone()),
                                        Some("Cranpose preview".to_string()),
                                        Modifier::empty().fill_max_size().rounded_corners(8.0),
                                        Alignment::CENTER,
                                        ContentScale::Fit,
                                        DEFAULT_ALPHA,
                                        None,
                                    );
                                }
                            },
                        );
                    }
                },
            );
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[composable]
fn CranposeCaptureSurface(
    compose_assets: ComposePreviewAssets,
    compose_plan: CardRenderPlan,
    scale: f32,
) {
    ComposeBox(Modifier::empty().fill_max_size(), BoxSpec::default(), {
        let compose_assets = compose_assets.clone();
        let compose_plan = compose_plan.clone();
        move || {
            let background = compose_assets.background.clone();
            let qr = compose_assets.qr.clone();
            let compose_plan = compose_plan.clone();
            Image(
                BitmapPainter(background),
                Some("Cranpose card background".to_string()),
                Modifier::empty().fill_max_size(),
                Alignment::CENTER,
                ContentScale::Crop,
                DEFAULT_ALPHA,
                None,
            );

            Image(
                BitmapPainter(qr),
                Some("QR overlay".to_string()),
                Modifier::empty()
                    .absolute_offset(
                        scale_x(compose_plan.qr.x, scale),
                        scale_y(compose_plan.qr.y, scale),
                    )
                    .size(scaled_size(
                        compose_plan.qr.width,
                        compose_plan.qr.height,
                        scale,
                    ))
                    .rounded_corners(18.0 * scale),
                Alignment::CENTER,
                ContentScale::Fit,
                DEFAULT_ALPHA * 0.72,
                None,
            );

            ComposeBox(
                Modifier::empty()
                    .absolute_offset(
                        scale_x(compose_plan.panel.x, scale),
                        scale_y(compose_plan.panel.y, scale),
                    )
                    .size(scaled_size(
                        compose_plan.panel.width,
                        compose_plan.panel.height,
                        scale,
                    ))
                    .background(Color::from_rgba_u8(5, 8, 14, 210))
                    .rounded_corners(46.0 * scale)
                    .padding(compose_plan.panel_padding as f32 * scale),
                BoxSpec::default(),
                {
                    let compose_plan = compose_plan.clone();
                    move || {
                        CranposePanelContent(compose_plan.clone(), scale);
                    }
                },
            );
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[composable]
fn CranposePanelContent(compose_plan: CardRenderPlan, scale: f32) {
    Column(Modifier::empty().fill_max_size(), ColumnSpec::default(), {
        let compose_plan = compose_plan.clone();
        move || {
            Spacer(Size::new(
                0.0,
                compose_plan.code_group_top_offset as f32 * scale,
            ));
            ComposeBox(
                Modifier::empty().fill_max_width(),
                BoxSpec::default().content_alignment(Alignment::CENTER),
                {
                    let compose_plan = compose_plan.clone();
                    move || {
                        Column(
                            Modifier::empty().width(compose_plan.shared_text_width as f32 * scale),
                            ColumnSpec::default().vertical_arrangement(
                                LinearArrangement::spaced_by(compose_plan.code_gap as f32 * scale),
                            ),
                            {
                                let code_blocks = compose_plan.code_blocks.clone();
                                move || {
                                    for code_block in code_blocks.clone() {
                                        CranposeCodeBlockCard(code_block, scale);
                                    }
                                }
                            },
                        );
                    }
                },
            );
            ComposeBox(
                Modifier::empty().fill_max_width().weight(1.0),
                BoxSpec::default(),
                || {},
            );
            ComposeBox(
                Modifier::empty().fill_max_width(),
                BoxSpec::default().content_alignment(Alignment::CENTER),
                {
                    let compose_plan = compose_plan.clone();
                    move || {
                        ComposeBox(
                            Modifier::empty().width(compose_plan.tldr.width as f32 * scale),
                            BoxSpec::default(),
                            {
                                let tldr = compose_plan.tldr.clone();
                                move || {
                                    CranposeTldrBlock(tldr.clone(), scale);
                                }
                            },
                        );
                    }
                },
            );
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[composable]
fn CranposeCodeBlockCard(code_block: CodeRenderPlan, scale: f32) {
    Column(Modifier::empty().fill_max_width(), ColumnSpec::default(), {
        let code_block = code_block.clone();
        move || {
            Text(
                format!("// {}", code_block.language),
                Modifier::empty(),
                preview_code_label_style(code_block.label_font_size * scale),
            );
            Spacer(Size::new(0.0, 4.0 * scale));
            Text(
                format!("// {}", code_block.runtime),
                Modifier::empty(),
                preview_runtime_style(code_block.label_font_size * scale),
            );
            Spacer(Size::new(0.0, 14.0 * scale));
            let line_gap =
                ((code_block.code_line_height as f32 - code_block.code_font_size).max(0.0)) * scale;
            for (index, line) in code_block.lines.iter().enumerate() {
                Text(
                    line.clone(),
                    Modifier::empty(),
                    preview_code_style(
                        code_block.code_font_size * scale,
                        code_block.code_line_height as f32 * scale,
                    ),
                );
                if index + 1 < code_block.lines.len() && line_gap > 0.0 {
                    Spacer(Size::new(0.0, line_gap));
                }
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[composable]
fn CranposeTldrBlock(tldr: crate::export::TextRenderPlan, scale: f32) {
    Column(Modifier::empty().fill_max_width(), ColumnSpec::default(), {
        let tldr = tldr.clone();
        move || {
            let line_gap = ((tldr.line_height as f32 - tldr.font_size).max(0.0)) * scale;
            for (index, line) in tldr.lines.iter().enumerate() {
                Text(
                    line.clone(),
                    Modifier::empty().fill_max_width(),
                    preview_tldr_style(tldr.font_size * scale, tldr.line_height as f32 * scale),
                );
                if index + 1 < tldr.lines.len() && line_gap > 0.0 {
                    Spacer(Size::new(0.0, line_gap));
                }
            }
        }
    });
}

#[composable]
fn MarkdownCard(markdown_preview: String, theme: ThemeMode) {
    section_card(theme, {
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
                            heading_style(28.0, theme),
                        );
                        ComposeBox(
                            Modifier::empty()
                                .fill_max_width()
                                .background(panel_surface(theme))
                                .rounded_corners(8.0)
                                .padding(18.0),
                            BoxSpec::default(),
                            move || {
                                Text(
                                    markdown_content.clone(),
                                    Modifier::empty().fill_max_width(),
                                    code_text_style(18.0, theme),
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
fn ProblemMetaCard(
    fields: EditorFields,
    status: MutableState<String>,
    saved_draft: PostDraft,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    theme: ThemeMode,
) {
    section_card(theme, {
        let fields = fields.clone();
        let status = status.clone();
        let saved_draft = saved_draft.clone();
        let ui_preferences = ui_preferences.clone();
        let layout_preferences = layout_preferences.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let saved_draft = saved_draft.clone();
                    let ui_preferences = ui_preferences.clone();
                    let ordered_fields = ordered_fields(&META_FIELDS, &layout_preferences);
                    move || {
                        Text(
                            "Problem Meta",
                            Modifier::empty(),
                            heading_style(28.0, theme),
                        );
                        ForEach(&ordered_fields, {
                            let fields = fields.clone();
                            let saved_draft = saved_draft.clone();
                            let status = status.clone();
                            let ui_preferences = ui_preferences.clone();
                            move |field| {
                                EditorField(
                                    *field,
                                    fields.clone(),
                                    saved_draft.clone(),
                                    status.clone(),
                                    ui_preferences.clone(),
                                    theme,
                                );
                            }
                        });
                    }
                },
            );
        }
    });
}

#[composable]
fn WriteupCard(
    fields: EditorFields,
    status: MutableState<String>,
    saved_draft: PostDraft,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    theme: ThemeMode,
) {
    section_card(theme, {
        let fields = fields.clone();
        let status = status.clone();
        let saved_draft = saved_draft.clone();
        let ui_preferences = ui_preferences.clone();
        let layout_preferences = layout_preferences.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let saved_draft = saved_draft.clone();
                    let ui_preferences = ui_preferences.clone();
                    let ordered_fields = ordered_fields(&WRITEUP_FIELDS, &layout_preferences);
                    move || {
                        Text("Writeup", Modifier::empty(), heading_style(28.0, theme));
                        ForEach(&ordered_fields, {
                            let fields = fields.clone();
                            let saved_draft = saved_draft.clone();
                            let status = status.clone();
                            let ui_preferences = ui_preferences.clone();
                            move |field| {
                                EditorField(
                                    *field,
                                    fields.clone(),
                                    saved_draft.clone(),
                                    status.clone(),
                                    ui_preferences.clone(),
                                    theme,
                                );
                            }
                        });
                    }
                },
            );
        }
    });
}

#[composable]
fn CodeCard(
    fields: EditorFields,
    status: MutableState<String>,
    saved_draft: PostDraft,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    theme: ThemeMode,
) {
    section_card(theme, {
        let fields = fields.clone();
        let status = status.clone();
        let saved_draft = saved_draft.clone();
        let ui_preferences = ui_preferences.clone();
        let layout_preferences = layout_preferences.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let saved_draft = saved_draft.clone();
                    let ui_preferences = ui_preferences.clone();
                    let ordered_fields = ordered_fields(&CODE_FIELDS, &layout_preferences);
                    move || {
                        Text("Code Blocks", Modifier::empty(), heading_style(28.0, theme));
                        ForEach(&ordered_fields, {
                            let fields = fields.clone();
                            let saved_draft = saved_draft.clone();
                            let status = status.clone();
                            let ui_preferences = ui_preferences.clone();
                            move |field| {
                                EditorField(
                                    *field,
                                    fields.clone(),
                                    saved_draft.clone(),
                                    status.clone(),
                                    ui_preferences.clone(),
                                    theme,
                                );
                            }
                        });
                    }
                },
            );
        }
    });
}

#[composable]
fn EditorField(
    field: EditorFieldId,
    fields: EditorFields,
    saved_draft: PostDraft,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    match field {
        EditorFieldId::Date => labeled_field(
            field.label(),
            field.field_id(),
            fields.date.clone(),
            saved_draft.date.clone(),
            1,
            1,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::ProblemTitle => labeled_field(
            field.label(),
            field.field_id(),
            fields.problem_title.clone(),
            saved_draft.problem_title.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::ProblemUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.problem_url.clone(),
            saved_draft.problem_url.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::Difficulty => labeled_field(
            field.label(),
            field.field_id(),
            fields.difficulty.clone(),
            saved_draft.difficulty.clone(),
            1,
            1,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::BlogPostUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.blog_post_url.clone(),
            saved_draft.blog_post_url.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::SubstackUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.substack_url.clone(),
            saved_draft.substack_url.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::YoutubeUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.youtube_url.clone(),
            saved_draft.youtube_url.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::ReferenceUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.reference_url.clone(),
            saved_draft.reference_url.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::TelegramText => labeled_field(
            field.label(),
            field.field_id(),
            fields.telegram_text.clone(),
            saved_draft.telegram_text.clone(),
            3,
            5,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::ProblemTldr => labeled_field(
            field.label(),
            field.field_id(),
            fields.problem_tldr.clone(),
            saved_draft.problem_tldr.clone(),
            3,
            6,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::Intuition => labeled_field(
            field.label(),
            field.field_id(),
            fields.intuition.clone(),
            saved_draft.intuition.clone(),
            6,
            14,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::Approach => labeled_field(
            field.label(),
            field.field_id(),
            fields.approach.clone(),
            saved_draft.approach.clone(),
            6,
            14,
            status,
            ui_preferences,
            theme,
            true,
        ),
        EditorFieldId::TimeComplexity => labeled_field(
            field.label(),
            field.field_id(),
            fields.time_complexity.clone(),
            saved_draft.time_complexity.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            false,
        ),
        EditorFieldId::SpaceComplexity => labeled_field(
            field.label(),
            field.field_id(),
            fields.space_complexity.clone(),
            saved_draft.space_complexity.clone(),
            1,
            2,
            status,
            ui_preferences,
            theme,
            false,
        ),
        EditorFieldId::KotlinRuntimeMs => labeled_field(
            field.label(),
            field.field_id(),
            fields.kotlin_runtime_ms.clone(),
            saved_draft.kotlin_runtime_ms.clone(),
            1,
            1,
            status,
            ui_preferences,
            theme,
            false,
        ),
        EditorFieldId::KotlinCode => labeled_code_field(
            field.label(),
            field.field_id(),
            fields.kotlin_code.clone(),
            saved_draft.kotlin_code.clone(),
            10,
            18,
            status,
            ui_preferences,
            theme,
        ),
        EditorFieldId::RustRuntimeMs => labeled_field(
            field.label(),
            field.field_id(),
            fields.rust_runtime_ms.clone(),
            saved_draft.rust_runtime_ms.clone(),
            1,
            1,
            status,
            ui_preferences,
            theme,
            false,
        ),
        EditorFieldId::RustCode => labeled_code_field(
            field.label(),
            field.field_id(),
            fields.rust_code.clone(),
            saved_draft.rust_code.clone(),
            10,
            18,
            status,
            ui_preferences,
            theme,
        ),
    }
}

impl EditorFieldId {
    fn label(self) -> &'static str {
        match self {
            Self::Date => "Date",
            Self::ProblemTitle => "Problem Title",
            Self::ProblemUrl => "Problem URL",
            Self::Difficulty => "Difficulty",
            Self::BlogPostUrl => "Blog Post URL",
            Self::SubstackUrl => "Substack URL",
            Self::YoutubeUrl => "YouTube URL",
            Self::ReferenceUrl => "Reference URL",
            Self::TelegramText => "Telegram CTA Text",
            Self::ProblemTldr => "Problem TLDR",
            Self::Intuition => "Intuition",
            Self::Approach => "Approach",
            Self::TimeComplexity => "Time Complexity Inner Value",
            Self::SpaceComplexity => "Space Complexity Inner Value",
            Self::KotlinRuntimeMs => "Kotlin Runtime (ms)",
            Self::KotlinCode => "Kotlin Code",
            Self::RustRuntimeMs => "Rust Runtime (ms)",
            Self::RustCode => "Rust Code",
        }
    }

    fn field_id(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::ProblemTitle => "problem_title",
            Self::ProblemUrl => "problem_url",
            Self::Difficulty => "difficulty",
            Self::BlogPostUrl => "blog_post_url",
            Self::SubstackUrl => "substack_url",
            Self::YoutubeUrl => "youtube_url",
            Self::ReferenceUrl => "reference_url",
            Self::TelegramText => "telegram_text",
            Self::ProblemTldr => "problem_tldr",
            Self::Intuition => "intuition",
            Self::Approach => "approach",
            Self::TimeComplexity => "time_complexity",
            Self::SpaceComplexity => "space_complexity",
            Self::KotlinRuntimeMs => "kotlin_runtime_ms",
            Self::KotlinCode => "kotlin_code",
            Self::RustRuntimeMs => "rust_runtime_ms",
            Self::RustCode => "rust_code",
        }
    }

    fn component_key(self) -> String {
        format!("field.{}", self.field_id())
    }
}

fn ordered_fields(defaults: &[EditorFieldId], preferences: &UiPreferences) -> Vec<EditorFieldId> {
    let mut fields = defaults.to_vec();
    fields.sort_by_key(|field| {
        component_sort_key(
            preferences,
            &field.component_key(),
            defaults
                .iter()
                .position(|candidate| candidate == field)
                .unwrap_or(usize::MAX),
        )
    });
    fields
}

#[composable]
fn section_card(theme: ThemeMode, content: impl FnMut() + 'static) {
    ComposeBox(
        Modifier::empty()
            .fill_max_width()
            .background(card_surface(theme))
            .rounded_corners(8.0)
            .padding(22.0),
        BoxSpec::default(),
        content,
    );
}

#[composable]
fn primary_button(
    label: &'static str,
    count_key: &'static str,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
    disabled: bool,
    busy: bool,
    on_click: impl FnMut() + 'static,
) {
    let count = ui_preferences.value().button_count(count_key);
    let count_key = count_key.to_string();
    let busy_pulse = if busy { busy_pulse() } else { 0.0 };
    let background = if busy {
        button_surface(theme).with_alpha(0.66 + 0.26 * busy_pulse)
    } else if disabled {
        disabled_button_surface(theme)
    } else {
        button_surface(theme)
    };
    let text_style = if busy {
        busy_button_text_style(theme, busy_pulse)
    } else if disabled {
        disabled_button_text_style(theme)
    } else {
        button_text_style(theme)
    };
    Button(
        Modifier::empty()
            .background(background)
            .rounded_corners(8.0)
            .padding_symmetric(20.0, 14.0),
        move || {
            if disabled {
                return;
            }
            record_button_press(ui_preferences.clone(), &count_key);
            on_click();
        },
        move || {
            button_content(label.to_string(), count, text_style.clone(), theme, busy);
        },
    );
}

#[composable]
fn subtle_button(
    label: String,
    count_key: String,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
    on_click: impl FnMut() + 'static,
) {
    let count = ui_preferences.value().button_count(&count_key);
    Button(
        Modifier::empty()
            .background(panel_surface(theme))
            .rounded_corners(8.0)
            .padding_symmetric(14.0, 10.0),
        move || {
            record_button_press(ui_preferences.clone(), &count_key);
            on_click();
        },
        move || {
            button_content(
                label.clone(),
                count,
                subtle_button_text_style(theme),
                theme,
                false,
            );
        },
    );
}

#[composable]
fn button_content(label: String, count: u64, style: TextStyle, theme: ThemeMode, busy: bool) {
    Row(
        Modifier::empty(),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            let label = if busy {
                format!("{}...", label)
            } else {
                label.clone()
            };
            Text(label, Modifier::empty(), style.clone());
            button_badge(count, theme);
        },
    );
}

#[composable]
fn busy_pulse() -> f32 {
    let transition = rememberInfiniteTransition("busy_button_pulse");
    transition
        .animateFloat(
            0.35,
            1.0,
            infiniteRepeatable(
                AnimationSpec::linear(650),
                RepeatMode::Reverse,
                StartOffset::default(),
            ),
            "busy_button_pulse",
        )
        .value()
}

#[composable]
fn button_badge(count: u64, theme: ThemeMode) {
    ComposeBox(
        Modifier::empty()
            .background(badge_surface(theme))
            .rounded_corners(999.0)
            .padding_symmetric(7.0, 2.0),
        BoxSpec::default().content_alignment(Alignment::CENTER),
        move || {
            Text(
                count.to_string(),
                Modifier::empty(),
                badge_text_style(theme),
            );
        },
    );
}

#[composable]
fn labeled_field(
    label: &'static str,
    field_id: &'static str,
    state: TextFieldState,
    saved_text: String,
    min_lines: usize,
    max_lines: usize,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
    allow_paste: bool,
) {
    let current_text = state.text();
    track_field_interaction(field_id, current_text.clone(), ui_preferences.clone());
    let is_changed = current_text != saved_text;
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            field_header(
                label,
                field_id,
                state.clone(),
                status.clone(),
                allow_paste,
                is_changed,
                ui_preferences.clone(),
                theme,
            );

            let field_state = state.clone();
            ComposeBox(
                Modifier::empty()
                    .fill_max_width()
                    .background(panel_surface(theme))
                    .rounded_corners(8.0)
                    .padding(14.0),
                BoxSpec::default(),
                move || {
                    BasicTextFieldWithOptions(
                        field_state.clone(),
                        Modifier::empty().fill_max_width(),
                        BasicTextFieldOptions {
                            text_style: field_text_style(theme),
                            cursor_color: accent_color(theme),
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
    field_id: &'static str,
    state: TextFieldState,
    saved_text: String,
    min_lines: usize,
    max_lines: usize,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let current_text = state.text();
    track_field_interaction(field_id, current_text.clone(), ui_preferences.clone());
    let is_changed = current_text != saved_text;
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(8.0)),
        move || {
            field_header(
                label,
                field_id,
                state.clone(),
                status.clone(),
                true,
                is_changed,
                ui_preferences.clone(),
                theme,
            );

            let field_state = state.clone();
            ComposeBox(
                Modifier::empty()
                    .fill_max_width()
                    .background(panel_surface(theme))
                    .rounded_corners(8.0)
                    .padding(14.0),
                BoxSpec::default(),
                move || {
                    BasicTextFieldWithOptions(
                        field_state.clone(),
                        Modifier::empty().fill_max_width(),
                        BasicTextFieldOptions {
                            text_style: code_field_style(theme),
                            cursor_color: accent_color(theme),
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
fn track_field_interaction(
    field_id: &'static str,
    current_text: String,
    ui_preferences: MutableState<UiPreferences>,
) {
    let last_text = useState(|| current_text.clone());
    cranpose_core::LaunchedEffect!(current_text.clone(), {
        let current_text = current_text.clone();
        let last_text = last_text.clone();
        let ui_preferences = ui_preferences.clone();
        let component_key = format!("field.{field_id}");
        move |_scope| {
            if last_text.value() == current_text {
                return;
            }
            last_text.set(current_text);
            record_component_interaction(ui_preferences, &component_key);
        }
    });
}

#[composable]
fn field_header(
    label: &'static str,
    field_id: &'static str,
    state: TextFieldState,
    status: MutableState<String>,
    allow_paste: bool,
    is_changed: bool,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    Row(
        Modifier::empty().fill_max_width(),
        RowSpec::default().horizontal_arrangement(LinearArrangement::SpaceBetween),
        move || {
            Text(label, Modifier::empty(), label_style(theme, is_changed));
            Row(
                Modifier::empty(),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(8.0)),
                {
                    let state = state.clone();
                    let status = status.clone();
                    let ui_preferences = ui_preferences.clone();
                    move || {
                        if allow_paste {
                            let paste_state = state.clone();
                            let paste_status = status.clone();
                            subtle_button(
                                "Paste".to_string(),
                                format!("field.{field_id}.paste"),
                                ui_preferences.clone(),
                                theme,
                                move || {
                                    paste_text_from_clipboard(
                                        paste_state.clone(),
                                        paste_status.clone(),
                                        label,
                                    );
                                },
                            );
                        }

                        let clear_state = state.clone();
                        let clear_status = status.clone();
                        subtle_button(
                            "Clear".to_string(),
                            format!("field.{field_id}.clear"),
                            ui_preferences.clone(),
                            theme,
                            move || {
                                clear_field(clear_state.clone(), clear_status.clone(), label);
                            },
                        );
                    }
                },
            );
        },
    );
}

fn record_button_press(ui_preferences: MutableState<UiPreferences>, count_key: &str) {
    let preferences = ui_preferences.update(|preferences| {
        preferences.increment_button_count(count_key);
        preferences.mark_component_used(count_key);
        preferences.clone()
    });
    let _ = persist_ui_preferences(&preferences);
}

fn record_component_interaction(ui_preferences: MutableState<UiPreferences>, component_key: &str) {
    let preferences = ui_preferences.update(|preferences| {
        preferences.mark_component_used(component_key);
        preferences.clone()
    });
    let _ = persist_ui_preferences(&preferences);
}

fn set_theme_preference(
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
    status: MutableState<String>,
) {
    let preferences = ui_preferences.update(|preferences| {
        preferences.theme = theme;
        preferences.clone()
    });
    match persist_ui_preferences(&preferences) {
        Ok(_) => status.set(format!("Theme set to {}.", theme.label())),
        Err(error) => status.set(format!("Theme preference save failed: {error}")),
    }
}

fn clear_field(state: TextFieldState, status: MutableState<String>, label: &'static str) {
    state.set_text(String::new());
    status.set(format!("{label} cleared."));
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
    let fallback = draft.rich_text_fallback();

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

#[cfg(not(target_arch = "wasm32"))]
fn render_compose_preview_frame(draft: &PostDraft) -> std::result::Result<PreviewFrame, String> {
    let (draft_path, output_path) = compose_capture_paths();
    let result = (|| -> Result<PreviewFrame> {
        write_draft_snapshot(&draft_path, draft)?;
        let command_output = Command::new(
            std::env::current_exe().context("resolving current executable for compose capture")?,
        )
        .arg("--capture-compose-preview")
        .arg(&draft_path)
        .arg(&output_path)
        .output()
        .context("launching compose capture helper")?;

        if !command_output.status.success() {
            let stderr = String::from_utf8_lossy(&command_output.stderr);
            let message = stderr.trim();
            return Err(anyhow::anyhow!(if message.is_empty() {
                "compose capture helper exited unsuccessfully".to_string()
            } else {
                format!("compose capture helper failed: {message}")
            }));
        }

        let image = image::open(&output_path)
            .with_context(|| format!("reading compose capture image {}", output_path.display()))?
            .to_rgba8();
        Ok(PreviewFrame {
            width: image.width(),
            height: image.height(),
            pixels: image.into_raw(),
        })
    })();

    cleanup_capture_artifacts(&draft_path, &output_path);
    result.map_err(|error| error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_compose_capture_cli(draft_path: &Path, output_path: &Path) -> Result<()> {
    let draft = read_draft_snapshot(draft_path)?;
    let compose_assets = compose_preview_assets()?;
    let compose_plan = compose_preview_plan(&draft)?;
    let (tx, rx) = mpsc::channel::<std::result::Result<PreviewFrame, String>>();

    let launch_result = AppLauncher::new()
        .with_title("LeetCode Daily Cranpose Capture")
        .with_size(1600, 900)
        .with_fonts(crate::assets::APP_FONTS)
        .with_headless(true)
        .with_test_driver({
            let tx = tx.clone();
            move |robot| {
                let result = (|| -> std::result::Result<PreviewFrame, String> {
                    robot.wait_for_idle()?;
                    let screenshot = robot.screenshot_with_scale(1.0)?;
                    robot.exit()?;
                    Ok(PreviewFrame {
                        width: screenshot.width,
                        height: screenshot.height,
                        pixels: screenshot.pixels,
                    })
                })();
                let _ = tx.send(result);
            }
        })
        .try_run({
            let compose_assets = compose_assets.clone();
            let compose_plan = compose_plan.clone();
            move || {
                CranposeCaptureSurface(compose_assets.clone(), compose_plan.clone(), 1.0);
            }
        });

    launch_result.map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let frame = rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|error| anyhow::anyhow!("timed out waiting for Cranpose capture: {error}"))?
        .map_err(anyhow::Error::msg)?;
    let image = RgbaImage::from_raw(frame.width, frame.height, frame.pixels)
        .ok_or_else(|| anyhow::anyhow!("invalid RGBA frame from Cranpose capture"))?;
    image
        .save_with_format(output_path, ImageFormat::Png)
        .with_context(|| format!("writing compose capture image {}", output_path.display()))?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn render_compose_preview_frame(_draft: &PostDraft) -> std::result::Result<PreviewFrame, String> {
    Err("Cranpose preview capture is desktop-only right now.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn compose_capture_paths() -> (PathBuf, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "leetcodedaily-compose-{}-{nonce}",
        std::process::id()
    ));
    (base.with_extension("draft"), base.with_extension("png"))
}

#[cfg(not(target_arch = "wasm32"))]
fn cleanup_capture_artifacts(draft_path: &Path, output_path: &Path) {
    let _ = fs::remove_file(draft_path);
    let _ = fs::remove_file(output_path);
}

fn run_long_action(pending: PendingAction) -> LongActionResult {
    match pending.action {
        LongAction::RefreshRasterPreview => {
            LongActionResult::RefreshRasterPreview(render_raster_preview_result(&pending.draft))
        }
        LongAction::RefreshCranposePreview => {
            LongActionResult::RefreshCranposePreview(render_compose_preview_result(&pending.draft))
        }
        LongAction::SaveRasterWebp => LongActionResult::SaveRasterWebp(
            save_webp(&pending.draft).map_err(|error| error.to_string()),
        ),
        LongAction::SaveCranposeWebp => LongActionResult::SaveCranposeWebp(
            save_compose_webp_result(&pending.draft).map_err(|error| error.to_string()),
        ),
        LongAction::PublishBlog => {
            LongActionResult::PublishBlog(publish_blog_result(&pending.draft))
        }
        LongAction::PostTelegram => {
            LongActionResult::PostTelegram(post_telegram_channel_result(&pending.draft))
        }
        LongAction::PostTelegramComment => LongActionResult::PostTelegramComment(
            post_telegram_comment_result(&pending.draft, &pending.telegram_post_link),
        ),
    }
}

fn finish_long_action(
    result: LongActionResult,
    preview_state: MutableState<PreviewState>,
    compose_preview_state: MutableState<PreviewState>,
    compose_error: MutableState<String>,
    busy_action: MutableState<Option<LongAction>>,
    pending_action: MutableState<Option<PendingAction>>,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
) {
    busy_action.set(None);
    pending_action.set(None);

    match result {
        LongActionResult::RefreshRasterPreview(result) => match result {
            Ok(preview) => {
                preview_state.set(preview);
                status.set("Raster preview refreshed.".to_string());
            }
            Err(error) => status.set(format!("Raster preview failed: {error}")),
        },
        LongActionResult::RefreshCranposePreview(result) => match result {
            Ok(preview) => {
                compose_preview_state.set(preview);
                compose_error.set(String::new());
                status.set("Cranpose preview refreshed.".to_string());
            }
            Err(error) => {
                compose_error.set(error.clone());
                status.set(format!("Cranpose preview failed: {error}"));
            }
        },
        LongActionResult::SaveRasterWebp(result) => match result {
            Ok(preview) => {
                let saved_to = preview
                    .last_saved_webp_path
                    .clone()
                    .unwrap_or_else(|| "~/Downloads".to_string());
                preview_state.set(preview);
                status.set(format!("Raster WebP saved to {saved_to}"));
            }
            Err(error) => status.set(format!("Saving raster WebP failed: {error}")),
        },
        LongActionResult::SaveCranposeWebp(result) => match result {
            Ok(preview) => {
                let saved_to = preview
                    .last_saved_webp_path
                    .clone()
                    .unwrap_or_else(|| "~/Downloads".to_string());
                preview_state.set(preview);
                status.set(format!("Cranpose WebP saved to {saved_to}"));
            }
            Err(error) => status.set(format!("Saving Cranpose WebP failed: {error}")),
        },
        LongActionResult::PublishBlog(result) => match result {
            Ok(outcome) => {
                preview_state.set(outcome.preview);
                let action = match outcome.edit {
                    BlogArchiveEdit::Inserted => "inserted",
                    BlogArchiveEdit::Replaced => "replaced",
                };
                match outcome.commit_sha {
                    Some(sha) => status.set(format!(
                        "Blog post {action}, image copied, committed {sha}."
                    )),
                    None => status.set(format!(
                        "Blog post {action}; archive and image were already committed."
                    )),
                }
            }
            Err(error) => status.set(format!("Publishing blog failed: {error}")),
        },
        LongActionResult::PostTelegram(result) => match result {
            Ok(outcome) => {
                preview_state.set(outcome.preview);
                telegram_post_link.set(outcome.link.clone());
                status.set(format!("Telegram post published: {}", outcome.link));
            }
            Err(error) => status.set(format!("Telegram post failed: {error}")),
        },
        LongActionResult::PostTelegramComment(result) => match result {
            Ok(link) => status.set(format!("Telegram comment published: {link}")),
            Err(error) => status.set(format!("Telegram comment failed: {error}")),
        },
    }
}

fn render_raster_preview_result(draft: &PostDraft) -> std::result::Result<PreviewState, String> {
    render_preview_frame(draft)
        .map_err(|error| error.to_string())
        .and_then(|frame| PreviewState::from_frame(frame).map_err(|error| error.to_string()))
}

fn render_compose_preview_result(draft: &PostDraft) -> std::result::Result<PreviewState, String> {
    render_compose_preview_frame(draft)
        .map_err(|error| error.to_string())
        .and_then(|frame| PreviewState::from_frame(frame).map_err(|error| error.to_string()))
}

fn save_compose_webp_result(draft: &PostDraft) -> Result<PreviewState> {
    let frame = render_compose_preview_frame(draft).map_err(anyhow::Error::msg)?;
    save_preview_frame_as_webp(frame, draft)
}

#[cfg(not(target_arch = "wasm32"))]
fn publish_blog_result(draft: &PostDraft) -> std::result::Result<PublishBlogOutcome, String> {
    let preview = save_webp(draft).map_err(|error| format!("WebP save failed: {error}"))?;
    let Some(webp_path) = preview.last_saved_webp_path.clone() else {
        return Err("WebP save returned no path.".to_string());
    };
    let result = publish_blog_post(draft, &webp_path).map_err(|error| error.to_string())?;
    let edit = match result.edit {
        ArchiveEdit::Inserted => BlogArchiveEdit::Inserted,
        ArchiveEdit::Replaced => BlogArchiveEdit::Replaced,
    };
    Ok(PublishBlogOutcome {
        preview,
        edit,
        commit_sha: result.commit_sha,
    })
}

#[cfg(target_arch = "wasm32")]
fn publish_blog_result(_draft: &PostDraft) -> std::result::Result<PublishBlogOutcome, String> {
    Err("Blog publishing is desktop-only.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn post_telegram_channel_result(
    draft: &PostDraft,
) -> std::result::Result<TelegramPostOutcome, String> {
    let preview = save_webp(draft).map_err(|error| format!("WebP save failed: {error}"))?;
    let Some(webp_path) = preview.last_saved_webp_path.clone() else {
        return Err("WebP save returned no path.".to_string());
    };
    let link = run_telegram_channel_script(draft, &webp_path).map_err(|error| error.to_string())?;
    Ok(TelegramPostOutcome { preview, link })
}

#[cfg(target_arch = "wasm32")]
fn post_telegram_channel_result(
    _draft: &PostDraft,
) -> std::result::Result<TelegramPostOutcome, String> {
    Err("Telegram posting is desktop-only.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn post_telegram_comment_result(
    draft: &PostDraft,
    post_link: &str,
) -> std::result::Result<String, String> {
    run_telegram_comment_script(draft, post_link).map_err(|error| error.to_string())
}

#[cfg(target_arch = "wasm32")]
fn post_telegram_comment_result(
    _draft: &PostDraft,
    _post_link: &str,
) -> std::result::Result<String, String> {
    Err("Telegram comment posting is desktop-only.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn run_telegram_channel_script(draft: &PostDraft, webp_path: &str) -> Result<String> {
    let script_path = telegram_script_path("telegram_post_channel.py")?;
    let output = Command::new("python3")
        .arg(script_path)
        .arg("--date")
        .arg(draft.date_or_placeholder())
        .arg("--title")
        .arg(draft.problem_title.trim())
        .arg("--difficulty")
        .arg(draft.difficulty_or_placeholder())
        .arg("--tldr")
        .arg(draft.problem_tldr.trim())
        .arg("--blog-url")
        .arg(draft.reference_url.trim())
        .arg("--substack-url")
        .arg(draft.substack_url.trim())
        .arg("--youtube-url")
        .arg(draft.youtube_url.trim())
        .arg("--image")
        .arg(webp_path)
        .output()
        .context("launching Telegram channel script")?;
    script_json_link(output)
}

#[cfg(not(target_arch = "wasm32"))]
fn run_telegram_comment_script(draft: &PostDraft, post_link: &str) -> Result<String> {
    let script_path = telegram_script_path("telegram_post_comment.py")?;
    let body_path = telegram_temp_path("comment.md");
    fs::write(&body_path, draft.rich_text_fallback())
        .with_context(|| format!("writing Telegram comment body {}", body_path.display()))?;

    let result = (|| {
        let mut command = Command::new("python3");
        command.arg(script_path).arg("--body-file").arg(&body_path);
        if !post_link.trim().is_empty() {
            command.arg("--post-link").arg(post_link.trim());
        }
        let output = command
            .output()
            .context("launching Telegram comment script")?;
        script_json_link(output)
    })();

    let _ = fs::remove_file(&body_path);
    result
}

#[cfg(not(target_arch = "wasm32"))]
fn telegram_script_path(name: &str) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("LEETCODE_DAILY_TELEGRAM_SCRIPTS_DIR") {
        candidates.push(PathBuf::from(dir).join(name));
    }
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        candidates.push(PathBuf::from(dir).join("leetcodedaily/scripts").join(name));
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            PathBuf::from(home)
                .join(".config/leetcodedaily/scripts")
                .join(name),
        );
    }
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        candidates.push(exe_dir.join("scripts").join(name));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join(name),
    );

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    Err(anyhow::anyhow!(
        "Telegram script {name} not found; set LEETCODE_DAILY_TELEGRAM_SCRIPTS_DIR or install scripts next to the app"
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn telegram_temp_path(extension: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "leetcodedaily-telegram-{}-{nonce}.{extension}",
        std::process::id()
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn script_json_link(output: std::process::Output) -> Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(anyhow::anyhow!(message.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    extract_json_string(&stdout, "link")
        .ok_or_else(|| anyhow::anyhow!("Telegram script did not return a link: {}", stdout.trim()))
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_json_string(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let start = json.find(&needle)?;
    let after_field = &json[start + needle.len()..];
    let colon = after_field.find(':')?;
    let after_colon = after_field[colon + 1..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let mut escaped = false;
    let mut value = String::new();
    for character in after_colon[1..].chars() {
        if escaped {
            value.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(value);
        } else {
            value.push(character);
        }
    }
    None
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

#[cfg(not(target_arch = "wasm32"))]
fn scale_x(value: i32, scale: f32) -> f32 {
    value as f32 * scale
}

#[cfg(not(target_arch = "wasm32"))]
fn scale_y(value: i32, scale: f32) -> f32 {
    value as f32 * scale
}

#[cfg(not(target_arch = "wasm32"))]
fn scaled_size(width: u32, height: u32, scale: f32) -> Size {
    Size {
        width: width as f32 * scale,
        height: height as f32 * scale,
    }
}

fn heading_style(size: f32, theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(primary_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(size),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn muted_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(muted_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(15.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn body_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(body_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(18.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn eyebrow_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(accent_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(15.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn stage_label_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(muted_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(16.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn accent_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(accent_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(17.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn code_text_style(size: f32, theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(primary_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(size),
            font_family: Some(cranpose::text::FontFamily::Monospace),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn field_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(primary_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(18.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn code_field_style(theme: ThemeMode) -> TextStyle {
    code_text_style(18.0, theme)
}

fn label_style(theme: ThemeMode, is_changed: bool) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(if is_changed {
                changed_label_color(theme)
            } else {
                label_color(theme)
            }),
            font_size: cranpose::text::TextUnit::Sp(16.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn button_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(button_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(17.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn focus_button_text_style(theme: ThemeMode, pulse: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(button_text_color(theme).with_alpha(0.82 + 0.18 * pulse.max(0.0))),
            font_size: cranpose::text::TextUnit::Sp(22.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn busy_button_text_style(theme: ThemeMode, pulse: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(button_text_color(theme).with_alpha(0.72 + 0.28 * pulse)),
            font_size: cranpose::text::TextUnit::Sp(17.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn disabled_button_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(muted_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(17.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn subtle_button_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(label_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(15.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn queue_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(primary_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(14.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn badge_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(badge_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(12.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn preview_tldr_style(size: f32, line_height: f32) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(Color::from_rgb_u8(170, 176, 187)),
            font_size: cranpose::text::TextUnit::Sp(size.max(10.0)),
            font_weight: Some(cranpose::text::FontWeight::MEDIUM),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle {
            text_align: cranpose::text::TextAlign::Center,
            line_height: cranpose::text::TextUnit::Sp(line_height.max(size)),
            ..ParagraphStyle::default()
        },
    }
}

fn ui_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(13, 12, 17),
        ThemeMode::Light => Color::from_rgb_u8(240, 242, 238),
    }
}

fn card_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(24, 23, 29),
        ThemeMode::Light => Color::from_rgb_u8(252, 253, 255),
    }
}

fn panel_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(35, 34, 42),
        ThemeMode::Light => Color::from_rgb_u8(228, 232, 225),
    }
}

fn next_panel_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(31, 38, 31),
        ThemeMode::Light => Color::from_rgb_u8(232, 241, 225),
    }
}

fn stage_surface(theme: ThemeMode, active: bool) -> Color {
    if active {
        match theme {
            ThemeMode::Dark => Color::from_rgb_u8(56, 45, 25),
            ThemeMode::Light => Color::from_rgb_u8(246, 230, 199),
        }
    } else {
        panel_surface(theme)
    }
}

fn button_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(244, 173, 74),
        ThemeMode::Light => Color::from_rgb_u8(183, 88, 42),
    }
}

fn disabled_button_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(48, 47, 55),
        ThemeMode::Light => Color::from_rgb_u8(218, 222, 216),
    }
}

fn badge_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(62, 61, 69),
        ThemeMode::Light => Color::from_rgb_u8(207, 213, 204),
    }
}

fn primary_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(247, 245, 238),
        ThemeMode::Light => Color::from_rgb_u8(35, 36, 31),
    }
}

fn body_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(204, 201, 193),
        ThemeMode::Light => Color::from_rgb_u8(70, 75, 67),
    }
}

fn muted_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(151, 148, 141),
        ThemeMode::Light => Color::from_rgb_u8(95, 101, 91),
    }
}

fn label_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(115, 214, 165),
        ThemeMode::Light => Color::from_rgb_u8(28, 107, 73),
    }
}

fn changed_label_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(126, 216, 240),
        ThemeMode::Light => Color::from_rgb_u8(13, 103, 130),
    }
}

fn accent_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(245, 177, 80),
        ThemeMode::Light => Color::from_rgb_u8(171, 76, 37),
    }
}

fn button_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(19, 17, 15),
        ThemeMode::Light => Color::from_rgb_u8(255, 252, 246),
    }
}

fn badge_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(237, 235, 226),
        ThemeMode::Light => Color::from_rgb_u8(55, 59, 51),
    }
}

#[cfg(test)]
mod tests {
    use crate::draft::{PostDraft, UiPreferences};
    use crate::export::PreviewState;

    use super::{
        APP_HEIGHT, APP_WIDTH, ActionButtonId, EditorFieldId, META_FIELDS, NextWorkItem,
        WEB_SURFACE_MAX_DIM, compute_web_canvas_size, ordered_action_buttons, ordered_fields,
        recommended_next_work,
    };

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

    #[test]
    fn remembered_action_order_moves_used_buttons_first() {
        let mut preferences = UiPreferences::default();
        preferences.mark_component_used(ActionButtonId::CopyBlog.count_key());
        preferences.mark_component_used(ActionButtonId::PostTelegram.count_key());

        let ordered = ordered_action_buttons(&preferences);

        assert_eq!(ordered[0], ActionButtonId::CopyBlog);
        assert_eq!(ordered[1], ActionButtonId::PostTelegram);
        assert_eq!(ordered[2], ActionButtonId::CopyLeetcode);
    }

    #[test]
    fn remembered_field_order_moves_used_fields_first() {
        let mut preferences = UiPreferences::default();
        preferences.mark_component_used(&EditorFieldId::YoutubeUrl.component_key());
        preferences.mark_component_used(&EditorFieldId::ProblemTitle.component_key());

        let ordered = ordered_fields(&META_FIELDS, &preferences);

        assert_eq!(ordered[0], EditorFieldId::YoutubeUrl);
        assert_eq!(ordered[1], EditorFieldId::ProblemTitle);
        assert_eq!(ordered[2], EditorFieldId::Date);
    }

    #[test]
    fn next_work_prioritizes_missing_prepare_field() {
        let mut draft = PostDraft::default();
        draft.problem_title.clear();
        let preview = PreviewState::placeholder();

        let next = recommended_next_work(&draft, &preview, "", &UiPreferences::default());

        assert_eq!(next, NextWorkItem::Field(EditorFieldId::ProblemTitle));
    }

    #[test]
    fn next_work_recommends_image_after_complete_draft() {
        let draft = PostDraft::default();
        let preview = PreviewState::placeholder();

        let next = recommended_next_work(&draft, &preview, "", &UiPreferences::default());

        assert_eq!(next, NextWorkItem::Action(ActionButtonId::SaveRasterWebp));
    }
}
