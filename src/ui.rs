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
    PreviewFrame, PreviewState, preview_webp_data_url, render_preview_frame,
    save_preview_frame_as_webp, save_webp,
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
use cranpose_foundation::DrawScope;
use cranpose_foundation::text::{TextFieldLineLimits, TextFieldState};
#[cfg(not(target_arch = "wasm32"))]
use image::ImageFormat;
use image::{RgbaImage, imageops::FilterType};
#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Object, Promise, Reflect};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
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
const APP_BOTTOM_LIST_GAP: f32 = 50.0;
const INTERACTIVE_QUEUE_CHIP_WIDTH: f32 = 214.0;
const INTERACTIVE_QUEUE_CHIP_GAP: f32 = 10.0;
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
    pushed: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum UiIcon {
    AppLogo,
    Save,
    Code,
    Telegram,
    Title,
    Subtitle,
    RichText,
    Youtube,
    Comment,
    Blog,
    Refresh,
    RefreshAlt,
    CranposeSave,
    Document,
    Leetcode,
    Substack,
    Date,
    Difficulty,
    Web,
    StagePrepare,
    StageWrite,
    StageCode,
    StageReview,
    StageShip,
    Theme,
    Paste,
    Clear,
    Generic,
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

#[cfg(test)]
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
    let startup_interactive_queue = remember({
        let initial_queue = ui_preferences.value().interactive_queue().to_vec();
        move || initial_queue
    })
    .with(|queue| queue.clone());
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
    let active_queue_target = useState(|| None::<String>);
    let current_draft = PostDraft::from_fields(&fields);
    let markdown_preview = current_draft.blog_template();
    let queued_action = pending_action.value();
    let theme = ui_preferences.value().theme;
    let queue_reset_done = useState(|| false);

    cranpose_core::LaunchedEffect!(queue_reset_done.value(), {
        let queue_reset_done = queue_reset_done.clone();
        let ui_preferences = ui_preferences.clone();
        move |_scope| {
            if queue_reset_done.value() {
                return;
            }
            queue_reset_done.set(true);
            let preferences = ui_preferences.update(|preferences| {
                preferences.clear_interactive_queue();
                preferences.clone()
            });
            let _ = persist_ui_preferences(&preferences);
        }
    });

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

    ComposeBox(
        Modifier::empty().fill_max_size().draw_behind(move |scope| {
            draw_app_background(scope);
        }),
        BoxSpec::default(),
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
            let startup_interactive_queue = startup_interactive_queue.clone();
            let pending_action = pending_action.clone();
            let action_request_counter = action_request_counter.clone();
            let busy_action = busy_action.clone();
            let active_queue_target = active_queue_target.clone();
            move || {
                Column(Modifier::empty().fill_max_size(), ColumnSpec::default(), {
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
                    let startup_interactive_queue = startup_interactive_queue.clone();
                    let pending_action = pending_action.clone();
                    let action_request_counter = action_request_counter.clone();
                    let busy_action = busy_action.clone();
                    let active_queue_target = active_queue_target.clone();
                    move || {
                        BoxWithConstraints(Modifier::empty().fill_max_width().weight(1.0), {
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
                            let startup_interactive_queue = startup_interactive_queue.clone();
                            let pending_action = pending_action.clone();
                            let action_request_counter = action_request_counter.clone();
                            let busy_action = busy_action.clone();
                            let active_queue_target = active_queue_target.clone();
                            move |scope| {
                                let compact = scope.max_width().0 < 1120.0;
                                let root_horizontal_padding = if scope.max_width().0 < 700.0 {
                                    18.0
                                } else if compact {
                                    24.0
                                } else {
                                    34.0
                                };
                                Column(
                                    Modifier::empty().fill_max_size().padding_each(
                                        root_horizontal_padding,
                                        30.0,
                                        root_horizontal_padding,
                                        APP_BOTTOM_LIST_GAP,
                                    ),
                                    ColumnSpec::default()
                                        .vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                                    {
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
                                        let startup_interactive_queue =
                                            startup_interactive_queue.clone();
                                        let pending_action = pending_action.clone();
                                        let action_request_counter = action_request_counter.clone();
                                        let busy_action = busy_action.clone();
                                        let active_queue_target = active_queue_target.clone();
                                        let workspace_scroll_state = scroll_state.clone();
                                        move || {
                                            ActionsCard(
                                                fields.clone(),
                                                status.clone(),
                                                preview_state.clone(),
                                                autosave_destination.clone(),
                                                telegram_post_link.clone(),
                                                ui_preferences.clone(),
                                                layout_preferences.clone(),
                                                startup_interactive_queue.clone(),
                                                pending_action.clone(),
                                                action_request_counter.clone(),
                                                busy_action.clone(),
                                                active_queue_target.clone(),
                                                theme,
                                                compact,
                                            );
                                            let viewport_scroll_state =
                                                workspace_scroll_state.clone();
                                            BoxWithConstraints(
                                                Modifier::empty()
                                                    .fill_max_width()
                                                    .weight(1.0)
                                                    .padding_each(0.0, 12.0, 0.0, 0.0),
                                                {
                                                    let fields = fields.clone();
                                                    let status = status.clone();
                                                    let preview_state = preview_state.clone();
                                                    let preview_loading = preview_loading.clone();
                                                    let compose_preview_state =
                                                        compose_preview_state.clone();
                                                    let compose_loading = compose_loading.clone();
                                                    let compose_error = compose_error.clone();
                                                    let markdown_preview = markdown_preview.clone();
                                                    let saved_draft = saved_draft.clone();
                                                    let ui_preferences = ui_preferences.clone();
                                                    let layout_preferences =
                                                        layout_preferences.clone();
                                                    let active_queue_target =
                                                        active_queue_target.clone();
                                                    move |viewport_scope| {
                                                        let viewport_width =
                                                            viewport_scope.max_width().0;
                                                        let viewport_height =
                                                            viewport_scope.max_height().0;
                                                        ComposeBox(
                                                            workspace_viewport_modifier(
                                                                Modifier::empty().fill_max_size(),
                                                                theme,
                                                                viewport_scroll_state.clone(),
                                                                viewport_width,
                                                                viewport_height,
                                                            ),
                                                            BoxSpec::default(),
                                                            {
                                                                let fields = fields.clone();
                                                                let status = status.clone();
                                                                let preview_state =
                                                                    preview_state.clone();
                                                                let preview_loading =
                                                                    preview_loading.clone();
                                                                let compose_preview_state =
                                                                    compose_preview_state.clone();
                                                                let compose_loading =
                                                                    compose_loading.clone();
                                                                let compose_error =
                                                                    compose_error.clone();
                                                                let markdown_preview =
                                                                    markdown_preview.clone();
                                                                let saved_draft =
                                                                    saved_draft.clone();
                                                                let ui_preferences =
                                                                    ui_preferences.clone();
                                                                let layout_preferences =
                                                                    layout_preferences.clone();
                                                                let active_queue_target =
                                                                    active_queue_target.clone();
                                                                let viewport_scroll_state =
                                                                    viewport_scroll_state.clone();
                                                                move || {
                                                                    Column(
                                                                Modifier::empty()
                                                                    .fill_max_size()
                                                                    .vertical_scroll(
                                                                        viewport_scroll_state
                                                                            .clone(),
                                                                        false,
                                                                    )
                                                                    .padding_each(
                                                                        0.0, 22.0, 0.0, 0.0,
                                                                    ),
                                                                ColumnSpec::default()
                                                                    .vertical_arrangement(
                                                                    LinearArrangement::spaced_by(
                                                                        22.0,
                                                                    ),
                                                                ),
                                                                {
                                                                    let fields = fields.clone();
                                                                    let status = status.clone();
                                                                    let preview_state =
                                                                        preview_state.clone();
                                                                    let preview_loading =
                                                                        preview_loading.clone();
                                                                    let compose_preview_state =
                                                                        compose_preview_state
                                                                            .clone();
                                                                    let compose_loading =
                                                                        compose_loading.clone();
                                                                    let compose_error =
                                                                        compose_error.clone();
                                                                    let markdown_preview =
                                                                        markdown_preview.clone();
                                                                    let saved_draft =
                                                                        saved_draft.clone();
                                                                    let ui_preferences =
                                                                        ui_preferences.clone();
                                                                    let layout_preferences =
                                                                        layout_preferences.clone();
                                                                    move || {
                                                                        GuidedWorkspace(
                                                                            fields.clone(),
                                                                            preview_state.clone(),
                                                                            preview_loading.clone(),
                                                                            compose_preview_state
                                                                                .clone(),
                                                                            compose_loading.clone(),
                                                                            compose_error.clone(),
                                                                            markdown_preview
                                                                                .clone(),
                                                                            status.clone(),
                                                                            saved_draft.clone(),
                                                                            ui_preferences.clone(),
                                                                            layout_preferences
                                                                                .clone(),
                                                                            active_queue_target
                                                                                .clone(),
                                                                            theme,
                                                                            compact,
                                                                        );
                                                                        Spacer(Size::new(
                                                                            0.0, 86.0,
                                                                        ));
                                                                    }
                                                                },
                                                            );
                                                                }
                                                            },
                                                        );
                                                    }
                                                },
                                            );
                                        }
                                    },
                                );
                            }
                        });
                        BottomListGapMask();
                    }
                });
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
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
    compact: bool,
) {
    ProblemMetaCard(
        fields.clone(),
        status.clone(),
        saved_draft.clone(),
        ui_preferences.clone(),
        layout_preferences.clone(),
        active_queue_target.clone(),
        theme,
        compact,
    );
    WriteupCard(
        fields.clone(),
        status.clone(),
        saved_draft.clone(),
        ui_preferences.clone(),
        layout_preferences.clone(),
        active_queue_target.clone(),
        theme,
    );
    Spacer(Size::new(0.0, 82.0));
    CodeCard(
        fields,
        status,
        saved_draft,
        ui_preferences,
        layout_preferences,
        active_queue_target,
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
    startup_interactive_queue: Vec<String>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
    compact: bool,
) {
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
            let startup_interactive_queue = startup_interactive_queue.clone();
            let pending_action = pending_action.clone();
            let action_request_counter = action_request_counter.clone();
            let busy_action = busy_action.clone();
            let active_queue_target = active_queue_target.clone();
            move || {
                HeaderBar(
                    autosave_destination.clone(),
                    ui_preferences.clone(),
                    status.clone(),
                    theme,
                    compact,
                );

                let draft = PostDraft::from_fields(&fields);
                let preview = preview_state.value();
                let latest_telegram_link = telegram_post_link.value();
                let current_queue = ui_preferences.value().interactive_queue().to_vec();
                let next_queue_key =
                    interactive_queue_next_key(&startup_interactive_queue, &current_queue);
                let next_item = next_queue_key
                    .as_deref()
                    .and_then(next_work_item_from_queue_key)
                    .unwrap_or_else(|| {
                        recommended_next_work(
                            &draft,
                            &preview,
                            &latest_telegram_link,
                            &layout_preferences,
                        )
                    });
                let next_title = next_queue_key
                    .as_deref()
                    .map(|key| interactive_queue_label(key, false, false));
                if compact {
                    Column(
                        Modifier::empty().fill_max_width(),
                        ColumnSpec::default()
                            .vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                        {
                            let fields = fields.clone();
                            let status = status.clone();
                            let telegram_post_link = telegram_post_link.clone();
                            let ui_preferences = ui_preferences.clone();
                            let layout_preferences = layout_preferences.clone();
                            let pending_action = pending_action.clone();
                            let action_request_counter = action_request_counter.clone();
                            let busy_action = busy_action.clone();
                            let active_queue_target = active_queue_target.clone();
                            move || {
                                NextWorkPanel(
                                    next_item,
                                    next_title.clone(),
                                    fields.clone(),
                                    status.clone(),
                                    telegram_post_link.clone(),
                                    ui_preferences.clone(),
                                    pending_action.clone(),
                                    action_request_counter.clone(),
                                    busy_action.clone(),
                                    active_queue_target.clone(),
                                    theme,
                                    true,
                                );
                                QuickActionsPanel(
                                    fields.clone(),
                                    status.clone(),
                                    telegram_post_link.clone(),
                                    ui_preferences.clone(),
                                    layout_preferences.clone(),
                                    pending_action.clone(),
                                    action_request_counter.clone(),
                                    busy_action.clone(),
                                    theme,
                                    true,
                                );
                            }
                        },
                    );
                } else {
                    Row(
                        Modifier::empty().fill_max_width(),
                        RowSpec::default()
                            .horizontal_arrangement(LinearArrangement::spaced_by(18.0)),
                        {
                            let fields = fields.clone();
                            let status = status.clone();
                            let telegram_post_link = telegram_post_link.clone();
                            let ui_preferences = ui_preferences.clone();
                            let layout_preferences = layout_preferences.clone();
                            let pending_action = pending_action.clone();
                            let action_request_counter = action_request_counter.clone();
                            let busy_action = busy_action.clone();
                            let active_queue_target = active_queue_target.clone();
                            move || {
                                NextWorkPanel(
                                    next_item,
                                    next_title.clone(),
                                    fields.clone(),
                                    status.clone(),
                                    telegram_post_link.clone(),
                                    ui_preferences.clone(),
                                    pending_action.clone(),
                                    action_request_counter.clone(),
                                    busy_action.clone(),
                                    active_queue_target.clone(),
                                    theme,
                                    false,
                                );
                                QuickActionsPanel(
                                    fields.clone(),
                                    status.clone(),
                                    telegram_post_link.clone(),
                                    ui_preferences.clone(),
                                    layout_preferences.clone(),
                                    pending_action.clone(),
                                    action_request_counter.clone(),
                                    busy_action.clone(),
                                    theme,
                                    false,
                                );
                            }
                        },
                    );
                }

                InteractiveQueuePanel(
                    startup_interactive_queue.clone(),
                    fields.clone(),
                    status.clone(),
                    telegram_post_link.clone(),
                    ui_preferences.clone(),
                    pending_action.clone(),
                    action_request_counter.clone(),
                    busy_action.clone(),
                    active_queue_target.clone(),
                    theme,
                );

                StatusStrip(status.value(), theme);

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

#[composable]
fn HeaderBar(
    autosave_destination: String,
    ui_preferences: MutableState<UiPreferences>,
    status: MutableState<String>,
    theme: ThemeMode,
    compact: bool,
) {
    if compact {
        Column(
            Modifier::empty().fill_max_width(),
            ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(10.0)),
            move || {
                HeaderTitle(autosave_destination.clone(), theme, true);
                let next_theme = theme.toggled();
                theme_button(format!("Theme: {}", theme.label()), theme, move || {
                    record_button_press(ui_preferences.clone(), "theme.toggle");
                    set_theme_preference(ui_preferences.clone(), next_theme, status.clone());
                });
            },
        );
    } else {
        Row(
            Modifier::empty().fill_max_width(),
            RowSpec::default()
                .horizontal_arrangement(LinearArrangement::SpaceBetween)
                .vertical_alignment(VerticalAlignment::CenterVertically),
            move || {
                HeaderTitle(autosave_destination.clone(), theme, false);
                let next_theme = theme.toggled();
                theme_button(format!("Theme: {}", theme.label()), theme, move || {
                    record_button_press(ui_preferences.clone(), "theme.toggle");
                    set_theme_preference(ui_preferences.clone(), next_theme, status.clone());
                });
            },
        );
    }
}

#[composable]
fn HeaderTitle(autosave_destination: String, theme: ThemeMode, compact: bool) {
    Row(
        Modifier::empty(),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(18.0)),
        move || {
            let autosave_destination = autosave_destination.clone();
            AppLogo();
            Column(
                Modifier::empty(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(3.0)),
                move || {
                    BasicText(
                        "LeetCode Daily Composer",
                        Modifier::empty(),
                        app_title_style(theme, compact),
                        TextOverflow::Ellipsis,
                        false,
                        1,
                        1,
                    );
                    BasicText(
                        autosave_destination.clone(),
                        Modifier::empty(),
                        muted_style(theme),
                        TextOverflow::Ellipsis,
                        false,
                        1,
                        1,
                    );
                },
            );
        },
    );
}

#[composable]
fn QuickActionsPanel(
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    theme: ThemeMode,
    compact: bool,
) {
    let modifier = if compact {
        Modifier::empty().fill_max_width()
    } else {
        Modifier::empty().weight(2.04)
    };
    glass_panel(modifier, theme, 18.0, 18.0, {
        let fields = fields.clone();
        let status = status.clone();
        let telegram_post_link = telegram_post_link.clone();
        let ui_preferences = ui_preferences.clone();
        let pending_action = pending_action.clone();
        let action_request_counter = action_request_counter.clone();
        let busy_action = busy_action.clone();
        let layout_preferences = layout_preferences.clone();
        move || {
            let layout_preferences = layout_preferences.clone();
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
                        Text("Quick Actions", Modifier::empty(), panel_title_style(theme));
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
    });
}

#[composable]
fn StatusStrip(message: String, theme: ThemeMode) {
    glass_panel(
        Modifier::empty().fill_max_width(),
        theme,
        14.0,
        12.0,
        move || {
            let message = message.clone();
            Row(
                Modifier::empty().fill_max_width(),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(10.0)),
                move || {
                    StatusDot(true, theme);
                    BasicText(
                        message.clone(),
                        Modifier::empty().weight(1.0),
                        accent_style(theme),
                        TextOverflow::Ellipsis,
                        false,
                        1,
                        1,
                    );
                },
            );
        },
    );
}

#[composable]
fn NextWorkPanel(
    next_item: NextWorkItem,
    title_override: Option<String>,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
    compact: bool,
) {
    let modifier = if compact {
        Modifier::empty().fill_max_width()
    } else {
        Modifier::empty().weight(1.0)
    };
    let title = title_override.unwrap_or_else(|| next_item.title());
    glass_panel(modifier, theme, 18.0, 18.0, {
        let fields = fields.clone();
        let status = status.clone();
        let telegram_post_link = telegram_post_link.clone();
        let ui_preferences = ui_preferences.clone();
        let pending_action = pending_action.clone();
        let action_request_counter = action_request_counter.clone();
        let busy_action = busy_action.clone();
        let active_queue_target = active_queue_target.clone();
        let title = title.clone();
        move || {
            Row(
                Modifier::empty().fill_max_width(),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(18.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let telegram_post_link = telegram_post_link.clone();
                    let ui_preferences = ui_preferences.clone();
                    let pending_action = pending_action.clone();
                    let action_request_counter = action_request_counter.clone();
                    let busy_action = busy_action.clone();
                    let active_queue_target = active_queue_target.clone();
                    let row_title = title.clone();
                    move || {
                        HeroTile(next_item.stage(), theme);
                        Column(
                            Modifier::empty().weight(1.0),
                            ColumnSpec::default()
                                .vertical_arrangement(LinearArrangement::spaced_by(12.0)),
                            {
                                let fields = fields.clone();
                                let status = status.clone();
                                let telegram_post_link = telegram_post_link.clone();
                                let ui_preferences = ui_preferences.clone();
                                let pending_action = pending_action.clone();
                                let action_request_counter = action_request_counter.clone();
                                let busy_action = busy_action.clone();
                                let active_queue_target = active_queue_target.clone();
                                let title = row_title.clone();
                                move || {
                                    Row(
                                        Modifier::empty().fill_max_width(),
                                        RowSpec::default().horizontal_arrangement(
                                            LinearArrangement::SpaceBetween,
                                        ),
                                        move || {
                                            Text("Now", Modifier::empty(), eyebrow_style(theme));
                                            Text(
                                                next_item.stage().label(),
                                                Modifier::empty(),
                                                stage_label_style(theme),
                                            );
                                        },
                                    );
                                    Text(
                                        title.clone(),
                                        Modifier::empty(),
                                        heading_style(21.0, theme),
                                    );
                                    match next_item {
                                        NextWorkItem::Field(field) => {
                                            FieldSuggestion(
                                                field,
                                                active_queue_target.clone(),
                                                status.clone(),
                                                theme,
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
    });
}

#[composable]
fn InteractiveQueuePanel(
    queue: Vec<String>,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
) {
    if queue.is_empty() {
        return;
    }

    let scroll_state = remember(|| ScrollState::new(0.0)).with(|state| state.clone());
    let scroll_retry = useState(|| 0u64);
    let last_auto_scroll_key = useState(|| None::<String>);
    glass_panel(Modifier::empty().fill_max_width(), theme, 14.0, 10.0, {
        let fields = fields.clone();
        let status = status.clone();
        let telegram_post_link = telegram_post_link.clone();
        let ui_preferences = ui_preferences.clone();
        let pending_action = pending_action.clone();
        let action_request_counter = action_request_counter.clone();
        let busy_action = busy_action.clone();
        let active_queue_target = active_queue_target.clone();
        let scroll_retry = scroll_retry.clone();
        let last_auto_scroll_key = last_auto_scroll_key.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(9.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let telegram_post_link = telegram_post_link.clone();
                    let ui_preferences = ui_preferences.clone();
                    let pending_action = pending_action.clone();
                    let action_request_counter = action_request_counter.clone();
                    let busy_action = busy_action.clone();
                    let active_queue_target = active_queue_target.clone();
                    let queue = queue.clone();
                    let current_queue = ui_preferences.value().interactive_queue().to_vec();
                    let selected_key = interactive_queue_selected_key(
                        &queue,
                        &current_queue,
                        active_queue_target.value().as_deref(),
                    );
                    let scroll_state = scroll_state.clone();
                    let scroll_retry = scroll_retry.clone();
                    let last_auto_scroll_key = last_auto_scroll_key.clone();
                    move || {
                        Text("Interactive Queue", Modifier::empty(), eyebrow_style(theme));
                        BoxWithConstraints(Modifier::empty().fill_max_width(), {
                            let fields = fields.clone();
                            let status = status.clone();
                            let telegram_post_link = telegram_post_link.clone();
                            let ui_preferences = ui_preferences.clone();
                            let pending_action = pending_action.clone();
                            let action_request_counter = action_request_counter.clone();
                            let busy_action = busy_action.clone();
                            let active_queue_target = active_queue_target.clone();
                            let row_queue = queue.clone();
                            let row_current_queue = current_queue.clone();
                            let selected_key = selected_key.clone();
                            let scroll_state = scroll_state.clone();
                            let scroll_retry = scroll_retry.clone();
                            let last_auto_scroll_key = last_auto_scroll_key.clone();
                            move |scope| {
                                let viewport_width = scope.max_width().0;
                                let selected_index = selected_key
                                    .as_ref()
                                    .and_then(|key| row_queue.iter().position(|item| item == key));
                                let selected_key_for_effect = selected_key.clone();
                                let max_scroll_probe = scroll_state.max_value();
                                let scroll_effect_key = (
                                    selected_key_for_effect.clone(),
                                    (viewport_width * 10.0).round() as i32,
                                    (max_scroll_probe * 10.0).round() as i32,
                                    scroll_retry.value(),
                                );
                                cranpose_core::LaunchedEffect!(scroll_effect_key, {
                                    let scroll_state = scroll_state.clone();
                                    let scroll_retry = scroll_retry.clone();
                                    let last_auto_scroll_key = last_auto_scroll_key.clone();
                                    let selected_key = selected_key_for_effect.clone();
                                    move |scope| {
                                        let Some(selected_key) = selected_key.clone() else {
                                            last_auto_scroll_key.set(None);
                                            scroll_retry.set(0);
                                            return;
                                        };
                                        if !interactive_queue_should_auto_scroll(
                                            Some(&selected_key),
                                            last_auto_scroll_key.value().as_deref(),
                                            scroll_retry.value(),
                                        ) {
                                            return;
                                        }
                                        let Some(index) = selected_index else {
                                            return;
                                        };
                                        let current_scroll = scroll_state.value_non_reactive();
                                        let max_scroll = scroll_state.max_value();
                                        if max_scroll <= 0.0 {
                                            schedule_interactive_queue_scroll_retry(
                                                scope,
                                                scroll_retry.clone(),
                                            );
                                            return;
                                        }
                                        let target = interactive_queue_scroll_target(
                                            index,
                                            viewport_width,
                                            current_scroll,
                                        );
                                        let clamped_target = target.min(max_scroll).max(0.0);
                                        scroll_retry.set(0);
                                        last_auto_scroll_key.set(Some(selected_key));
                                        if (clamped_target - current_scroll).abs() > 0.5 {
                                            scroll_state.scroll_to(clamped_target);
                                        }
                                    }
                                });
                                Row(
                                    Modifier::empty()
                                        .fill_max_width()
                                        .height(58.0)
                                        .clip_to_bounds()
                                        .horizontal_scroll(scroll_state.clone(), false),
                                    RowSpec::default().horizontal_arrangement(
                                        LinearArrangement::spaced_by(INTERACTIVE_QUEUE_CHIP_GAP),
                                    ),
                                    {
                                        let fields = fields.clone();
                                        let status = status.clone();
                                        let telegram_post_link = telegram_post_link.clone();
                                        let ui_preferences = ui_preferences.clone();
                                        let pending_action = pending_action.clone();
                                        let action_request_counter = action_request_counter.clone();
                                        let busy_action = busy_action.clone();
                                        let active_queue_target = active_queue_target.clone();
                                        let row_queue = row_queue.clone();
                                        let row_current_queue = row_current_queue.clone();
                                        move || {
                                            for item_key in &row_queue {
                                                InteractiveQueueChip(
                                                    item_key.clone(),
                                                    row_current_queue.contains(item_key),
                                                    fields.clone(),
                                                    status.clone(),
                                                    telegram_post_link.clone(),
                                                    ui_preferences.clone(),
                                                    pending_action.clone(),
                                                    action_request_counter.clone(),
                                                    busy_action.clone(),
                                                    active_queue_target.clone(),
                                                    theme,
                                                );
                                            }
                                        }
                                    },
                                );
                            }
                        });
                        QueueCurrentRow(
                            active_queue_target.value(),
                            fields.clone(),
                            status.clone(),
                            ui_preferences.clone(),
                            theme,
                        );
                    }
                },
            );
        }
    });
}

#[composable]
fn QueueCurrentRow(
    active_key: Option<String>,
    fields: EditorFields,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let Some(active_key) = active_key else {
        return;
    };
    let Some((field, FieldQueueCommand::Edit)) = parse_field_queue_key(&active_key) else {
        return;
    };

    cranpose_core::with_key(&active_key, || {
        Column(
            Modifier::empty().fill_max_width(),
            ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(7.0)),
            {
                let fields = fields.clone();
                let status = status.clone();
                let ui_preferences = ui_preferences.clone();
                move || {
                    Text(
                        "Current",
                        Modifier::empty(),
                        queue_current_label_style(theme),
                    );
                    QueueCurrentEditorField(
                        field,
                        fields.clone(),
                        status.clone(),
                        ui_preferences.clone(),
                        theme,
                    );
                }
            },
        );
    });
}

#[composable]
fn QueueCurrentEditorField(
    field: EditorFieldId,
    fields: EditorFields,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let state = field_state(&fields, field);
    let saved_text = state.text();
    match field {
        EditorFieldId::KotlinCode | EditorFieldId::RustCode => labeled_code_field(
            field.label(),
            field.field_id(),
            state,
            saved_text,
            6,
            14,
            status,
            ui_preferences,
            false,
            theme,
        ),
        EditorFieldId::ProblemTldr | EditorFieldId::Intuition | EditorFieldId::Approach => {
            labeled_field(
                field.label(),
                field.field_id(),
                state,
                saved_text,
                3,
                8,
                status,
                ui_preferences,
                false,
                theme,
                true,
            );
        }
        EditorFieldId::TimeComplexity | EditorFieldId::SpaceComplexity => labeled_field(
            field.label(),
            field.field_id(),
            state,
            saved_text,
            2,
            4,
            status,
            ui_preferences,
            false,
            theme,
            true,
        ),
        _ => labeled_field(
            field.label(),
            field.field_id(),
            state,
            saved_text,
            1,
            1,
            status,
            ui_preferences,
            false,
            theme,
            true,
        ),
    }
}

#[composable]
fn InteractiveQueueChip(
    item_key: String,
    done: bool,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
) {
    let action = ActionButtonId::from_count_key(&item_key);
    let long_action = action.and_then(ActionButtonId::long_action);
    let action_busy = busy_action.value();
    let is_busy = long_action.is_some() && action_busy == long_action;
    let disabled = long_action.is_some() && action_busy.is_some();
    let busy_pulse = if is_busy { busy_pulse() } else { 0.0 };
    let invokes_button = queue_item_invokes_button(&item_key);
    let background =
        interactive_queue_surface(theme, done, disabled, is_busy, busy_pulse, invokes_button);
    Button(
        glass_button_modifier(
            Modifier::empty()
                .width(INTERACTIVE_QUEUE_CHIP_WIDTH)
                .height(48.0),
            theme,
            !disabled,
            done || is_busy,
            background,
            10.0,
        )
        .padding_symmetric(10.0, 8.0),
        {
            let item_key = item_key.clone();
            move || {
                if disabled {
                    return;
                }
                handle_interactive_queue_press(
                    &item_key,
                    fields.clone(),
                    status.clone(),
                    telegram_post_link.clone(),
                    ui_preferences.clone(),
                    pending_action.clone(),
                    action_request_counter.clone(),
                    busy_action.clone(),
                    active_queue_target.clone(),
                    theme,
                );
            }
        },
        move || {
            interactive_queue_content(
                interactive_queue_icon(&item_key),
                interactive_queue_label(&item_key, done, is_busy),
                interactive_queue_text_style(theme, done, disabled, is_busy, busy_pulse),
                theme,
                is_busy,
            );
        },
    );
}

#[composable]
fn interactive_queue_content(
    icon: UiIcon,
    label: String,
    style: TextStyle,
    theme: ThemeMode,
    busy: bool,
) {
    let icon_size = 24.0;
    let label = if busy { format!("{label}...") } else { label };
    Row(
        icon_overlay_modifier(
            Modifier::empty().fill_max_width(),
            icon,
            icon_size,
            0.0,
            theme,
            busy,
        ),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(5.0)),
        move || {
            Spacer(Size::new(icon_size, 0.0));
            BasicText(
                label.clone(),
                Modifier::empty().weight(1.0),
                style.clone(),
                TextOverflow::Ellipsis,
                false,
                1,
                1,
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
    BoxWithConstraints(Modifier::empty().fill_max_width(), {
        let fields = fields.clone();
        let status = status.clone();
        let telegram_post_link = telegram_post_link.clone();
        let ui_preferences = ui_preferences.clone();
        let pending_action = pending_action.clone();
        let action_request_counter = action_request_counter.clone();
        let busy_action = busy_action.clone();
        move |scope| {
            let width = scope.max_width().0;
            let columns = if width >= 820.0 {
                5
            } else if width >= 640.0 {
                4
            } else if width >= 480.0 {
                3
            } else {
                2
            };
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
                    let ordered_actions = ordered_actions.clone();
                    move || {
                        for row in ordered_actions.chunks(columns) {
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
    });
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
        action.icon(),
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
        glass_button_modifier(
            Modifier::empty().fill_max_width(),
            theme,
            !disabled,
            is_busy,
            background,
            14.0,
        )
        .height(64.0)
        .padding_symmetric(14.0, 16.0),
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
                action.icon(),
                action.label().to_string(),
                count,
                style.clone(),
                theme,
                is_busy,
                true,
            );
        },
    );
}

fn handle_interactive_queue_press(
    item_key: &str,
    fields: EditorFields,
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
) {
    if let Some(action) = ActionButtonId::from_count_key(item_key) {
        record_button_press(ui_preferences, item_key);
        handle_action_button(
            action,
            fields,
            status,
            telegram_post_link,
            pending_action,
            action_request_counter,
            busy_action,
        );
        return;
    }

    if item_key == "theme.toggle" {
        record_button_press(ui_preferences.clone(), item_key);
        set_theme_preference(ui_preferences, theme.toggled(), status);
        return;
    }

    if let Some((field, command)) = parse_field_queue_key(item_key) {
        match command {
            FieldQueueCommand::Edit => {
                active_queue_target.set(Some(field.component_key()));
                status.set(format!("Current queue row: {}.", field.label()));
            }
            FieldQueueCommand::Paste => {
                record_button_press(ui_preferences, item_key);
                paste_text_from_clipboard(field_state(&fields, field), status, field.label());
            }
            FieldQueueCommand::Clear => {
                record_button_press(ui_preferences, item_key);
                clear_field(field_state(&fields, field), status, field.label());
            }
        }
        return;
    }

    active_queue_target.set(Some(item_key.to_string()));
    status.set(format!("Current queued target: {item_key}."));
}

fn interactive_queue_label(item_key: &str, done: bool, busy: bool) -> String {
    let label = if let Some(action) = ActionButtonId::from_count_key(item_key) {
        action.label().to_string()
    } else if item_key == "theme.toggle" {
        "Toggle Theme".to_string()
    } else if let Some((field, command)) = parse_field_queue_key(item_key) {
        match command {
            FieldQueueCommand::Edit => format!("Edit {}", field.label()),
            FieldQueueCommand::Paste => format!("Paste {}", field.label()),
            FieldQueueCommand::Clear => format!("Clear {}", field.label()),
        }
    } else {
        item_key.to_string()
    };

    if done && !busy {
        format!("Done: {label}")
    } else {
        label
    }
}

fn queue_item_invokes_button(item_key: &str) -> bool {
    ActionButtonId::from_count_key(item_key).is_some()
        || item_key == "theme.toggle"
        || parse_field_queue_key(item_key)
            .is_some_and(|(_, command)| command != FieldQueueCommand::Edit)
}

fn interactive_queue_icon(item_key: &str) -> UiIcon {
    if let Some(action) = ActionButtonId::from_count_key(item_key) {
        return action.icon();
    }
    if item_key == "theme.toggle" {
        return UiIcon::Theme;
    }
    if let Some((field, command)) = parse_field_queue_key(item_key) {
        return match command {
            FieldQueueCommand::Edit => field.icon(),
            FieldQueueCommand::Paste => UiIcon::Paste,
            FieldQueueCommand::Clear => UiIcon::Clear,
        };
    }
    UiIcon::Generic
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldQueueCommand {
    Edit,
    Paste,
    Clear,
}

fn parse_field_queue_key(item_key: &str) -> Option<(EditorFieldId, FieldQueueCommand)> {
    let field_key = item_key.strip_prefix("field.")?;
    if let Some(field_id) = field_key.strip_suffix(".paste") {
        return EditorFieldId::from_field_id(field_id)
            .map(|field| (field, FieldQueueCommand::Paste));
    }
    if let Some(field_id) = field_key.strip_suffix(".clear") {
        return EditorFieldId::from_field_id(field_id)
            .map(|field| (field, FieldQueueCommand::Clear));
    }
    EditorFieldId::from_field_id(field_key).map(|field| (field, FieldQueueCommand::Edit))
}

fn field_state(fields: &EditorFields, field: EditorFieldId) -> TextFieldState {
    match field {
        EditorFieldId::Date => fields.date.clone(),
        EditorFieldId::ProblemTitle => fields.problem_title.clone(),
        EditorFieldId::ProblemUrl => fields.problem_url.clone(),
        EditorFieldId::Difficulty => fields.difficulty.clone(),
        EditorFieldId::BlogPostUrl => fields.blog_post_url.clone(),
        EditorFieldId::SubstackUrl => fields.substack_url.clone(),
        EditorFieldId::YoutubeUrl => fields.youtube_url.clone(),
        EditorFieldId::ReferenceUrl => fields.reference_url.clone(),
        EditorFieldId::TelegramText => fields.telegram_text.clone(),
        EditorFieldId::ProblemTldr => fields.problem_tldr.clone(),
        EditorFieldId::Intuition => fields.intuition.clone(),
        EditorFieldId::Approach => fields.approach.clone(),
        EditorFieldId::TimeComplexity => fields.time_complexity.clone(),
        EditorFieldId::SpaceComplexity => fields.space_complexity.clone(),
        EditorFieldId::KotlinRuntimeMs => fields.kotlin_runtime_ms.clone(),
        EditorFieldId::KotlinCode => fields.kotlin_code.clone(),
        EditorFieldId::RustRuntimeMs => fields.rust_runtime_ms.clone(),
        EditorFieldId::RustCode => fields.rust_code.clone(),
    }
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
    fn from_count_key(key: &str) -> Option<Self> {
        ACTION_BUTTONS
            .iter()
            .copied()
            .find(|action| action.count_key() == key)
    }

    fn icon(self) -> UiIcon {
        match self {
            Self::RefreshRasterPreview => UiIcon::Refresh,
            Self::RefreshCranposePreview => UiIcon::RefreshAlt,
            Self::CopyLeetcode => UiIcon::Code,
            Self::CopyYoutube => UiIcon::Youtube,
            Self::CopyBlog => UiIcon::Document,
            Self::CopyTelegram => UiIcon::Telegram,
            Self::CopyTitle => UiIcon::Title,
            Self::CopySubtitle => UiIcon::Subtitle,
            Self::CopyRichText => UiIcon::RichText,
            Self::SaveRasterWebp => UiIcon::Save,
            Self::SaveCranposeWebp => UiIcon::CranposeSave,
            Self::PublishBlog => UiIcon::Blog,
            Self::PostTelegram => UiIcon::Telegram,
            Self::PostTelegramComment => UiIcon::Comment,
        }
    }

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

fn interactive_queue_next_key(queue: &[String], done_queue: &[String]) -> Option<String> {
    queue
        .iter()
        .filter(|key| !done_queue.contains(key))
        .find(|key| next_work_item_from_queue_key(key).is_some())
        .cloned()
        .or_else(|| {
            queue
                .iter()
                .find(|key| next_work_item_from_queue_key(key).is_some())
                .cloned()
        })
}

fn interactive_queue_selected_key(
    queue: &[String],
    done_queue: &[String],
    active_queue_target: Option<&str>,
) -> Option<String> {
    active_queue_target
        .filter(|target| {
            queue.iter().any(|item| item.as_str() == *target)
                && !done_queue.iter().any(|item| item.as_str() == *target)
        })
        .map(str::to_string)
        .or_else(|| interactive_queue_next_key(queue, done_queue))
}

fn interactive_queue_scroll_target(index: usize, viewport_width: f32, current_scroll: f32) -> f32 {
    let item_left = index as f32 * (INTERACTIVE_QUEUE_CHIP_WIDTH + INTERACTIVE_QUEUE_CHIP_GAP);
    let item_center = item_left + INTERACTIVE_QUEUE_CHIP_WIDTH * 0.5;
    let visible_center = item_center - current_scroll;
    if visible_center > viewport_width * 0.5 {
        (item_center - viewport_width * 0.42).max(0.0)
    } else if item_left < current_scroll {
        item_left.max(0.0)
    } else {
        current_scroll.max(0.0)
    }
}

fn interactive_queue_should_auto_scroll(
    selected_key: Option<&str>,
    last_auto_scroll_key: Option<&str>,
    retry: u64,
) -> bool {
    match selected_key {
        Some(key) => last_auto_scroll_key != Some(key) || retry > 0,
        None => false,
    }
}

fn bump_interactive_queue_scroll_retry(scroll_retry: MutableState<u64>) {
    scroll_retry.update(|value| {
        *value = value.saturating_add(1).min(16);
        *value
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn schedule_interactive_queue_scroll_retry(
    scope: cranpose_core::LaunchedEffectScope,
    scroll_retry: MutableState<u64>,
) {
    if scroll_retry.value() >= 16 {
        return;
    }
    scope.launch_background(
        |token| async move {
            std::thread::sleep(Duration::from_millis(35));
            token.is_active()
        },
        move |active| {
            if active {
                bump_interactive_queue_scroll_retry(scroll_retry);
            }
        },
    );
}

#[cfg(target_arch = "wasm32")]
fn schedule_interactive_queue_scroll_retry(
    scope: cranpose_core::LaunchedEffectScope,
    scroll_retry: MutableState<u64>,
) {
    if scroll_retry.value() >= 16 {
        return;
    }
    scope.post_ui(move || {
        bump_interactive_queue_scroll_retry(scroll_retry);
    });
}

fn next_work_item_from_queue_key(item_key: &str) -> Option<NextWorkItem> {
    if let Some(action) = ActionButtonId::from_count_key(item_key) {
        return Some(NextWorkItem::Action(action));
    }
    parse_field_queue_key(item_key).map(|(field, _)| NextWorkItem::Field(field))
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

impl WorkStage {
    fn icon(self) -> UiIcon {
        match self {
            Self::Prepare => UiIcon::StagePrepare,
            Self::Write => UiIcon::StageWrite,
            Self::Code => UiIcon::StageCode,
            Self::Review => UiIcon::StageReview,
            Self::Ship => UiIcon::StageShip,
        }
    }

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
    _layout_preferences: UiPreferences,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
    compact: bool,
) {
    section_card(theme, {
        let fields = fields.clone();
        let status = status.clone();
        let saved_draft = saved_draft.clone();
        let ui_preferences = ui_preferences.clone();
        let active_queue_target = active_queue_target.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let saved_draft = saved_draft.clone();
                    let ui_preferences = ui_preferences.clone();
                    let active_queue_target = active_queue_target.clone();
                    move || {
                        SectionHeader("Problem Meta", UiIcon::Document, theme);
                        if compact {
                            MetaFieldColumn(
                                vec![
                                    EditorFieldId::ProblemTitle,
                                    EditorFieldId::YoutubeUrl,
                                    EditorFieldId::ProblemUrl,
                                    EditorFieldId::TelegramText,
                                    EditorFieldId::ReferenceUrl,
                                    EditorFieldId::SubstackUrl,
                                    EditorFieldId::Date,
                                    EditorFieldId::Difficulty,
                                    EditorFieldId::BlogPostUrl,
                                ],
                                fields.clone(),
                                saved_draft.clone(),
                                status.clone(),
                                ui_preferences.clone(),
                                active_queue_target.clone(),
                                theme,
                                false,
                            );
                        } else {
                            Row(
                                Modifier::empty().fill_max_width(),
                                RowSpec::default()
                                    .horizontal_arrangement(LinearArrangement::spaced_by(18.0)),
                                {
                                    let fields = fields.clone();
                                    let saved_draft = saved_draft.clone();
                                    let status = status.clone();
                                    let ui_preferences = ui_preferences.clone();
                                    let active_queue_target = active_queue_target.clone();
                                    move || {
                                        MetaFieldColumn(
                                            vec![
                                                EditorFieldId::ProblemTitle,
                                                EditorFieldId::YoutubeUrl,
                                                EditorFieldId::ProblemUrl,
                                                EditorFieldId::TelegramText,
                                                EditorFieldId::ReferenceUrl,
                                            ],
                                            fields.clone(),
                                            saved_draft.clone(),
                                            status.clone(),
                                            ui_preferences.clone(),
                                            active_queue_target.clone(),
                                            theme,
                                            true,
                                        );
                                        MetaFieldColumn(
                                            vec![
                                                EditorFieldId::SubstackUrl,
                                                EditorFieldId::Date,
                                                EditorFieldId::Difficulty,
                                                EditorFieldId::BlogPostUrl,
                                            ],
                                            fields.clone(),
                                            saved_draft.clone(),
                                            status.clone(),
                                            ui_preferences.clone(),
                                            active_queue_target.clone(),
                                            theme,
                                            true,
                                        );
                                    }
                                },
                            );
                        }
                    }
                },
            );
        }
    });
}

#[composable]
fn MetaFieldColumn(
    field_ids: Vec<EditorFieldId>,
    fields: EditorFields,
    saved_draft: PostDraft,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
    weighted: bool,
) {
    let modifier = if weighted {
        Modifier::empty().weight(1.0)
    } else {
        Modifier::empty().fill_max_width()
    };
    Column(
        modifier,
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(12.0)),
        move || {
            ForEach(&field_ids, {
                let fields = fields.clone();
                let saved_draft = saved_draft.clone();
                let status = status.clone();
                let ui_preferences = ui_preferences.clone();
                let active_queue_target = active_queue_target.clone();
                move |field| {
                    EditorField(
                        *field,
                        fields.clone(),
                        saved_draft.clone(),
                        status.clone(),
                        ui_preferences.clone(),
                        active_queue_target.clone(),
                        theme,
                    );
                }
            });
        },
    );
}

#[composable]
fn WriteupCard(
    fields: EditorFields,
    status: MutableState<String>,
    saved_draft: PostDraft,
    ui_preferences: MutableState<UiPreferences>,
    layout_preferences: UiPreferences,
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
) {
    section_card(theme, {
        let fields = fields.clone();
        let status = status.clone();
        let saved_draft = saved_draft.clone();
        let ui_preferences = ui_preferences.clone();
        let layout_preferences = layout_preferences.clone();
        let active_queue_target = active_queue_target.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let saved_draft = saved_draft.clone();
                    let ui_preferences = ui_preferences.clone();
                    let active_queue_target = active_queue_target.clone();
                    let ordered_fields = ordered_fields(&WRITEUP_FIELDS, &layout_preferences);
                    move || {
                        Text("Writeup", Modifier::empty(), heading_style(28.0, theme));
                        ForEach(&ordered_fields, {
                            let fields = fields.clone();
                            let saved_draft = saved_draft.clone();
                            let status = status.clone();
                            let ui_preferences = ui_preferences.clone();
                            let active_queue_target = active_queue_target.clone();
                            move |field| {
                                EditorField(
                                    *field,
                                    fields.clone(),
                                    saved_draft.clone(),
                                    status.clone(),
                                    ui_preferences.clone(),
                                    active_queue_target.clone(),
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
    active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
) {
    section_card(theme, {
        let fields = fields.clone();
        let status = status.clone();
        let saved_draft = saved_draft.clone();
        let ui_preferences = ui_preferences.clone();
        let layout_preferences = layout_preferences.clone();
        let active_queue_target = active_queue_target.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let status = status.clone();
                    let saved_draft = saved_draft.clone();
                    let ui_preferences = ui_preferences.clone();
                    let active_queue_target = active_queue_target.clone();
                    let ordered_fields = ordered_fields(&CODE_FIELDS, &layout_preferences);
                    move || {
                        Text("Code Blocks", Modifier::empty(), heading_style(28.0, theme));
                        ForEach(&ordered_fields, {
                            let fields = fields.clone();
                            let saved_draft = saved_draft.clone();
                            let status = status.clone();
                            let ui_preferences = ui_preferences.clone();
                            let active_queue_target = active_queue_target.clone();
                            move |field| {
                                EditorField(
                                    *field,
                                    fields.clone(),
                                    saved_draft.clone(),
                                    status.clone(),
                                    ui_preferences.clone(),
                                    active_queue_target.clone(),
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
    _active_queue_target: MutableState<Option<String>>,
    theme: ThemeMode,
) {
    let highlighted = false;
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
            highlighted,
            theme,
            true,
        ),
        EditorFieldId::ProblemTitle => labeled_field(
            field.label(),
            field.field_id(),
            fields.problem_title.clone(),
            saved_draft.problem_title.clone(),
            1,
            1,
            status,
            ui_preferences,
            highlighted,
            theme,
            true,
        ),
        EditorFieldId::ProblemUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.problem_url.clone(),
            saved_draft.problem_url.clone(),
            1,
            1,
            status,
            ui_preferences,
            highlighted,
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
            highlighted,
            theme,
            true,
        ),
        EditorFieldId::BlogPostUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.blog_post_url.clone(),
            saved_draft.blog_post_url.clone(),
            1,
            1,
            status,
            ui_preferences,
            highlighted,
            theme,
            true,
        ),
        EditorFieldId::SubstackUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.substack_url.clone(),
            saved_draft.substack_url.clone(),
            1,
            1,
            status,
            ui_preferences,
            highlighted,
            theme,
            true,
        ),
        EditorFieldId::YoutubeUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.youtube_url.clone(),
            saved_draft.youtube_url.clone(),
            1,
            1,
            status,
            ui_preferences,
            highlighted,
            theme,
            true,
        ),
        EditorFieldId::ReferenceUrl => labeled_field(
            field.label(),
            field.field_id(),
            fields.reference_url.clone(),
            saved_draft.reference_url.clone(),
            1,
            1,
            status,
            ui_preferences,
            highlighted,
            theme,
            true,
        ),
        EditorFieldId::TelegramText => labeled_field(
            field.label(),
            field.field_id(),
            fields.telegram_text.clone(),
            saved_draft.telegram_text.clone(),
            1,
            2,
            status,
            ui_preferences,
            highlighted,
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
            highlighted,
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
            highlighted,
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
            highlighted,
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
            highlighted,
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
            highlighted,
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
            highlighted,
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
            highlighted,
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
            highlighted,
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
            highlighted,
            theme,
        ),
    }
}

impl EditorFieldId {
    fn from_field_id(field_id: &str) -> Option<Self> {
        WORKFLOW_FIELDS
            .iter()
            .copied()
            .find(|field| field.field_id() == field_id)
    }

    fn icon(self) -> UiIcon {
        UiIcon::for_field_id(self.field_id())
    }

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
fn ReferenceIcon(icon: UiIcon, size: Size, theme: ThemeMode, active: bool) {
    ComposeBox(
        Modifier::empty().size(size).draw_behind(move |scope| {
            let size = scope.size();
            draw_ui_icon(
                scope,
                icon,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                theme,
                if active { 1.0 } else { 0.96 },
            );
        }),
        BoxSpec::default(),
        || {},
    );
}

fn app_background_bitmap() -> Option<ImageBitmap> {
    static BITMAP: OnceLock<Option<ImageBitmap>> = OnceLock::new();
    BITMAP
        .get_or_init(|| bitmap_from_png(crate::assets::APP_BACKGROUND_PNG))
        .clone()
}

fn hero_bitmap(stage: WorkStage) -> Option<ImageBitmap> {
    static BITMAPS: OnceLock<[Option<ImageBitmap>; 5]> = OnceLock::new();
    BITMAPS.get_or_init(|| {
        [
            bitmap_from_png(crate::assets::HERO_PREPARE_PNG),
            bitmap_from_png(crate::assets::HERO_WRITE_PNG),
            bitmap_from_png(crate::assets::HERO_CODE_PNG),
            bitmap_from_png(crate::assets::HERO_REVIEW_PNG),
            bitmap_from_png(crate::assets::HERO_SHIP_PNG),
        ]
    })[stage.sort_index() as usize]
        .clone()
}

fn app_logo_bitmap() -> Option<ImageBitmap> {
    static BITMAP: OnceLock<Option<ImageBitmap>> = OnceLock::new();
    BITMAP
        .get_or_init(|| bitmap_from_png(crate::assets::APP_LOGO_PNG))
        .clone()
}

fn ui_icons_bitmap() -> Option<ImageBitmap> {
    static BITMAP: OnceLock<Option<ImageBitmap>> = OnceLock::new();
    BITMAP
        .get_or_init(|| bitmap_from_png(crate::assets::UI_ICONS_PNG))
        .clone()
}

fn ui_icons_bitmap_24() -> Option<ImageBitmap> {
    static BITMAP: OnceLock<Option<ImageBitmap>> = OnceLock::new();
    BITMAP
        .get_or_init(|| bitmap_from_png(crate::assets::UI_ICONS_24_PNG))
        .clone()
}

fn ui_icons_bitmap_44() -> Option<ImageBitmap> {
    static BITMAP: OnceLock<Option<ImageBitmap>> = OnceLock::new();
    BITMAP
        .get_or_init(|| bitmap_from_png(crate::assets::UI_ICONS_44_PNG))
        .clone()
}

fn ui_icons_bitmap_58() -> Option<ImageBitmap> {
    static BITMAP: OnceLock<Option<ImageBitmap>> = OnceLock::new();
    BITMAP
        .get_or_init(|| bitmap_from_png(crate::assets::UI_ICONS_58_PNG))
        .clone()
}

fn bitmap_from_png(bytes: &[u8]) -> Option<ImageBitmap> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    ImageBitmap::from_rgba8(image.width(), image.height(), image.into_raw()).ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SharpBitmapKey {
    image_id: u64,
    src_x: u32,
    src_y: u32,
    src_width: u32,
    src_height: u32,
    target_width: u32,
    target_height: u32,
}

const SHARP_IMAGE_MAX_PIXELS: u32 = 4_000_000;

fn draw_sharp_image_src<S: DrawScope + ?Sized>(
    scope: &mut S,
    bitmap: ImageBitmap,
    src: Rect,
    dst: Rect,
    alpha: f32,
    color_filter: Option<ColorFilter>,
) {
    let dst = snap_rect(dst);
    if let Some(sharp_bitmap) = sharp_bitmap_for_draw(&bitmap, src, dst) {
        let sharp_src = Rect {
            x: 0.0,
            y: 0.0,
            width: sharp_bitmap.width() as f32,
            height: sharp_bitmap.height() as f32,
        };
        scope.draw_image_src(sharp_bitmap, sharp_src, dst, alpha, color_filter);
    } else {
        scope.draw_image_src(bitmap, src, dst, alpha, color_filter);
    }
}

fn sharp_bitmap_for_draw(bitmap: &ImageBitmap, src: Rect, dst: Rect) -> Option<ImageBitmap> {
    let (src_x, src_y, src_width, src_height) = source_rect_pixels(bitmap, src)?;
    let (target_width, target_height) = target_image_pixels(dst)?;
    if target_width.saturating_mul(target_height) > SHARP_IMAGE_MAX_PIXELS {
        return None;
    }
    let source_is_full =
        src_x == 0 && src_y == 0 && src_width == bitmap.width() && src_height == bitmap.height();
    if source_is_full && src_width == target_width && src_height == target_height {
        return Some(bitmap.clone());
    }

    static CACHE: OnceLock<Mutex<HashMap<SharpBitmapKey, ImageBitmap>>> = OnceLock::new();
    let key = SharpBitmapKey {
        image_id: bitmap.id(),
        src_x,
        src_y,
        src_width,
        src_height,
        target_width,
        target_height,
    };
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().ok()?.get(&key).cloned() {
        return Some(cached);
    }

    let source = RgbaImage::from_raw(bitmap.width(), bitmap.height(), bitmap.pixels().to_vec())?;
    let cropped =
        image::imageops::crop_imm(&source, src_x, src_y, src_width, src_height).to_image();
    let resized = if src_width == target_width && src_height == target_height {
        cropped
    } else {
        image::imageops::resize(&cropped, target_width, target_height, FilterType::Lanczos3)
    };
    let sharp_bitmap =
        ImageBitmap::from_rgba8(resized.width(), resized.height(), resized.into_raw()).ok()?;
    cache.lock().ok()?.insert(key, sharp_bitmap.clone());
    Some(sharp_bitmap)
}

fn source_rect_pixels(bitmap: &ImageBitmap, src: Rect) -> Option<(u32, u32, u32, u32)> {
    let max_width = bitmap.width() as f32;
    let max_height = bitmap.height() as f32;
    let x = src.x.round().clamp(0.0, max_width) as u32;
    let y = src.y.round().clamp(0.0, max_height) as u32;
    let right = (src.x + src.width).round().clamp(0.0, max_width) as u32;
    let bottom = (src.y + src.height).round().clamp(0.0, max_height) as u32;
    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
}

fn target_image_pixels(dst: Rect) -> Option<(u32, u32)> {
    if dst.width <= 0.0 || dst.height <= 0.0 {
        return None;
    }
    let density = cranpose::current_density();
    let density = if density.is_finite() && density > 0.0 {
        density
    } else {
        1.0
    };
    let width = (dst.width * density).round().max(1.0) as u32;
    let height = (dst.height * density).round().max(1.0) as u32;
    Some((width, height))
}

fn draw_app_background<S: DrawScope + ?Sized>(scope: &mut S) {
    let size = scope.size();
    scope.draw_rect(Brush::linear_gradient_range(
        vec![
            Color::from_rgb_u8(218, 244, 255),
            Color::from_rgb_u8(236, 251, 255),
            Color::from_rgb_u8(188, 242, 249),
            Color::from_rgb_u8(120, 226, 209),
        ],
        Point::new(0.0, 0.0),
        Point::new(size.width, size.height),
    ));

    if let Some(bitmap) = app_background_bitmap() {
        draw_stretchable_app_background(scope, bitmap, size, app_background_slices());
    }
}

#[composable]
fn BottomListGapMask() {
    ComposeBox(
        Modifier::empty()
            .fill_max_width()
            .height(APP_BOTTOM_LIST_GAP)
            .draw_behind(|scope| draw_bottom_list_gap_mask(scope)),
        BoxSpec::default(),
        || {},
    );
}

fn draw_bottom_list_gap_mask<S: DrawScope + ?Sized>(scope: &mut S) {
    let size = scope.size();
    let horizontal_padding = if size.width < 700.0 {
        18.0
    } else if size.width < 1120.0 {
        24.0
    } else {
        34.0
    };
    let y = (size.height - APP_BOTTOM_LIST_GAP).max(0.0);
    let content_width = (size.width - horizontal_padding * 2.0).max(0.0);
    scope.draw_rect_at(
        Rect {
            x: horizontal_padding,
            y,
            width: content_width,
            height: APP_BOTTOM_LIST_GAP,
        },
        Brush::linear_gradient_range(
            vec![
                Color::from_rgba_u8(209, 247, 252, 255),
                Color::from_rgba_u8(153, 236, 231, 255),
                Color::from_rgba_u8(71, 218, 218, 255),
            ],
            Point::new(0.0, y),
            Point::new(size.width, size.height),
        ),
    );
    scope.draw_rect_at(
        Rect {
            x: horizontal_padding,
            y,
            width: content_width,
            height: 2.0,
        },
        Brush::horizontal_gradient(
            vec![
                Color::TRANSPARENT,
                Color::from_rgba_u8(255, 255, 255, 172),
                Color::TRANSPARENT,
            ],
            0.0,
            size.width,
        ),
    );
}

#[derive(Clone, Copy)]
struct NineSliceInsets {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

fn app_background_slices() -> NineSliceInsets {
    NineSliceInsets {
        left: 220.0,
        top: 180.0,
        right: 220.0,
        bottom: 340.0,
    }
}

fn draw_stretchable_app_background<S: DrawScope + ?Sized>(
    scope: &mut S,
    bitmap: ImageBitmap,
    size: Size,
    insets: NineSliceInsets,
) {
    let source_width = bitmap.width() as f32;
    let source_height = bitmap.height() as f32;
    if source_width <= 0.0 || source_height <= 0.0 || size.width <= 0.0 || size.height <= 0.0 {
        return;
    }

    let full_src = Rect {
        x: 0.0,
        y: 0.0,
        width: source_width,
        height: source_height,
    };
    let full_dst = Rect {
        x: 0.0,
        y: 0.0,
        width: size.width,
        height: size.height,
    };

    if size.width <= source_width && size.height <= source_height {
        draw_sharp_image_src(scope, bitmap, full_src, full_dst, DEFAULT_ALPHA, None);
    } else {
        draw_nine_slice_bitmap(scope, bitmap, size, insets);
    }
}

fn draw_nine_slice_bitmap<S: DrawScope + ?Sized>(
    scope: &mut S,
    bitmap: ImageBitmap,
    size: Size,
    insets: NineSliceInsets,
) {
    let source_width = bitmap.width() as f32;
    let source_height = bitmap.height() as f32;
    if source_width <= 0.0 || source_height <= 0.0 || size.width <= 0.0 || size.height <= 0.0 {
        return;
    }

    let source_left = insets.left.clamp(0.0, source_width);
    let source_right = insets
        .right
        .clamp(0.0, (source_width - source_left).max(0.0));
    let source_top = insets.top.clamp(0.0, source_height);
    let source_bottom = insets
        .bottom
        .clamp(0.0, (source_height - source_top).max(0.0));

    let horizontal_scale = (size.width / source_width).min(1.0);
    let vertical_scale = (size.height / source_height).min(1.0);
    let dest_left = (source_left * horizontal_scale).round();
    let dest_right = (source_right * horizontal_scale).round();
    let dest_top = (source_top * vertical_scale).round();
    let dest_bottom = (source_bottom * vertical_scale).round();

    let src_x = [0.0, source_left, source_width - source_right, source_width];
    let src_y = [
        0.0,
        source_top,
        source_height - source_bottom,
        source_height,
    ];
    let dst_x = [0.0, dest_left, size.width - dest_right, size.width];
    let dst_y = [0.0, dest_top, size.height - dest_bottom, size.height];

    for row in 0..3 {
        for column in 0..3 {
            let src = Rect {
                x: src_x[column],
                y: src_y[row],
                width: src_x[column + 1] - src_x[column],
                height: src_y[row + 1] - src_y[row],
            };
            let dst = Rect {
                x: dst_x[column],
                y: dst_y[row],
                width: dst_x[column + 1] - dst_x[column],
                height: dst_y[row + 1] - dst_y[row],
            };
            if src.width > 0.0 && src.height > 0.0 && dst.width > 0.0 && dst.height > 0.0 {
                draw_sharp_image_src(scope, bitmap.clone(), src, dst, DEFAULT_ALPHA, None);
            }
        }
    }
}

const ICON_SHEET_COLUMNS: u32 = 8;
const ICON_SHEET_CELL: u32 = 256;

fn draw_ui_icon<S: DrawScope + ?Sized>(
    scope: &mut S,
    icon: UiIcon,
    dst_rect: Rect,
    theme: ThemeMode,
    alpha: f32,
) {
    let requested_size = dst_rect.width.min(dst_rect.height).round().max(1.0);
    let (bitmap, source_cell) = icon_bitmap_for_size(requested_size);
    if let Some(bitmap) = bitmap {
        let dst_rect = Rect {
            x: dst_rect.x,
            y: dst_rect.y,
            width: requested_size,
            height: requested_size,
        };
        draw_sharp_image_src(
            scope,
            bitmap,
            icon.src_rect_for_cell(source_cell as f32),
            snap_rect(dst_rect),
            alpha,
            None,
        );
    } else {
        draw_reference_icon(scope, icon, dst_rect, theme, alpha);
    }
}

fn icon_bitmap_for_size(size: f32) -> (Option<ImageBitmap>, u32) {
    if size <= 30.0 {
        (ui_icons_bitmap_24(), 48)
    } else if size <= 50.0 {
        (ui_icons_bitmap_44(), 88)
    } else if size <= 80.0 {
        (ui_icons_bitmap_58(), 116)
    } else {
        (ui_icons_bitmap(), ICON_SHEET_CELL)
    }
}

fn snap_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x.round(),
        y: rect.y.round(),
        width: rect.width.round(),
        height: rect.height.round(),
    }
}

fn draw_reference_icon<S: DrawScope + ?Sized>(
    scope: &mut S,
    icon: UiIcon,
    dst_rect: Rect,
    _theme: ThemeMode,
    alpha: f32,
) {
    let color = icon.fallback_color().with_alpha(0.9 * alpha);
    let radius = if icon.is_round_icon() {
        dst_rect.width.min(dst_rect.height) * 0.5
    } else {
        dst_rect.width.min(dst_rect.height) * 0.24
    };
    scope.draw_round_rect(
        Brush::linear_gradient_range(
            vec![lighten_color(color, 0.38), color, darken_color(color, 0.18)],
            Point::new(dst_rect.x, dst_rect.y),
            Point::new(dst_rect.x + dst_rect.width, dst_rect.y + dst_rect.height),
        ),
        CornerRadii::uniform(radius),
    );
    scope.draw_rect_at(
        Rect {
            x: dst_rect.x + dst_rect.width * 0.20,
            y: dst_rect.y + dst_rect.height * 0.17,
            width: dst_rect.width * 0.34,
            height: dst_rect.height * 0.10,
        },
        Brush::solid(Color::from_rgba_u8(255, 255, 255, 82)),
    );
}

fn icon_overlay_modifier(
    modifier: Modifier,
    icon: UiIcon,
    icon_size: f32,
    x_offset: f32,
    theme: ThemeMode,
    active: bool,
) -> Modifier {
    modifier.draw_with_content(move |scope| {
        scope.draw_content();
        let size = scope.size();
        draw_ui_icon(
            scope,
            icon,
            Rect {
                x: x_offset,
                y: ((size.height - icon_size) * 0.5).max(0.0),
                width: icon_size,
                height: icon_size,
            },
            theme,
            if active { 1.0 } else { 0.96 },
        );
    })
}

impl UiIcon {
    fn for_field_id(field_id: &str) -> Self {
        match field_id {
            "date" => Self::Date,
            "problem_title" => Self::Document,
            "problem_url" => Self::Leetcode,
            "difficulty" => Self::Difficulty,
            "blog_post_url" => Self::Web,
            "substack_url" => Self::Substack,
            "youtube_url" => Self::Youtube,
            "reference_url" => Self::Web,
            "telegram_text" => Self::Telegram,
            "problem_tldr" => Self::Document,
            "intuition" => Self::RichText,
            "approach" => Self::Document,
            "time_complexity" => Self::Difficulty,
            "space_complexity" => Self::Difficulty,
            "kotlin_runtime_ms" | "kotlin_code" => Self::Code,
            "rust_runtime_ms" | "rust_code" => Self::Code,
            _ => Self::Generic,
        }
    }

    fn src_rect_for_cell(self, cell: f32) -> Rect {
        let index = self.sheet_index();
        Rect {
            x: (index % ICON_SHEET_COLUMNS) as f32 * cell,
            y: (index / ICON_SHEET_COLUMNS) as f32 * cell,
            width: cell,
            height: cell,
        }
    }

    fn sheet_index(self) -> u32 {
        match self {
            Self::AppLogo => 0,
            Self::CranposeSave => 1,
            Self::Save => 2,
            Self::Code => 3,
            Self::Telegram => 4,
            Self::Title => 5,
            Self::Subtitle => 6,
            Self::RichText => 7,
            Self::Youtube => 8,
            Self::Comment => 9,
            Self::Blog => 10,
            Self::Refresh => 11,
            Self::RefreshAlt => 12,
            Self::Document => 14,
            Self::Leetcode => 15,
            Self::Substack => 16,
            Self::Date => 17,
            Self::Difficulty => 18,
            Self::Web => 19,
            Self::StagePrepare => 20,
            Self::StageWrite => 21,
            Self::StageCode => 22,
            Self::StageReview => 23,
            Self::StageShip => 24,
            Self::Theme => 25,
            Self::Paste => 26,
            Self::Clear => 27,
            Self::Generic => 28,
        }
    }

    fn is_round_icon(self) -> bool {
        matches!(
            self,
            Self::Telegram
                | Self::Blog
                | Self::Web
                | Self::Refresh
                | Self::RefreshAlt
                | Self::StagePrepare
                | Self::StageWrite
                | Self::StageCode
                | Self::StageReview
                | Self::StageShip
                | Self::Theme
                | Self::Clear
        )
    }

    fn fallback_color(self) -> Color {
        match self {
            Self::Save | Self::Difficulty => Color::from_rgb_u8(87, 200, 56),
            Self::StagePrepare => Color::from_rgb_u8(42, 139, 224),
            Self::StageWrite => Color::from_rgb_u8(69, 139, 214),
            Self::StageCode => Color::from_rgb_u8(42, 166, 204),
            Self::Youtube | Self::Clear => Color::from_rgb_u8(232, 49, 45),
            Self::Telegram => Color::from_rgb_u8(45, 169, 230),
            Self::Substack => Color::from_rgb_u8(255, 116, 43),
            Self::StageReview | Self::Title => Color::from_rgb_u8(247, 177, 20),
            Self::RefreshAlt => Color::from_rgb_u8(155, 73, 218),
            Self::StageShip => Color::from_rgb_u8(139, 169, 188),
            Self::Comment => Color::from_rgb_u8(109, 205, 58),
            Self::RichText => Color::from_rgb_u8(48, 143, 177),
            Self::CranposeSave | Self::Document | Self::Subtitle => {
                Color::from_rgb_u8(38, 151, 226)
            }
            Self::Leetcode => Color::from_rgb_u8(248, 177, 48),
            Self::Blog | Self::Web => Color::from_rgb_u8(42, 162, 231),
            Self::Refresh => Color::from_rgb_u8(92, 196, 57),
            _ => Color::from_rgb_u8(38, 151, 226),
        }
    }
}

fn button_icon_for_label(label: &str) -> UiIcon {
    match label {
        "Paste" => UiIcon::Paste,
        "Clear" => UiIcon::Clear,
        _ if label.starts_with("Theme:") => UiIcon::Theme,
        _ => UiIcon::Generic,
    }
}

#[composable]
fn AppLogo() {
    ComposeBox(
        Modifier::empty()
            .size(Size {
                width: 76.0,
                height: 76.0,
            })
            .drop_shadow(
                LayerShape::Rounded(RoundedCornerShape::uniform(38.0)),
                |shadow| {
                    shadow.radius = 18.0;
                    shadow.spread = 1.0;
                    shadow.offset = Point::new(0.0, 8.0);
                    shadow.color = Color::from_rgba_u8(34, 127, 194, 112);
                },
            )
            .draw_behind(|scope| {
                let dst = snap_rect(Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 76.0,
                    height: 76.0,
                });
                if let Some(bitmap) = app_logo_bitmap() {
                    draw_sharp_image_src(
                        scope,
                        bitmap.clone(),
                        Rect {
                            x: 0.0,
                            y: 0.0,
                            width: bitmap.width() as f32,
                            height: bitmap.height() as f32,
                        },
                        dst,
                        DEFAULT_ALPHA,
                        None,
                    );
                } else {
                    draw_ui_icon(scope, UiIcon::AppLogo, dst, ThemeMode::Light, 1.0);
                }
            }),
        BoxSpec::default().content_alignment(Alignment::CENTER),
        || {},
    );
}

#[composable]
fn SectionHeader(title: &'static str, icon: UiIcon, theme: ThemeMode) {
    Row(
        icon_overlay_modifier(Modifier::empty(), icon, 24.0, 0.0, theme, false),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(10.0)),
        move || {
            Spacer(Size::new(24.0, 0.0));
            Text(title, Modifier::empty(), heading_style(24.0, theme));
        },
    );
}

#[composable]
fn HeroTile(stage: WorkStage, theme: ThemeMode) {
    ComposeBox(
        glass_panel_modifier(
            Modifier::empty().size(Size {
                width: 184.0,
                height: 164.0,
            }),
            theme,
            18.0,
        )
        .padding(0.0),
        BoxSpec::default().content_alignment(Alignment::CENTER),
        move || {
            if let Some(bitmap) = hero_bitmap(stage) {
                ComposeBox(
                    Modifier::empty()
                        .size(Size {
                            width: 176.0,
                            height: 160.0,
                        })
                        .draw_behind(move |scope| {
                            draw_sharp_image_src(
                                scope,
                                bitmap.clone(),
                                Rect {
                                    x: 0.0,
                                    y: 0.0,
                                    width: bitmap.width() as f32,
                                    height: bitmap.height() as f32,
                                },
                                snap_rect(Rect {
                                    x: 0.0,
                                    y: 0.0,
                                    width: 176.0,
                                    height: 160.0,
                                }),
                                DEFAULT_ALPHA,
                                None,
                            );
                        }),
                    BoxSpec::default(),
                    || {},
                );
            } else {
                ReferenceIcon(stage.icon(), Size::new(112.0, 112.0), theme, true);
            }
        },
    );
}

#[composable]
fn FieldSuggestion(
    field: EditorFieldId,
    active_queue_target: MutableState<Option<String>>,
    status: MutableState<String>,
    theme: ThemeMode,
) {
    let icon = field.icon();
    Button(
        glass_button_modifier(
            Modifier::empty().fill_max_width(),
            theme,
            true,
            false,
            Color::from_rgba_u8(237, 250, 255, 210),
            11.0,
        )
        .padding_symmetric(13.0, 11.0),
        move || {
            active_queue_target.set(Some(field.component_key()));
            status.set(format!("Current queue row: {}.", field.label()));
        },
        move || {
            Row(
                icon_overlay_modifier(Modifier::empty(), icon, 24.0, 0.0, theme, false),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(9.0)),
                move || {
                    Spacer(Size::new(24.0, 0.0));
                    Text(field.label(), Modifier::empty(), queue_text_style(theme));
                },
            );
        },
    );
}

#[composable]
fn StatusDot(ok: bool, theme: ThemeMode) {
    ComposeBox(
        glass_button_modifier(
            Modifier::empty().size(Size {
                width: 22.0,
                height: 22.0,
            }),
            theme,
            true,
            ok,
            if ok {
                Color::from_rgb_u8(91, 204, 68)
            } else {
                Color::from_rgb_u8(247, 168, 21)
            },
            11.0,
        ),
        BoxSpec::default().content_alignment(Alignment::CENTER),
        move || {
            Text(
                if ok { "OK" } else { "!" },
                Modifier::empty(),
                dot_style(theme),
            );
        },
    );
}

#[composable]
fn section_card(theme: ThemeMode, content: impl FnMut() + 'static) {
    let radius = 18.0;
    let shape = LayerShape::Rounded(RoundedCornerShape::uniform(radius));
    glass_panel(
        Modifier::empty()
            .fill_max_width()
            .drop_shadow(shape, move |shadow| {
                shadow.radius = 22.0;
                shadow.spread = 1.0;
                shadow.offset = Point::new(0.0, 14.0);
                shadow.color = Color::from_rgba_u8(16, 79, 122, 82);
                shadow.alpha = 0.36;
            }),
        theme,
        radius,
        20.0,
        content,
    );
}

fn workspace_viewport_modifier(
    modifier: Modifier,
    theme: ThemeMode,
    scroll_state: ScrollState,
    viewport_width: f32,
    viewport_height: f32,
) -> Modifier {
    modifier
        .draw_with_content(move |scope| {
            scope.draw_content();
            let size = scope.size();
            let current_scroll = scroll_state.value_non_reactive();
            let max_scroll = scroll_state.max_value();
            if current_scroll > 0.5 {
                draw_workspace_scroll_shadow(scope, theme, size, true);
            }
            if max_scroll - current_scroll > 0.5 {
                draw_workspace_scroll_shadow(scope, theme, size, false);
            }
        })
        .graphics_layer_block(|layer| {
            layer.clip = true;
            layer.compositing_strategy = CompositingStrategy::Offscreen;
        })
        .rounded_alpha_mask(viewport_width, viewport_height, 0.0, 0.0)
        .clip_to_bounds()
}

fn draw_workspace_scroll_shadow(
    scope: &mut dyn DrawScope,
    theme: ThemeMode,
    size: Size,
    top: bool,
) {
    let shadow_height = 82.0_f32.min(size.height.max(0.0));
    if shadow_height <= 0.0 || size.width <= 0.0 {
        return;
    }
    let y = if top {
        0.0
    } else {
        (size.height - shadow_height).max(0.0)
    };
    let edge_color = match theme {
        ThemeMode::Dark => Color::from_rgba_u8(3, 33, 72, 178),
        ThemeMode::Light => Color::from_rgba_u8(4, 57, 105, 152),
    };
    let mid_color = match theme {
        ThemeMode::Dark => Color::from_rgba_u8(6, 76, 132, 92),
        ThemeMode::Light => Color::from_rgba_u8(8, 86, 140, 78),
    };
    let soft_color = match theme {
        ThemeMode::Dark => Color::from_rgba_u8(8, 86, 140, 38),
        ThemeMode::Light => Color::from_rgba_u8(8, 86, 140, 34),
    };
    let colors = if top {
        vec![edge_color, mid_color, soft_color, Color::TRANSPARENT]
    } else {
        vec![Color::TRANSPARENT, soft_color, mid_color, edge_color]
    };
    scope.draw_rect_at(
        Rect {
            x: 0.0,
            y,
            width: size.width,
            height: shadow_height,
        },
        Brush::linear_gradient_range(
            colors,
            Point::new(0.0, y),
            Point::new(0.0, y + shadow_height),
        ),
    );
}

#[composable]
fn glass_panel(
    modifier: Modifier,
    theme: ThemeMode,
    radius: f32,
    padding: f32,
    content: impl FnMut() + 'static,
) {
    ComposeBox(
        glass_panel_modifier(modifier, theme, radius).padding(padding),
        BoxSpec::default(),
        content,
    );
}

fn glass_panel_modifier(modifier: Modifier, theme: ThemeMode, radius: f32) -> Modifier {
    let shape = LayerShape::Rounded(RoundedCornerShape::uniform(radius));
    modifier
        .drop_shadow(shape, move |shadow| {
            shadow.radius = 18.0;
            shadow.spread = 0.0;
            shadow.offset = Point::new(0.0, 8.0);
            shadow.color = shadow_color(theme);
            shadow.alpha = 0.78;
        })
        .draw_behind(move |scope| {
            let size = scope.size();
            let radii = CornerRadii::uniform(radius);
            scope.draw_round_rect(panel_brush(theme, size), radii);
            scope.draw_round_rect(
                Brush::linear_gradient_range(
                    vec![
                        Color::from_rgba_u8(255, 255, 255, 205),
                        Color::from_rgba_u8(255, 255, 255, 62),
                        Color::from_rgba_u8(17, 144, 212, 42),
                    ],
                    Point::new(0.0, 0.0),
                    Point::new(size.width, size.height),
                ),
                radii,
            );
            scope.draw_rect_at(
                Rect {
                    x: 2.0,
                    y: 2.0,
                    width: (size.width - 4.0).max(0.0),
                    height: 2.0,
                },
                Brush::horizontal_gradient(
                    vec![
                        Color::TRANSPARENT,
                        Color::from_rgba_u8(255, 255, 255, 180),
                        Color::TRANSPARENT,
                    ],
                    0.0,
                    size.width,
                ),
            );
        })
        .inner_shadow(shape, move |shadow| {
            shadow.radius = 8.0;
            shadow.spread = -1.0;
            shadow.offset = Point::new(0.0, 2.0);
            shadow.color = Color::from_rgba_u8(255, 255, 255, 150);
            shadow.alpha = 0.72;
        })
        .rounded_corners(radius)
}

fn glass_button_modifier(
    modifier: Modifier,
    theme: ThemeMode,
    enabled: bool,
    active: bool,
    base: Color,
    radius: f32,
) -> Modifier {
    let shape = LayerShape::Rounded(RoundedCornerShape::uniform(radius));
    let shadow_alpha = if enabled { 0.64 } else { 0.18 };
    modifier
        .drop_shadow(shape, move |shadow| {
            shadow.radius = if active { 13.0 } else { 9.0 };
            shadow.spread = if active { 1.0 } else { 0.0 };
            shadow.offset = Point::new(0.0, if active { 6.0 } else { 4.0 });
            shadow.color = shadow_color(theme);
            shadow.alpha = shadow_alpha;
        })
        .draw_behind(move |scope| {
            let size = scope.size();
            let radii = CornerRadii::uniform(radius);
            let top = if enabled {
                lighten_color(base, if active { 0.56 } else { 0.38 })
            } else {
                base.with_alpha(0.5)
            };
            let bottom = if enabled {
                darken_color(base, if active { 0.16 } else { 0.06 })
            } else {
                base.with_alpha(0.38)
            };
            scope.draw_round_rect(
                Brush::linear_gradient_range(
                    vec![top, base.with_alpha(base.a().max(0.82)), bottom],
                    Point::new(0.0, 0.0),
                    Point::new(0.0, size.height),
                ),
                radii,
            );
            let gloss_height = (size.height * 0.44).max(1.0);
            scope.draw_round_rect(
                Brush::linear_gradient_range(
                    vec![
                        Color::from_rgba_u8(255, 255, 255, if active { 145 } else { 118 }),
                        Color::from_rgba_u8(255, 255, 255, if active { 58 } else { 42 }),
                        Color::TRANSPARENT,
                    ],
                    Point::new(0.0, 0.0),
                    Point::new(0.0, gloss_height),
                ),
                radii,
            );
            scope.draw_rect_at(
                Rect {
                    x: radius * 0.45,
                    y: 0.0,
                    width: (size.width - radius * 0.9).max(0.0),
                    height: 3.0,
                },
                Brush::horizontal_gradient(
                    vec![
                        Color::TRANSPARENT,
                        Color::from_rgba_u8(255, 255, 255, if active { 230 } else { 190 }),
                        Color::TRANSPARENT,
                    ],
                    0.0,
                    size.width,
                ),
            );
        })
        .inner_shadow(shape, move |shadow| {
            shadow.radius = 5.0;
            shadow.spread = -1.0;
            shadow.offset = Point::new(0.0, 1.0);
            shadow.color = Color::from_rgba_u8(255, 255, 255, if active { 180 } else { 115 });
            shadow.alpha = if enabled { 0.72 } else { 0.3 };
        })
        .rounded_corners(radius)
}

#[composable]
fn primary_button(
    icon: UiIcon,
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
        glass_button_modifier(
            Modifier::empty().weight(1.0),
            theme,
            !disabled,
            busy,
            background,
            10.0,
        )
        .height(46.0)
        .padding_symmetric(8.0, 9.0),
        move || {
            if disabled {
                return;
            }
            record_button_press(ui_preferences.clone(), &count_key);
            on_click();
        },
        move || {
            button_content(
                icon,
                label.to_string(),
                count,
                text_style.clone(),
                theme,
                busy,
                true,
            );
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
        glass_button_modifier(
            Modifier::empty(),
            theme,
            true,
            false,
            Color::from_rgba_u8(237, 250, 255, 185),
            9.0,
        )
        .padding_symmetric(9.0, 7.0),
        move || {
            record_button_press(ui_preferences.clone(), &count_key);
            on_click();
        },
        move || {
            button_content(
                button_icon_for_label(&label),
                label.clone(),
                count,
                subtle_button_text_style(theme),
                theme,
                false,
                false,
            );
        },
    );
}

#[composable]
fn theme_button(label: String, theme: ThemeMode, on_click: impl FnMut() + 'static) {
    Button(
        glass_button_modifier(
            Modifier::empty(),
            theme,
            true,
            false,
            Color::from_rgba_u8(237, 250, 255, 185),
            9.0,
        )
        .padding_symmetric(10.0, 7.0),
        on_click,
        move || {
            let label = label.clone();
            Row(
                icon_overlay_modifier(Modifier::empty(), UiIcon::Theme, 24.0, 0.0, theme, false),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(8.0)),
                move || {
                    Spacer(Size::new(24.0, 0.0));
                    Text(
                        label.clone(),
                        Modifier::empty(),
                        subtle_button_text_style(theme),
                    );
                    Text("v", Modifier::empty(), subtle_button_text_style(theme));
                },
            );
        },
    );
}

#[composable]
fn button_content(
    icon: UiIcon,
    label: String,
    count: u64,
    style: TextStyle,
    theme: ThemeMode,
    busy: bool,
    expand_label: bool,
) {
    let icon_size = 24.0;
    let row_modifier = if expand_label {
        Modifier::empty().fill_max_width()
    } else {
        Modifier::empty()
    };
    Row(
        icon_overlay_modifier(row_modifier, icon, icon_size, 0.0, theme, busy),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(4.0)),
        move || {
            let label = if busy {
                format!("{}...", label)
            } else {
                label.clone()
            };
            Spacer(Size::new(icon_size, 0.0));
            if expand_label {
                BasicText(
                    label,
                    Modifier::empty().weight(1.0),
                    style.clone(),
                    TextOverflow::Ellipsis,
                    false,
                    1,
                    1,
                );
            } else {
                BasicText(
                    label,
                    Modifier::empty(),
                    style.clone(),
                    TextOverflow::Ellipsis,
                    false,
                    1,
                    1,
                );
            }
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
            .padding_symmetric(5.0, 1.0),
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
    _highlighted: bool,
    theme: ThemeMode,
    allow_paste: bool,
) {
    let current_text = state.text();
    track_field_interaction(field_id, current_text.clone(), ui_preferences.clone());
    let is_changed = current_text != saved_text;
    let icon = UiIcon::for_field_id(field_id);
    ComposeBox(
        glass_panel_modifier(Modifier::empty().fill_max_width(), theme, 12.0)
            .padding_symmetric(12.0, 10.0),
        BoxSpec::default(),
        move || {
            let state = state.clone();
            let status = status.clone();
            let ui_preferences = ui_preferences.clone();
            Row(
                icon_overlay_modifier(
                    Modifier::empty().fill_max_width(),
                    icon,
                    44.0,
                    0.0,
                    theme,
                    false,
                ),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(16.0)),
                move || {
                    Spacer(Size::new(44.0, 0.0));
                    let field_state = state.clone();
                    Column(
                        Modifier::empty().weight(1.0),
                        ColumnSpec::default()
                            .vertical_arrangement(LinearArrangement::spaced_by(6.0)),
                        {
                            let field_state = field_state.clone();
                            move || {
                                Text(label, Modifier::empty(), label_style(theme, is_changed));
                                let field_state = field_state.clone();
                                ComposeBox(
                                    Modifier::empty()
                                        .fill_max_width()
                                        .background(input_surface(theme))
                                        .rounded_corners(8.0)
                                        .padding_symmetric(11.0, 7.0),
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
                            }
                        },
                    );
                    field_action_buttons(
                        label,
                        field_id,
                        state.clone(),
                        status.clone(),
                        allow_paste,
                        ui_preferences.clone(),
                        theme,
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
    _highlighted: bool,
    theme: ThemeMode,
) {
    let current_text = state.text();
    track_field_interaction(field_id, current_text.clone(), ui_preferences.clone());
    let is_changed = current_text != saved_text;
    let icon = UiIcon::for_field_id(field_id);
    ComposeBox(
        glass_panel_modifier(Modifier::empty().fill_max_width(), theme, 12.0)
            .padding_symmetric(12.0, 10.0),
        BoxSpec::default(),
        move || {
            let state = state.clone();
            let status = status.clone();
            let ui_preferences = ui_preferences.clone();
            Row(
                icon_overlay_modifier(
                    Modifier::empty().fill_max_width(),
                    icon,
                    44.0,
                    0.0,
                    theme,
                    false,
                ),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(16.0)),
                move || {
                    Spacer(Size::new(44.0, 0.0));
                    let field_state = state.clone();
                    Column(
                        Modifier::empty().weight(1.0),
                        ColumnSpec::default()
                            .vertical_arrangement(LinearArrangement::spaced_by(6.0)),
                        {
                            let field_state = field_state.clone();
                            move || {
                                Text(label, Modifier::empty(), label_style(theme, is_changed));
                                let field_state = field_state.clone();
                                ComposeBox(
                                    Modifier::empty()
                                        .fill_max_width()
                                        .background(input_surface(theme))
                                        .rounded_corners(8.0)
                                        .padding(12.0),
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
                            }
                        },
                    );
                    field_action_buttons(
                        label,
                        field_id,
                        state.clone(),
                        status.clone(),
                        true,
                        ui_preferences.clone(),
                        theme,
                    );
                },
            );
        },
    );
}

#[composable]
fn field_action_buttons(
    label: &'static str,
    field_id: &'static str,
    state: TextFieldState,
    status: MutableState<String>,
    allow_paste: bool,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    Row(
        Modifier::empty(),
        RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(10.0)),
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
        preferences.record_interactive_queue_item(count_key);
        preferences.clone()
    });
    let _ = persist_ui_preferences(&preferences);
}

fn record_component_interaction(ui_preferences: MutableState<UiPreferences>, component_key: &str) {
    let preferences = ui_preferences.update(|preferences| {
        preferences.mark_component_used(component_key);
        preferences.record_interactive_queue_item(component_key);
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
    let image_data_url = preview_webp_data_url(&draft).ok();
    let html = draft.rich_html_with_image(image_data_url.as_deref());
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
    // The Cranpose headless helper currently drops non-image layers for this
    // capture surface on desktop, leaving a background-only preview. Keep the
    // user-facing preview/export path on the same renderer used by Card Preview.
    render_preview_frame(draft)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
fn render_compose_preview_frame_with_helper(
    draft: &PostDraft,
) -> std::result::Result<PreviewFrame, String> {
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
                    robot.pump_frames(4)?;
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

#[cfg(not(target_arch = "wasm32"))]
pub fn run_app_capture_cli(output_path: &Path, size: Option<(u32, u32)>) -> Result<()> {
    let (tx, rx) = mpsc::channel::<std::result::Result<PreviewFrame, String>>();
    let (width, height) = size.unwrap_or((APP_WIDTH, APP_HEIGHT));

    let launch_result = launcher_with_size(width, height)
        .with_headless(true)
        .with_test_driver({
            let tx = tx.clone();
            move |robot| {
                let result = (|| -> std::result::Result<PreviewFrame, String> {
                    robot.wait_for_idle()?;
                    robot.pump_frames(4)?;
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
        .try_run(App);

    launch_result.map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let frame = rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|error| anyhow::anyhow!("timed out waiting for app screenshot: {error}"))?
        .map_err(anyhow::Error::msg)?;
    let image = RgbaImage::from_raw(frame.width, frame.height, frame.pixels)
        .ok_or_else(|| anyhow::anyhow!("invalid RGBA frame from app screenshot"))?;
    image
        .save_with_format(output_path, ImageFormat::Png)
        .with_context(|| format!("writing app screenshot {}", output_path.display()))?;
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
                    Some(sha) => {
                        let suffix = if outcome.pushed { " and pushed" } else { "" };
                        status.set(format!(
                            "Blog post {action}, image copied, committed {sha}{suffix}."
                        ));
                    }
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
        pushed: result.pushed,
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

fn panel_brush(theme: ThemeMode, size: Size) -> Brush {
    match theme {
        ThemeMode::Dark => Brush::linear_gradient_range(
            vec![
                Color::from_rgba_u8(244, 253, 255, 205),
                Color::from_rgba_u8(212, 244, 255, 162),
                Color::from_rgba_u8(187, 242, 236, 142),
            ],
            Point::new(0.0, 0.0),
            Point::new(size.width, size.height),
        ),
        ThemeMode::Light => Brush::linear_gradient_range(
            vec![
                Color::from_rgba_u8(255, 255, 255, 242),
                Color::from_rgba_u8(232, 251, 255, 214),
                Color::from_rgba_u8(214, 247, 239, 188),
            ],
            Point::new(0.0, 0.0),
            Point::new(size.width, size.height),
        ),
    }
}

fn shadow_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(24, 113, 178, 106),
        ThemeMode::Light => Color::from_rgba_u8(44, 147, 184, 86),
    }
}

fn lighten_color(color: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::rgba(
        color.r() + (1.0 - color.r()) * amount,
        color.g() + (1.0 - color.g()) * amount,
        color.b() + (1.0 - color.b()) * amount,
        color.a(),
    )
}

fn darken_color(color: Color, amount: f32) -> Color {
    let amount = 1.0 - amount.clamp(0.0, 1.0);
    Color::rgba(
        color.r() * amount,
        color.g() * amount,
        color.b() * amount,
        color.a(),
    )
}

fn app_title_style(theme: ThemeMode, compact: bool) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(primary_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(if compact { 28.0 } else { 35.0 }),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn panel_title_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(primary_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(18.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn dot_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(button_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(8.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
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
            font_size: cranpose::text::TextUnit::Sp(14.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn body_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(body_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(16.0),
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
            font_size: cranpose::text::TextUnit::Sp(14.0),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn code_field_style(theme: ThemeMode) -> TextStyle {
    code_text_style(14.0, theme)
}

fn label_style(theme: ThemeMode, is_changed: bool) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(if is_changed {
                changed_label_color(theme)
            } else {
                label_color(theme)
            }),
            font_size: cranpose::text::TextUnit::Sp(11.0),
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
            font_size: cranpose::text::TextUnit::Sp(10.0),
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
            font_size: cranpose::text::TextUnit::Sp(15.0),
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
            font_size: cranpose::text::TextUnit::Sp(10.0),
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
            font_size: cranpose::text::TextUnit::Sp(10.0),
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
            font_size: cranpose::text::TextUnit::Sp(12.0),
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
            font_size: cranpose::text::TextUnit::Sp(12.0),
            font_weight: Some(cranpose::text::FontWeight::SEMI_BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn queue_current_label_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(accent_color(theme).with_alpha(0.86)),
            font_size: cranpose::text::TextUnit::Sp(10.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn interactive_queue_text_style(
    theme: ThemeMode,
    done: bool,
    disabled: bool,
    busy: bool,
    pulse: f32,
) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(if disabled {
                muted_text_color(theme)
            } else if done || busy {
                button_text_color(theme).with_alpha(0.82 + 0.18 * pulse.max(0.0))
            } else {
                primary_text_color(theme)
            }),
            font_size: cranpose::text::TextUnit::Sp(12.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn badge_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(badge_text_color(theme)),
            font_size: cranpose::text::TextUnit::Sp(11.0),
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

fn panel_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(223, 245, 255, 190),
        ThemeMode::Light => Color::from_rgba_u8(238, 252, 255, 205),
    }
}

fn input_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(248, 253, 255, 218),
        ThemeMode::Light => Color::from_rgba_u8(255, 255, 255, 226),
    }
}

fn button_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(100, 207, 50),
        ThemeMode::Light => Color::from_rgb_u8(39, 145, 224),
    }
}

fn disabled_button_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(197, 220, 229, 165),
        ThemeMode::Light => Color::from_rgba_u8(213, 230, 236, 170),
    }
}

fn interactive_queue_surface(
    theme: ThemeMode,
    done: bool,
    disabled: bool,
    busy: bool,
    pulse: f32,
    invokes_button: bool,
) -> Color {
    if busy {
        return button_surface(theme).with_alpha(0.66 + 0.26 * pulse);
    }
    if disabled {
        return disabled_button_surface(theme);
    }
    if invokes_button && !done {
        return button_surface(theme);
    }
    if done {
        return match theme {
            ThemeMode::Dark => Color::from_rgb_u8(100, 207, 50),
            ThemeMode::Light => Color::from_rgb_u8(77, 184, 91),
        };
    }
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(237, 250, 255, 188),
        ThemeMode::Light => Color::from_rgba_u8(255, 255, 255, 218),
    }
}

fn badge_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(255, 255, 255, 215),
        ThemeMode::Light => Color::from_rgba_u8(226, 245, 252, 230),
    }
}

fn primary_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(12, 45, 86),
        ThemeMode::Light => Color::from_rgb_u8(14, 58, 96),
    }
}

fn body_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(45, 78, 105),
        ThemeMode::Light => Color::from_rgb_u8(52, 84, 107),
    }
}

fn muted_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(41, 78, 117),
        ThemeMode::Light => Color::from_rgb_u8(60, 96, 128),
    }
}

fn label_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(14, 77, 133),
        ThemeMode::Light => Color::from_rgb_u8(18, 87, 145),
    }
}

fn changed_label_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(20, 151, 164),
        ThemeMode::Light => Color::from_rgb_u8(9, 131, 154),
    }
}

fn accent_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(27, 129, 199),
        ThemeMode::Light => Color::from_rgb_u8(13, 117, 181),
    }
}

fn button_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(7, 45, 85),
        ThemeMode::Light => Color::from_rgb_u8(9, 58, 103),
    }
}

fn badge_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(16, 75, 111),
        ThemeMode::Light => Color::from_rgb_u8(18, 81, 116),
    }
}

#[cfg(test)]
mod tests {
    use crate::draft::{PostDraft, UiPreferences};
    use crate::export::PreviewState;

    use super::{
        APP_HEIGHT, APP_WIDTH, ActionButtonId, EditorFieldId, FieldQueueCommand,
        INTERACTIVE_QUEUE_CHIP_GAP, INTERACTIVE_QUEUE_CHIP_WIDTH, META_FIELDS, NextWorkItem,
        WEB_SURFACE_MAX_DIM, compute_web_canvas_size, interactive_queue_label,
        interactive_queue_next_key, interactive_queue_scroll_target,
        interactive_queue_selected_key, interactive_queue_should_auto_scroll,
        ordered_action_buttons, ordered_fields, parse_field_queue_key, queue_item_invokes_button,
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
    fn interactive_queue_labels_actions_and_field_commands() {
        assert_eq!(
            interactive_queue_label(ActionButtonId::CopyBlog.count_key(), false, false),
            "Copy Blog"
        );
        assert_eq!(
            interactive_queue_label("field.problem_title", true, false),
            "Done: Edit Problem Title"
        );
        assert_eq!(
            parse_field_queue_key("field.problem_title.clear"),
            Some((EditorFieldId::ProblemTitle, FieldQueueCommand::Clear))
        );
        assert!(queue_item_invokes_button(
            ActionButtonId::CopyBlog.count_key()
        ));
        assert!(queue_item_invokes_button("field.problem_title.clear"));
        assert!(!queue_item_invokes_button("field.problem_title"));
    }

    #[test]
    fn interactive_queue_next_key_skips_done_items() {
        let queue = vec![
            ActionButtonId::CopyBlog.count_key().to_string(),
            "field.problem_title".to_string(),
        ];
        let done = vec![ActionButtonId::CopyBlog.count_key().to_string()];

        assert_eq!(
            interactive_queue_next_key(&queue, &done),
            Some("field.problem_title".to_string())
        );
    }

    #[test]
    fn interactive_queue_selected_key_prefers_active_target() {
        let queue = vec![
            ActionButtonId::CopyBlog.count_key().to_string(),
            "field.problem_title".to_string(),
        ];
        let done = vec![];

        assert_eq!(
            interactive_queue_selected_key(&queue, &done, Some("field.problem_title")),
            Some("field.problem_title".to_string())
        );
    }

    #[test]
    fn interactive_queue_selected_key_moves_past_done_active_target() {
        let queue = vec![
            "field.problem_title".to_string(),
            "field.youtube_url".to_string(),
        ];
        let done = vec!["field.problem_title".to_string()];

        assert_eq!(
            interactive_queue_selected_key(&queue, &done, Some("field.problem_title")),
            Some("field.youtube_url".to_string())
        );
    }

    #[test]
    fn interactive_queue_scroll_target_moves_right_half_item_leftward() {
        let target = interactive_queue_scroll_target(3, 500.0, 0.0);

        assert!(target > 0.0);
        let item_center = 3.0 * (INTERACTIVE_QUEUE_CHIP_WIDTH + INTERACTIVE_QUEUE_CHIP_GAP)
            + INTERACTIVE_QUEUE_CHIP_WIDTH * 0.5;
        assert!(item_center - target < 250.0);
    }

    #[test]
    fn interactive_queue_auto_scroll_only_runs_for_new_selection_or_retry() {
        assert!(interactive_queue_should_auto_scroll(
            Some("field.problem_title"),
            None,
            0
        ));
        assert!(interactive_queue_should_auto_scroll(
            Some("field.problem_title"),
            Some("field.problem_title"),
            1
        ));
        assert!(!interactive_queue_should_auto_scroll(
            Some("field.problem_title"),
            Some("field.problem_title"),
            0
        ));
        assert!(!interactive_queue_should_auto_scroll(None, None, 0));
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
