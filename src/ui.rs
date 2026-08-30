#![allow(non_snake_case)]

use crate::draft::{
    EditorFields, PostDraft, ThemeMode, UiPreferences, autosave_destination_label,
    load_initial_draft, load_ui_preferences, persist_autosave, persist_ui_preferences,
    startup_status_message,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::export::PreviewFrame;
use crate::export::{PreviewState, preview_webp_data_url, render_preview_frame, save_webp};
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
    AnimationSpec, Easing, RepeatMode, Spring, StartOffset, animateFloatAsState,
    infiniteRepeatable, rememberInfiniteTransition, spring, tween,
};
use cranpose_core::MutableState;
use cranpose_foundation::DrawScope;
use cranpose_foundation::PointerButtons;
use cranpose_foundation::text::{TextFieldLineLimits, TextFieldState};
#[cfg(not(target_arch = "wasm32"))]
use image::ImageFormat;
use image::{RgbaImage, imageops::FilterType};
#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Object, Promise, Reflect};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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
const BUTTON_ACTIVITY_INDICATOR_WIDTH: f32 = 66.0;
const BUTTON_ACTIVITY_INDICATOR_HEIGHT: f32 = 18.0;
const BUTTON_PRESS_RELEASE_DURATION_MS: u64 = 180;
const BUTTON_PRESS_SCALE_DELTA: f32 = 0.018;
const BUTTON_PRESS_TRANSLATION_Y: f32 = 1.4;
#[cfg(not(target_arch = "wasm32"))]
const MIN_LONG_ACTION_BUSY_MS: u64 = 1_250;
#[cfg(any(test, target_arch = "wasm32"))]
const WEB_SURFACE_MAX_DIM: u32 = 1900;
#[cfg(target_arch = "wasm32")]
const WEB_CANVAS_MARGIN: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ActionButtonId {
    RefreshRasterPreview,
    CopyLeetcode,
    CopyYoutube,
    CopyBlog,
    CopyTelegram,
    CopyTitle,
    CopySubtitle,
    CopyRichText,
    SaveRasterWebp,
    PublishBlog,
    PostTelegram,
    PostTelegramComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum LongAction {
    RefreshRasterPreview,
    #[cfg(not(target_arch = "wasm32"))]
    CopyRichText,
    SaveRasterWebp,
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

/// Handles for the editor's action machinery, created once in [`App`] and
/// passed down as a single `Copy` value instead of seven parallel parameters.
#[derive(Clone, Copy, PartialEq)]
struct ActionStates {
    status: MutableState<String>,
    telegram_post_link: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    pending_action: MutableState<Option<PendingAction>>,
    action_request_counter: MutableState<u64>,
    busy_action: MutableState<Option<LongAction>>,
    active_queue_target: MutableState<Option<String>>,
    /// Session-only set of queue keys the user chose to skip. Not persisted, so
    /// a skipped action reappears on the next launch and is never recorded.
    skipped_queue: MutableState<Vec<String>>,
}

/// Handles for the raster preview pipeline.
#[derive(Clone, Copy, PartialEq)]
struct PreviewStates {
    preview_state: MutableState<PreviewState>,
    preview_loading: MutableState<bool>,
}

/// Startup-time snapshots of the editing session, loaded once in [`App`].
#[derive(Clone, PartialEq)]
struct EditorSession {
    saved_draft: PostDraft,
    layout_preferences: UiPreferences,
    startup_interactive_queue: Vec<String>,
    autosave_destination: String,
}

/// Everything [`labeled_field`] needs to render one editor field.
#[derive(Clone, PartialEq)]
struct FieldSpec {
    label: &'static str,
    field_id: &'static str,
    state: TextFieldState,
    saved_text: String,
    min_lines: usize,
    max_lines: usize,
    allow_paste: bool,
    code: bool,
}

#[derive(Clone)]
enum LongActionResult {
    RefreshRasterPreview(std::result::Result<PreviewState, String>),
    #[cfg(not(target_arch = "wasm32"))]
    CopyRichText(std::result::Result<(), String>),
    SaveRasterWebp(std::result::Result<PreviewState, String>),
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

const ACTION_BUTTONS: [ActionButtonId; 12] = [
    ActionButtonId::CopyLeetcode,
    ActionButtonId::CopyYoutube,
    ActionButtonId::CopyBlog,
    ActionButtonId::CopyTelegram,
    ActionButtonId::CopyTitle,
    ActionButtonId::CopySubtitle,
    ActionButtonId::CopyRichText,
    ActionButtonId::RefreshRasterPreview,
    ActionButtonId::SaveRasterWebp,
    ActionButtonId::PublishBlog,
    ActionButtonId::PostTelegram,
    ActionButtonId::PostTelegramComment,
];

#[cfg(test)]
const META_FIELDS: [EditorFieldId; 8] = [
    EditorFieldId::Date,
    EditorFieldId::ProblemTitle,
    EditorFieldId::ProblemUrl,
    EditorFieldId::Difficulty,
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

const WORKFLOW_FIELDS: [EditorFieldId; 17] = [
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
    let scroll_state = remember(|| ScrollState::new(0.0)).with(|state| *state);
    let saved_draft = remember(load_initial_draft).with(|draft| draft.clone());
    let fields = remember({
        let saved_draft = saved_draft.clone();
        move || EditorFields::from_draft(&saved_draft)
    })
    .with(|fields| fields.clone());
    let ui_preferences = rememberMutableStateOf(load_ui_preferences);
    let startup_interactive_queue = remember({
        let initial_queue = ui_preferences.value().remembered_queue().to_vec();
        move || initial_queue
    })
    .with(|queue| queue.clone());
    let layout_preferences = remember({
        let initial_preferences = ui_preferences.value();
        move || initial_preferences
    })
    .with(|preferences| preferences.clone());
    let autosave_destination = remember(autosave_destination_label).with(|label| label.clone());
    // PreviewState holds an ImageBitmap and isn't PartialEq, so it can't use
    // rememberMutableStateOf (which requires structural equality). This is
    // remember+mutableStateOf spelled out, matching the pre-0.1.106 useState
    // semantics: recreated once per slot, every set() always notifies.
    let preview_state =
        remember(|| mutableStateOf(PreviewState::placeholder())).with(|state| *state);
    let preview_loading = rememberMutableStateOf(|| false);
    let telegram_post_link = rememberMutableStateOf(String::new);
    let status = rememberMutableStateOf(startup_status_message);
    let pending_action = rememberMutableStateOf(|| None::<PendingAction>);
    let action_request_counter = rememberMutableStateOf(|| 0u64);
    let busy_action = rememberMutableStateOf(|| None::<LongAction>);
    let active_queue_target = rememberMutableStateOf(|| None::<String>);
    let skipped_queue = rememberMutableStateOf(Vec::<String>::new);
    let actions = ActionStates {
        status,
        telegram_post_link,
        ui_preferences,
        pending_action,
        action_request_counter,
        busy_action,
        active_queue_target,
        skipped_queue,
    };
    let previews = PreviewStates {
        preview_state,
        preview_loading,
    };
    let session = EditorSession {
        saved_draft,
        layout_preferences,
        startup_interactive_queue,
        autosave_destination,
    };
    let current_draft = PostDraft::from_fields(&fields);
    let queued_action = pending_action.value();
    let theme = ui_preferences.value().theme;
    let queue_reset_done = rememberMutableStateOf(|| false);
    // Measured height of the full header (first item in the scroll), used to
    // fade in the pinned collapsed bar exactly as the header scrolls out.
    let header_height = rememberMutableStateOf(|| 0.0f32);
    // The field the hero currently points at, so its editor row can glow too.
    let current_field = resolve_next_work(
        &current_draft,
        &preview_state.value(),
        &telegram_post_link.value(),
        &skipped_queue.value(),
        &session.startup_interactive_queue,
        ui_preferences.value().interactive_queue(),
        &session.layout_preferences,
    )
    .field;

    cranpose_core::LaunchedEffect!(queue_reset_done.value(), {
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

    // Keep the shader-flame animations running. The flames advance off the frame
    // clock, but cranpose's desktop event loop only stays in continuous-render
    // (`ControlFlow::Poll`) mode while `should_render()` is true — and a bare
    // frame-clock loop (like `rememberInfiniteTransition`'s) does not keep it
    // true, so when the app is idle it renders ~once and the flames freeze until
    // the next input. AnimationPump invalidates a tiny composition scope every
    // frame, which keeps the loop hot so every flame keeps animating.
    AnimationPump();

    cranpose_core::LaunchedEffect!(current_draft.clone(), {
        let draft = current_draft.clone();
        move |_scope| {
            if let Err(error) = persist_autosave(&draft) {
                status.set(format!("Autosave failed: {error}"));
            }
        }
    });

    cranpose_core::LaunchedEffect!(queued_action.clone(), {
        let fields = fields.clone();
        move |scope| {
            let Some(action) = queued_action.clone() else {
                return;
            };

            scope.launch_background(
                move |_| async move { run_long_action(action) },
                move |result| {
                    finish_long_action(result, previews, actions, fields.clone());
                },
            );
        }
    });

    ComposeBox(
        Modifier::empty().fill_max_size().draw_behind(move |scope| {
            draw_app_background(scope, theme);
        }),
        BoxSpec::default(),
        {
            let fields = fields.clone();
            let session = session.clone();
            move || {
                Column(Modifier::empty().fill_max_size(), ColumnSpec::default(), {
                    let scroll_state = scroll_state;
                    let fields = fields.clone();
                    let session = session.clone();
                    move || {
                        BoxWithConstraints(Modifier::empty().fill_max_width().weight(1.0), {
                            let scroll_state = scroll_state;
                            let fields = fields.clone();
                            let session = session.clone();
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
                                        let session = session.clone();
                                        let workspace_scroll_state = scroll_state;
                                        move || {
                                            let viewport_scroll_state = workspace_scroll_state;
                                            BoxWithConstraints(
                                                Modifier::empty().fill_max_width().weight(1.0),
                                                {
                                                    let fields = fields.clone();
                                                    let session = session.clone();
                                                    move |viewport_scope| {
                                                        let viewport_width =
                                                            viewport_scope.max_width().0;
                                                        let viewport_height =
                                                            viewport_scope.max_height().0;
                                                        let vss = viewport_scroll_state;
                                                        // Overlay container: the scrollable body
                                                        // (header + workspace) with the pinned
                                                        // collapsed bar stacked on top.
                                                        ComposeBox(
                                                            Modifier::empty().fill_max_size(),
                                                            BoxSpec::default().content_alignment(
                                                                Alignment::TOP_START,
                                                            ),
                                                            {
                                                                let fields = fields.clone();
                                                                let session = session.clone();
                                                                move || {
                                                                    let fields = fields.clone();
                                                                    let session = session.clone();
                                                                    let vss = vss;
                                                                    ComposeBox(
                                                                        workspace_viewport_modifier(
                                                                            Modifier::empty()
                                                                                .fill_max_size(),
                                                                            theme,
                                                                            vss,
                                                                            viewport_width,
                                                                            viewport_height,
                                                                        ),
                                                                        BoxSpec::default(),
                                                                        {
                                                                            let fields =
                                                                                fields.clone();
                                                                            let session =
                                                                                session.clone();
                                                                            move || {
                                                                                let fields =
                                                                                    fields.clone();
                                                                                let session =
                                                                                    session.clone();
                                                                                Column(
                                                                                    Modifier::empty().fill_max_width().vertical_scroll(vss, false).padding_each(0.0, 4.0, 0.0, 0.0),
                                                                                    ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(18.0)),
                                                                                    move || {
                                                                                        let fields = fields.clone();
                                                                                        let session = session.clone();
                                                                                        ComposeBox(
                                                                                            Modifier::empty().fill_max_width().draw_behind(move |draw| {
                                                                                                let h = draw.size().height;
                                                                                                if (header_height.get_non_reactive() - h).abs() > 0.5 {
                                                                                                    header_height.set(h);
                                                                                                }
                                                                                            }),
                                                                                            BoxSpec::default(),
                                                                                            {
                                                                                                let fields = fields.clone();
                                                                                                let session = session.clone();
                                                                                                move || {
                                                                                                    ActionsCard(fields.clone(), session.clone(), actions, preview_state, theme, compact);
                                                                                                }
                                                                                            },
                                                                                        );
                                                                                        GuidedWorkspace(fields.clone(), previews, session.clone(), actions, theme, compact, current_field);
                                                                                        Spacer(Size::new(0.0, 86.0));
                                                                                    },
                                                                                );
                                                                            }
                                                                        },
                                                                    );
                                                                    CollapsedHeaderOverlay(
                                                                        fields.clone(),
                                                                        session.clone(),
                                                                        actions,
                                                                        preview_state,
                                                                        theme,
                                                                        vss,
                                                                        header_height,
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
                        BottomListGapMask(theme);
                    }
                });
            }
        },
    );
}

#[composable]
fn GuidedWorkspace(
    fields: EditorFields,
    previews: PreviewStates,
    session: EditorSession,
    actions: ActionStates,
    theme: ThemeMode,
    compact: bool,
    current_field: Option<EditorFieldId>,
) {
    ProblemMetaCard(
        fields.clone(),
        session.clone(),
        actions,
        theme,
        compact,
        current_field,
    );
    WriteupCard(
        fields.clone(),
        session.clone(),
        actions,
        theme,
        current_field,
    );
    Spacer(Size::new(0.0, 82.0));
    CodeCard(fields, session, actions, theme, current_field);
    PreviewCard(previews.preview_state, previews.preview_loading, theme);
}

/// Scroll offset (px) past which the tall header collapses, and the lower
/// offset it must fall back under to expand again. The gap is hysteresis: it
/// stops the header oscillating when a collapse/expand itself shifts the layout.
/// The current recommended work item, resolved once and shared by the header,
/// the collapsed overlay, the Quick Actions grid, and the field-editor glow.
struct NextWork {
    item: NextWorkItem,
    title: Option<String>,
    skip_key: String,
    action: Option<ActionButtonId>,
    field: Option<EditorFieldId>,
}

fn resolve_next_work(
    draft: &PostDraft,
    preview: &PreviewState,
    telegram_link: &str,
    skipped: &[String],
    startup_queue: &[String],
    current_queue: &[String],
    layout_prefs: &UiPreferences,
) -> NextWork {
    let mut excluded = current_queue.to_vec();
    excluded.extend(skipped.iter().cloned());
    let next_queue_key = interactive_queue_next_key(startup_queue, &excluded);
    let item = next_queue_key
        .as_deref()
        .and_then(next_work_item_from_queue_key)
        .unwrap_or_else(|| {
            recommended_next_work_excluding(draft, preview, telegram_link, layout_prefs, skipped)
        });
    let skip_key = next_queue_key
        .clone()
        .unwrap_or_else(|| next_work_item_key(item));
    let title = next_queue_key
        .as_deref()
        .map(|key| interactive_queue_label(key, false, false));
    let (action, field) = match item {
        NextWorkItem::Action(action) => (Some(action), None),
        NextWorkItem::Field(field) => (None, Some(field)),
    };
    NextWork {
        item,
        title,
        skip_key,
        action,
        field,
    }
}

/// Smooth 0→1 ramp between two edges (Hermite), used to drive scroll-linked fades.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[composable]
fn ActionsCard(
    fields: EditorFields,
    session: EditorSession,
    actions: ActionStates,
    preview_state: MutableState<PreviewState>,
    theme: ThemeMode,
    compact: bool,
) {
    let ActionStates {
        status,
        telegram_post_link,
        ui_preferences,
        ..
    } = actions;
    Column(
        Modifier::empty().fill_max_width(),
        ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
        {
            let fields = fields.clone();
            let session = session.clone();
            move || {
                let draft = PostDraft::from_fields(&fields);
                let next = resolve_next_work(
                    &draft,
                    &preview_state.value(),
                    &telegram_post_link.value(),
                    &actions.skipped_queue.value(),
                    &session.startup_interactive_queue,
                    ui_preferences.value().interactive_queue(),
                    &session.layout_preferences,
                );
                let next_item = next.item;
                let skip_key = next.skip_key;
                let next_title = next.title;
                let next_action = next.action;

                HeaderBar(
                    session.autosave_destination.clone(),
                    ui_preferences,
                    status,
                    theme,
                    compact,
                );

                if compact {
                    Column(
                        Modifier::empty().fill_max_width(),
                        ColumnSpec::default()
                            .vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                        {
                            let fields = fields.clone();
                            let session = session.clone();
                            move || {
                                NextWorkPanel(
                                    next_item,
                                    next_title.clone(),
                                    skip_key.clone(),
                                    fields.clone(),
                                    actions,
                                    theme,
                                    true,
                                );
                                QuickActionsPanel(
                                    fields.clone(),
                                    session.clone(),
                                    actions,
                                    theme,
                                    true,
                                    next_action,
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
                            let session = session.clone();
                            move || {
                                NextWorkPanel(
                                    next_item,
                                    next_title.clone(),
                                    skip_key.clone(),
                                    fields.clone(),
                                    actions,
                                    theme,
                                    false,
                                );
                                QuickActionsPanel(
                                    fields.clone(),
                                    session.clone(),
                                    actions,
                                    theme,
                                    false,
                                    next_action,
                                );
                            }
                        },
                    );
                }

                InteractiveQueuePanel(
                    session.startup_interactive_queue.clone(),
                    fields.clone(),
                    actions,
                    theme,
                );

                SessionQueuePanel(actions, theme);

                StatusStrip(status.value(), theme);

                if let Some(saved_webp) = preview_state.value().last_saved_webp_path {
                    Text(
                        format!("Latest WebP: {saved_webp}"),
                        Modifier::empty(),
                        body_style(theme),
                    );
                }
                let latest_telegram_link = telegram_post_link.value();
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

/// Pinned collapsed header that fades in as the full header scrolls out of the
/// viewport. Because it is an overlay it never resizes the scroll region, so the
/// collapse follows the scroll smoothly with no feedback loop or snap.
#[composable]
fn CollapsedHeaderOverlay(
    fields: EditorFields,
    session: EditorSession,
    actions: ActionStates,
    preview_state: MutableState<PreviewState>,
    theme: ThemeMode,
    scroll_state: ScrollState,
    header_height: MutableState<f32>,
) {
    let scroll = scroll_state.value();
    let hh = header_height.value();
    let alpha = if hh < 50.0 {
        0.0
    } else {
        smoothstep(hh - 200.0, hh - 120.0, scroll)
    };
    if alpha <= 0.001 {
        return;
    }
    let ActionStates {
        status,
        telegram_post_link,
        ui_preferences,
        ..
    } = actions;
    let draft = PostDraft::from_fields(&fields);
    let next = resolve_next_work(
        &draft,
        &preview_state.value(),
        &telegram_post_link.value(),
        &actions.skipped_queue.value(),
        &session.startup_interactive_queue,
        ui_preferences.value().interactive_queue(),
        &session.layout_preferences,
    );
    ComposeBox(
        Modifier::empty()
            .fill_max_width()
            .graphics_layer_block(move |layer| {
                layer.alpha = alpha;
            }),
        BoxSpec::default(),
        {
            let fields = fields.clone();
            move || {
                CollapsedHeader(
                    next.item,
                    next.title.clone(),
                    status.value(),
                    fields.clone(),
                    actions,
                    theme,
                );
            }
        },
    );
}

/// Thin header shown when the workspace is scrolled: a scaled-down hero (stage
/// badge, next-action title, fire-glow button) plus the live status line.
#[composable]
fn CollapsedHeader(
    next_item: NextWorkItem,
    next_title: Option<String>,
    status_message: String,
    fields: EditorFields,
    actions: ActionStates,
    theme: ThemeMode,
) {
    let title = next_title.unwrap_or_else(|| next_item.title());
    let stage = next_item.stage();
    let hero = match next_item {
        NextWorkItem::Field(field) => HeroButton::Field(field),
        NextWorkItem::Action(action) => HeroButton::Action(action),
    };
    let content_height = match next_item {
        NextWorkItem::Action(_) => 60.0,
        NextWorkItem::Field(_) => 48.0,
    };
    glass_panel(Modifier::empty().fill_max_width(), theme, 14.0, 12.0, {
        let fields = fields.clone();
        move || {
            let fields = fields.clone();
            let title = title.clone();
            let status_message = status_message.clone();
            Row(
                Modifier::empty().fill_max_width(),
                RowSpec::default()
                    .horizontal_arrangement(LinearArrangement::spaced_by(14.0))
                    .vertical_alignment(VerticalAlignment::CenterVertically),
                move || {
                    ReferenceIcon(stage.icon(), Size::new(38.0, 38.0), theme, true);
                    Column(
                        Modifier::empty().weight(1.4),
                        ColumnSpec::default()
                            .vertical_arrangement(LinearArrangement::spaced_by(3.0)),
                        {
                            let title = title.clone();
                            let status_message = status_message.clone();
                            move || {
                                Row(
                                    Modifier::empty(),
                                    RowSpec::default()
                                        .horizontal_arrangement(LinearArrangement::spaced_by(8.0))
                                        .vertical_alignment(VerticalAlignment::CenterVertically),
                                    move || {
                                        Text("Now", Modifier::empty(), eyebrow_style(theme));
                                        Text(
                                            stage.label(),
                                            Modifier::empty(),
                                            stage_label_style(theme),
                                        );
                                    },
                                );
                                BasicText(
                                    title.clone(),
                                    Modifier::empty().fill_max_width(),
                                    heading_style(17.0, theme),
                                    TextOverflow::Ellipsis,
                                    false,
                                    1,
                                    1,
                                );
                                let status_message = status_message.clone();
                                Row(
                                    Modifier::empty().fill_max_width(),
                                    RowSpec::default()
                                        .horizontal_arrangement(LinearArrangement::spaced_by(8.0))
                                        .vertical_alignment(VerticalAlignment::CenterVertically),
                                    move || {
                                        StatusDot(true, theme);
                                        BasicText(
                                            status_message.clone(),
                                            Modifier::empty().weight(1.0),
                                            accent_style(theme),
                                            TextOverflow::Ellipsis,
                                            false,
                                            1,
                                            1,
                                        );
                                    },
                                );
                            }
                        },
                    );
                    let fields_btn = fields.clone();
                    ComposeBox(
                        Modifier::empty().weight(1.0),
                        BoxSpec::default(),
                        move || {
                            hero_fire_glow(
                                hero,
                                content_height,
                                12.0,
                                fields_btn.clone(),
                                actions,
                                theme,
                            );
                        },
                    );
                },
            );
        }
    });
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
                    record_button_press(ui_preferences, "theme.toggle");
                    set_theme_preference(ui_preferences, next_theme, status);
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
                    record_button_press(ui_preferences, "theme.toggle");
                    set_theme_preference(ui_preferences, next_theme, status);
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
    session: EditorSession,
    actions: ActionStates,
    theme: ThemeMode,
    compact: bool,
    next_action: Option<ActionButtonId>,
) {
    let modifier = if compact {
        Modifier::empty().fill_max_width()
    } else {
        Modifier::empty().weight(2.04)
    };
    glass_panel(modifier, theme, 18.0, 18.0, {
        let fields = fields.clone();
        let session = session.clone();
        move || {
            let session = session.clone();
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(12.0)),
                {
                    let fields = fields.clone();
                    move || {
                        Text("Quick Actions", Modifier::empty(), panel_title_style(theme));
                        ActionButtons(fields.clone(), session.clone(), actions, theme, next_action);
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

// ── Fire-shader hero glow ────────────────────────────────────────────────
// Ported from the Cranpose desktop demo's shader-rect tab: an animated flame
// border drawn around the hero "next action" button via an offscreen runtime
// shader.

const HERO_FIRE_WGSL_PREAMBLE: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn fullscreen_vs(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var output: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 2 - 1);
    let y = f32(i32(vertex_index >> 1u) * 2 - 1);
    output.uv = vec2<f32>(x * 0.5 + 0.5, 1.0 - (y * 0.5 + 0.5));
    output.position = vec4<f32>(x, y, 0.0, 1.0);
    return output;
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(1) @binding(0) var<uniform> u: array<vec4<f32>, 64>;

fn get_float(index: u32) -> f32 {
    return u[index / 4u][index % 4u];
}

fn get_vec2(index: u32) -> vec2<f32> {
    return vec2<f32>(get_float(index), get_float(index + 1u));
}

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}
"#;

fn hero_fire_wgsl() -> Arc<str> {
    static SOURCE: OnceLock<Arc<str>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            Arc::<str>::from(format!(
                r#"{preamble}

const PI: f32 = 3.14159265358979;
const TWO_PI: f32 = 6.28318530717959;

fn rand_f(n: vec2<f32>) -> f32 {{
    return fract(sin(dot(n, vec2<f32>(12.9898, 12.1414))) * 83758.5453);
}}

fn noise_f(n: vec2<f32>) -> f32 {{
    let b = floor(n);
    let f = fract(n);
    return mix(
        mix(rand_f(b), rand_f(b + vec2<f32>(1.0, 0.0)), f.x),
        mix(rand_f(b + vec2<f32>(0.0, 1.0)), rand_f(b + vec2<f32>(1.0, 1.0)), f.x),
        f.y
    );
}}

fn fire_f(n: vec2<f32>) -> f32 {{
    return noise_f(n) + noise_f(n * 2.1) * 0.6 + noise_f(n * 5.4) * 0.42;
}}

fn ramp(t_in: f32) -> vec3<f32> {{
    let t = max(t_in, 0.001);
    if (t <= 0.5) {{
        return vec3<f32>(1.0 - t * 1.4, 0.2, 1.05) / t;
    }}
    return vec3<f32>(0.3 * (1.0 - t) * 2.0, 0.2, 1.05) / t;
}}

fn shade(uv_in: vec2<f32>, t: f32) -> f32 {{
    var uv = uv_in;
    if (uv.y < 0.5) {{
        uv.x = uv.x + 23.0 + t * 0.035;
    }} else {{
        uv.x = uv.x - 11.0 + t * 0.03;
    }}
    uv.y = abs(uv.y - 0.5);
    uv.x = uv.x * 35.0;

    let q = fire_f(uv - t * 0.013) / 2.0;
    let rv = vec2<f32>(
        fire_f(uv + q / 2.0 + t - uv.x - uv.y),
        fire_f(uv + q - t)
    );
    return pow((rv.y + rv.y) * max(0.0, uv.y) + 0.1, 4.0);
}}

fn color_from_grad(grad: f32) -> vec3<f32> {{
    let g = sqrt(max(grad, 0.0));
    let c = ramp(g);
    return c / (vec3<f32>(1.15) + max(vec3<f32>(0.0), c));
}}

fn perimeter_s(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {{
    let inner = max(half_size - vec2<f32>(r), vec2<f32>(0.0001));
    let lh = 2.0 * inner.x;
    let lv = 2.0 * inner.y;
    let lc = 0.5 * PI * r;
    let a = abs(p);
    let is_corner = (a.x > inner.x) && (a.y > inner.y);

    if (!is_corner) {{
        if (a.x > inner.x) {{
            if (p.x >= 0.0) {{ return clamp(p.y + inner.y, 0.0, lv); }}
            else {{ return lv + lc + lh + lc + clamp(inner.y - p.y, 0.0, lv); }}
        }}
        if (a.y > inner.y) {{
            if (p.y >= 0.0) {{ return lv + lc + clamp(inner.x - p.x, 0.0, lh); }}
            else {{ return lv + lc + lh + lc + lv + lc + clamp(p.x + inner.x, 0.0, lh); }}
        }}
        return clamp(p.y + inner.y, 0.0, lv);
    }}

    if (p.x >= 0.0 && p.y >= 0.0) {{
        let c = vec2<f32>(inner.x, inner.y);
        let v = (p - c) / r;
        return lv + clamp(atan2(v.y, v.x), 0.0, 0.5 * PI) * r;
    }}
    if (p.x <= 0.0 && p.y >= 0.0) {{
        let c = vec2<f32>(-inner.x, inner.y);
        let v = (p - c) / r;
        return lv + lc + lh + (clamp(atan2(v.y, v.x), 0.5 * PI, PI) - 0.5 * PI) * r;
    }}
    if (p.x <= 0.0 && p.y <= 0.0) {{
        let c = vec2<f32>(-inner.x, -inner.y);
        let v = (p - c) / r;
        var ang = atan2(v.y, v.x);
        if (ang < 0.0) {{ ang = ang + TWO_PI; }}
        ang = clamp(ang, PI, PI + 0.5 * PI);
        return lv + lc + lh + lc + lv + (ang - PI) * r;
    }}
    let c = vec2<f32>(inner.x, -inner.y);
    let v = (p - c) / r;
    let base = lv + lc + lh + lc + lv + lc + lh;
    return base + (clamp(atan2(v.y, v.x), -0.5 * PI, 0.0) + 0.5 * PI) * r;
}}

@fragment
fn effect_fs(input: VertexOutput) -> @location(0) vec4<f32> {{
    let uv_screen = input.uv;
    let tex_size = vec2<f32>(textureDimensions(input_texture));
    let effect_rect = vec4<f32>(get_float(248u), get_float(249u), get_float(250u), get_float(251u));
    let resolution = get_vec2(0u);
    let dp_scale = effect_rect.zw / max(resolution, vec2<f32>(1.0));
    let s = min(dp_scale.x, dp_scale.y);
    let local_px = uv_screen * tex_size - effect_rect.xy;

    let time_raw = get_float(2u);
    let band_px = get_float(3u) * s;
    let corner_raw = get_float(4u) * s;
    let contour_size = vec2<f32>(get_float(5u) * dp_scale.x, get_float(6u) * dp_scale.y);
    let smoke_scale = get_float(7u);
    let intensity = get_float(8u);
    let smoke_opacity = get_float(9u);
    let core_scale = get_float(10u);
    let smoke_blue_tint = clamp(get_float(11u), 0.0, 1.0);
    let thin_mode = clamp(get_float(12u), 0.0, 1.0);

    let size_px = resolution * dp_scale;
    let res = max(size_px, vec2<f32>(1.0));
    let t = time_raw * 60.0;
    let p = local_px - 0.5 * res;
    let p_norm = p / max(res.y, 1.0);

    let min_thickness = mix(1.5, 0.55, thin_mode);
    let thickness = max(min_thickness, band_px * mix(0.30, 0.16, thin_mode));
    let smoke_w = max(thickness * 4.8, band_px * 2.2) * max(smoke_scale, 0.3) * mix(1.0, 0.30, thin_mode);

    let half_size = contour_size * 0.5;
    var radius = min(corner_raw, min(half_size.x, half_size.y) - 0.1);
    radius = max(radius, 1.0);

    let d = sd_round_box(p, half_size, radius);

    let inner = max(half_size - vec2<f32>(radius), vec2<f32>(0.0001));
    let lh = 2.0 * inner.x;
    let lv = 2.0 * inner.y;
    let perimeter = 2.0 * (lh + lv) + TWO_PI * radius;

    let s_param = perimeter_s(p, half_size, radius);
    let u_coord = fract(s_param / perimeter);
    let v_coord = 0.5 + d / (thickness * 2.0);

    let core_mask = 1.0 - smoothstep(thickness, thickness * 2.0, abs(d));
    let smoke_mask = 1.0 - smoothstep(smoke_w, smoke_w * 3.2, abs(d));
    let ff = smoothstep(-0.15, 0.25, -p_norm.y);

    let overlap_px = 10.0;
    let seam_width = clamp(overlap_px / max(perimeter, 1.0), 0.001, 0.20);
    let seam_blend = smoothstep(0.0, seam_width, u_coord) * smoothstep(0.0, seam_width, 1.0 - u_coord);

    let uv_a = vec2<f32>(u_coord + 1.30, v_coord);
    let uv_a2 = vec2<f32>(u_coord + 1.90, 1.0 - v_coord);
    let a1 = color_from_grad(shade(uv_a, t)) * ff;
    let a2 = color_from_grad(shade(uv_a2, t)) * (1.0 - ff);
    let flame_a = a1 + a2;

    let u_b = u_coord + 1.0;
    let uv_b = vec2<f32>(u_b + 1.30, v_coord);
    let uv_b2 = vec2<f32>(u_b + 1.90, 1.0 - v_coord);
    let b1 = color_from_grad(shade(uv_b, t)) * ff;
    let b2 = color_from_grad(shade(uv_b2, t)) * (1.0 - ff);
    let flame_b = b1 + b2;

    let flame = mix(flame_b, flame_a, seam_blend) * intensity;
    let smoke_tinted = mix(
        flame,
        flame * vec3<f32>(0.50, 0.72, 1.35) + vec3<f32>(0.00, 0.02, 0.10),
        smoke_blue_tint
    );

    var col = flame * core_mask * max(core_scale, 0.0);
    col = col + smoke_tinted * (0.55 * max(smoke_opacity, 0.0)) * smoke_mask;
    col = col * smoke_mask;

    let crisp_edge = smoothstep(thickness * 0.85, thickness * 0.10, abs(d));
    col = col + flame * crisp_edge * thin_mode * 0.55;

    let tint = vec3<f32>(get_float(13u), get_float(14u), get_float(15u));
    col = col * tint;

    var alpha = clamp(max(max(col.r, col.g), col.b), 0.0, 1.0);
    alpha = max(alpha * 0.95, core_mask * 0.35);
    let halo = vec4<f32>(col, alpha);

    let base = textureSample(input_texture, input_sampler, uv_screen);
    let out_a = base.a + halo.a * (1.0 - base.a);
    let out_rgb = base.rgb + halo.rgb * (1.0 - base.a);
    return vec4<f32>(out_rgb, out_a);
}}
"#,
                preamble = HERO_FIRE_WGSL_PREAMBLE
            ))
        })
        .clone()
}

struct FireShaderParams {
    resolution_w: f32,
    resolution_h: f32,
    time: f32,
    band_width: f32,
    corner_radius: f32,
    contour_w: f32,
    contour_h: f32,
    smoke_scale: f32,
    intensity: f32,
    smoke_opacity: f32,
    core_scale: f32,
    color_tint: [f32; 3],
}

fn fire_shader_effect(p: &FireShaderParams) -> RenderEffect {
    let mut shader = RuntimeShader::from_shared_source(hero_fire_wgsl());
    shader.set_float2(0, p.resolution_w, p.resolution_h);
    shader.set_float(2, p.time);
    shader.set_float(3, p.band_width);
    shader.set_float(4, p.corner_radius);
    shader.set_float2(5, p.contour_w, p.contour_h);
    shader.set_float(7, p.smoke_scale);
    shader.set_float(8, p.intensity);
    shader.set_float(9, p.smoke_opacity);
    shader.set_float(10, p.core_scale);
    shader.set_float(11, 0.0);
    shader.set_float(12, 0.0);
    shader.set_float(13, p.color_tint[0]);
    shader.set_float(14, p.color_tint[1]);
    shader.set_float(15, p.color_tint[2]);
    RenderEffect::runtime_shader(shader)
}

/// A transparent fire-border overlay of an exact size. Sized with fill_max_width
/// plus an explicit (measured) height, it renders correctly and matches the
/// element it is stacked over — so it never changes layout and hugs the bounds.
///
/// The animation frame is read as `time.get()` *inside* the graphics_layer
/// closure. That closure runs under `observe_draw_reads`, so the read subscribes
/// the draw node directly: when the infinite transition ticks, the runtime
/// schedules a draw-repass for just this layer and re-renders it — no
/// recomposition needed. Reading the frame in the composable body instead only
/// subscribes the recompose scope, which is throttled/cached when the element
/// lives inside a scroll container (that was the "animates only while scrolling"
/// bug). This mirrors how the cranpose desktop-demo's shader rect animates.
#[composable]
fn FireBorderOverlay(corner_radius: f32, height: f32) {
    let transition = rememberInfiniteTransition("fire_border");
    let time = transition.animateFloat(
        0.0,
        1.0,
        infiniteRepeatable(
            AnimationSpec::linear(60_000),
            RepeatMode::Restart,
            StartOffset::default(),
        ),
        "fire_border_t",
    );
    // Read the frame in the BODY so this composable recomposes each tick (which
    // keeps the frame flowing alongside AnimationPump).
    let frame = time.value();
    let height = height.max(1.0);
    // How far the flame overhangs the element on every side. The layer texture is
    // grown by this much so the outward glow has room and is not clipped.
    let pad = 13.0f32;
    BoxWithConstraints(
        Modifier::empty().fill_max_width().height(height),
        move |scope| {
            let w = scope.max_width().0.max(1.0);
            let outer_w = w + 2.0 * pad;
            let outer_h = height + 2.0 * pad;
            // Force the shader to re-render every frame. Cranpose caches the raster
            // of a scroll's content, keyed on a content hash that excludes shader
            // uniforms (see render_hash.rs) — so an animated shader inside a scroll
            // reuses its cached (frozen) pixels. The content hash DOES include the
            // layer's alpha (graph_hash.rs), so nudging alpha by a hair each frame
            // forces a cache miss and a real re-raster with the new time. Unlike a
            // bounds nudge this costs no relayout, and the change is far below
            // perceptible (~0.3% opacity).
            let anim_alpha = 1.0 - (frame * 240.0).fract() * 0.003;
            // `required_size` makes the flame layer bigger than the element and
            // lets it overflow, while the parent is still reported the element-sized
            // (coerced) bounds — so the flame sits OUTSIDE the element edge without
            // shifting layout. cranpose places the oversized child at the parent's
            // top-left, overflowing to the bottom-right (alignment/offset can't move
            // it — they act on the coerced size, which already equals the parent).
            // So we recentre the overhang with a render-time `translation` of -pad on
            // the graphics_layer, which shifts the drawn pixels without touching
            // layout. The contour is the element bounds, centred in the larger
            // texture, so the band straddles the edge and its glow bleeds outward.
            ComposeBox(
                Modifier::empty()
                    .required_size(Size::new(outer_w, outer_h))
                    .graphics_layer(move || GraphicsLayer {
                        render_effect: Some(fire_shader_effect(&FireShaderParams {
                            resolution_w: outer_w,
                            resolution_h: outer_h,
                            time: frame,
                            band_width: 14.0,
                            corner_radius,
                            contour_w: w,
                            contour_h: height,
                            smoke_scale: 0.5,
                            intensity: 1.15,
                            smoke_opacity: 0.5,
                            core_scale: 1.3,
                            color_tint: [1.3, 0.7, 0.25],
                        })),
                        alpha: anim_alpha,
                        translation_x: -pad,
                        translation_y: -pad,
                        compositing_strategy: CompositingStrategy::Offscreen,
                        ..Default::default()
                    }),
                BoxSpec::default(),
                || {},
            );
        },
    );
}

/// A zero-size, invisible composable that drives continuous rendering so the
/// shader-flame animations never freeze. Each frame it bumps a state that it
/// reads itself, invalidating only this leaf scope. That keeps the desktop event
/// loop's `should_render()` true (→ `ControlFlow::Poll`), so the frame clock the
/// flames animate off of keeps ticking even when the app is otherwise idle. The
/// recompose is confined to this one leaf; the rest of the tree is untouched.
#[composable]
fn AnimationPump() {
    let tick = rememberMutableStateOf(|| 0u64);
    // Read the tick so this scope subscribes and recomposes when it changes.
    let _ = tick.value();
    cranpose_core::LaunchedEffectAsync!((), move |scope| {
        Box::pin(async move {
            let clock = scope.runtime().frame_clock();
            let mut n = 0u64;
            loop {
                if !scope.is_active() {
                    break;
                }
                clock.next_frame().await;
                n = n.wrapping_add(1);
                tick.set(n);
            }
        })
    });
}

/// The hero "next action" button to render inside the flame border.
#[derive(Clone, Copy, PartialEq)]
enum HeroButton {
    Field(EditorFieldId),
    Action(ActionButtonId),
}

/// Wraps the hero "next action" button in an animated flame border. The flame is
/// a transparent overlay stacked over the measured button, so it never changes
/// layout and always hugs the button's bounds. Used for both the action CTA and
/// the field-edit suggestion, in every hero layout.
#[composable]
fn hero_fire_glow(
    hero: HeroButton,
    content_height: f32,
    corner_radius: f32,
    fields: EditorFields,
    actions: ActionStates,
    theme: ThemeMode,
) {
    let _ = content_height;
    let button_h = rememberMutableStateOf(|| 0.0f32);
    ComposeBox(
        Modifier::empty().fill_max_width(),
        BoxSpec::default().content_alignment(Alignment::TOP_START),
        {
            let fields = fields.clone();
            move || {
                let fields = fields.clone();
                ComposeBox(
                    Modifier::empty().fill_max_width().draw_behind(move |draw| {
                        let h = draw.size().height;
                        if (button_h.get_non_reactive() - h).abs() > 0.5 {
                            button_h.set(h);
                        }
                    }),
                    BoxSpec::default(),
                    move || match hero {
                        HeroButton::Field(field) => FieldSuggestion(
                            field,
                            actions.active_queue_target,
                            actions.status,
                            theme,
                        ),
                        HeroButton::Action(action) => {
                            focus_action_button(action, fields.clone(), actions, theme)
                        }
                    },
                );
                let h = button_h.value();
                if h > 1.0 {
                    FireBorderOverlay(corner_radius, h);
                }
            }
        },
    );
}

#[composable]
fn NextWorkPanel(
    next_item: NextWorkItem,
    title_override: Option<String>,
    skip_key: String,
    fields: EditorFields,
    actions: ActionStates,
    theme: ThemeMode,
    compact: bool,
) {
    let ActionStates {
        status,
        skipped_queue,
        ..
    } = actions;
    let modifier = if compact {
        Modifier::empty().fill_max_width()
    } else {
        Modifier::empty().weight(1.0)
    };
    let title = title_override.unwrap_or_else(|| next_item.title());
    glass_panel(modifier, theme, 18.0, 18.0, {
        let fields = fields.clone();
        let title = title.clone();
        let skip_key = skip_key.clone();
        move || {
            Row(
                Modifier::empty().fill_max_width(),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(18.0)),
                {
                    let fields = fields.clone();
                    let row_title = title.clone();
                    let skip_key = skip_key.clone();
                    move || {
                        HeroTile(next_item.stage(), theme);
                        Column(
                            Modifier::empty().weight(1.0),
                            ColumnSpec::default()
                                .vertical_arrangement(LinearArrangement::spaced_by(12.0)),
                            {
                                let fields = fields.clone();
                                let title = row_title.clone();
                                let skip_key = skip_key.clone();
                                move || {
                                    Row(
                                        Modifier::empty().fill_max_width(),
                                        RowSpec::default()
                                            .horizontal_arrangement(LinearArrangement::SpaceBetween)
                                            .vertical_alignment(
                                                VerticalAlignment::CenterVertically,
                                            ),
                                        {
                                            let skip_key = skip_key.clone();
                                            let skip_label = title.clone();
                                            move || {
                                                Text(
                                                    "Now",
                                                    Modifier::empty(),
                                                    eyebrow_style(theme),
                                                );
                                                Row(
                                                    Modifier::empty(),
                                                    RowSpec::default()
                                                        .horizontal_arrangement(
                                                            LinearArrangement::spaced_by(10.0),
                                                        )
                                                        .vertical_alignment(
                                                            VerticalAlignment::CenterVertically,
                                                        ),
                                                    {
                                                        let skip_key = skip_key.clone();
                                                        let skip_label = skip_label.clone();
                                                        move || {
                                                            Text(
                                                                next_item.stage().label(),
                                                                Modifier::empty(),
                                                                stage_label_style(theme),
                                                            );
                                                            skip_work_button(
                                                                skip_key.clone(),
                                                                skip_label.clone(),
                                                                skipped_queue,
                                                                status,
                                                                theme,
                                                            );
                                                        }
                                                    },
                                                );
                                            }
                                        },
                                    );
                                    Text(
                                        title.clone(),
                                        Modifier::empty(),
                                        heading_style(21.0, theme),
                                    );
                                    match next_item {
                                        NextWorkItem::Field(field) => {
                                            hero_fire_glow(
                                                HeroButton::Field(field),
                                                48.0,
                                                11.0,
                                                fields.clone(),
                                                actions,
                                                theme,
                                            );
                                        }
                                        NextWorkItem::Action(action) => {
                                            hero_fire_glow(
                                                HeroButton::Action(action),
                                                64.0,
                                                14.0,
                                                fields.clone(),
                                                actions,
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
    actions: ActionStates,
    theme: ThemeMode,
) {
    let ActionStates {
        status,
        ui_preferences,
        active_queue_target,
        ..
    } = actions;
    if queue.is_empty() {
        glass_panel(
            Modifier::empty().fill_max_width(),
            theme,
            14.0,
            10.0,
            move || {
                Column(
                    Modifier::empty().fill_max_width(),
                    ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(6.0)),
                    move || {
                        Text(
                            "Saved Queue (replays on launch)",
                            Modifier::empty(),
                            eyebrow_style(theme),
                        );
                        Text(
                            "No saved queue yet. Build one below in New Actions Queue, then press Remember queue.",
                            Modifier::empty(),
                            muted_style(theme),
                        );
                    },
                );
            },
        );
        return;
    }

    let scroll_state = remember(|| ScrollState::new(0.0)).with(|state| *state);
    let scroll_retry = rememberMutableStateOf(|| 0u64);
    let last_auto_scroll_key = rememberMutableStateOf(|| None::<String>);
    glass_panel(Modifier::empty().fill_max_width(), theme, 14.0, 10.0, {
        let fields = fields.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(9.0)),
                {
                    let fields = fields.clone();
                    let queue = queue.clone();
                    let current_queue = ui_preferences.value().interactive_queue().to_vec();
                    let selected_key = interactive_queue_selected_key(
                        &queue,
                        &current_queue,
                        active_queue_target.value().as_deref(),
                    );
                    let scroll_state = scroll_state;
                    move || {
                        Text(
                            "Saved Queue (replays on launch)",
                            Modifier::empty(),
                            eyebrow_style(theme),
                        );
                        BoxWithConstraints(Modifier::empty().fill_max_width(), {
                            let fields = fields.clone();
                            let row_queue = queue.clone();
                            let row_current_queue = current_queue.clone();
                            let selected_key = selected_key.clone();
                            let scroll_state = scroll_state;
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
                                    let scroll_state = scroll_state;
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
                                                scroll_retry,
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
                                        .height(60.0)
                                        .clip_to_bounds()
                                        .horizontal_scroll(scroll_state, false),
                                    RowSpec::default()
                                        .horizontal_arrangement(LinearArrangement::spaced_by(
                                            INTERACTIVE_QUEUE_CHIP_GAP,
                                        ))
                                        .vertical_alignment(VerticalAlignment::CenterVertically),
                                    {
                                        let fields = fields.clone();
                                        let row_queue = row_queue.clone();
                                        let row_current_queue = row_current_queue.clone();
                                        let selected_key = selected_key.clone();
                                        move || {
                                            for item_key in &row_queue {
                                                InteractiveQueueChip(
                                                    item_key.clone(),
                                                    row_current_queue.contains(item_key),
                                                    selected_key.as_deref() == Some(item_key),
                                                    fields.clone(),
                                                    actions,
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
                            status,
                            ui_preferences,
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
                move || {
                    Text(
                        "Current",
                        Modifier::empty(),
                        queue_current_label_style(theme),
                    );
                    QueueCurrentEditorField(field, fields.clone(), status, ui_preferences, theme);
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
    if field == EditorFieldId::Difficulty {
        DifficultyField(state, saved_text, status, ui_preferences, theme);
        return;
    }
    let (min_lines, max_lines) = field.queue_line_bounds();
    labeled_field(
        FieldSpec {
            label: field.label(),
            field_id: field.field_id(),
            state,
            saved_text,
            min_lines,
            max_lines,
            allow_paste: true,
            code: field.is_code(),
        },
        status,
        ui_preferences,
        theme,
    );
}

/// Editable row of the actions taken this session. Chips can be reordered and
/// removed; "Remember queue" saves the curated list as the replay queue.
#[composable]
fn SessionQueuePanel(actions: ActionStates, theme: ThemeMode) {
    let ActionStates {
        status,
        ui_preferences,
        ..
    } = actions;
    let session_queue = ui_preferences.value().interactive_queue().to_vec();
    // Shared drag state: Some((dragged_index, horizontal_offset_px)).
    let drag = rememberMutableStateOf(|| None::<(usize, f32)>);
    let stride = SESSION_CHIP_W + INTERACTIVE_QUEUE_CHIP_GAP;
    // Horizontal scroll for long queues. The scroll modifier's own gesture is
    // disabled (guard returns false); we drive the offset ourselves from the one
    // pointer handler so wheel-pan and drag-reorder never fight each other.
    let scroll_state = remember(|| ScrollState::new(0.0)).with(|state| *state);
    glass_panel(Modifier::empty().fill_max_width(), theme, 14.0, 10.0, {
        let session_queue = session_queue.clone();
        move || {
            let session_queue = session_queue.clone();
            let scroll_state = scroll_state;
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(9.0)),
                move || {
                    let session_queue = session_queue.clone();
                    let scroll_state = scroll_state;
                    Row(
                        Modifier::empty().fill_max_width(),
                        RowSpec::default()
                            .horizontal_arrangement(LinearArrangement::SpaceBetween)
                            .vertical_alignment(VerticalAlignment::CenterVertically),
                        {
                            let queue_len = session_queue.len();
                            move || {
                                Text("New Actions Queue", Modifier::empty(), eyebrow_style(theme));
                                remember_queue_button(queue_len, status, ui_preferences, theme);
                            }
                        },
                    );
                    if session_queue.is_empty() {
                        Text(
                            "Actions you take collect here. Drag chips to reorder, then press Remember to save them as your replay queue.",
                            Modifier::empty(),
                            muted_style(theme),
                        );
                    } else {
                        let len = session_queue.len();
                        // A single pointer handler on the full-width container owns
                        // both gestures: a click-drag reorders chips (live), and the
                        // mouse wheel pans the row when it overflows. Because the
                        // container spans the whole range, the pointer never slips off
                        // it mid-drag (a per-chip handler froze once the chip
                        // translated out from under the cursor).
                        ComposeBox(
                            Modifier::empty()
                                .fill_max_width()
                                .height(48.0)
                                .clip_to_bounds()
                                .pointer_input(len, {
                                    move |scope: PointerInputScope| {
                                        let scroll_state = scroll_state;
                                        async move {
                                            scope
                                                .await_pointer_event_scope(
                                                    move |await_scope| async move {
                                                        let mut idx = 0usize;
                                                        let mut start_x = 0.0f32;
                                                        let mut dragging = false;
                                                        // Only an actual press starts a
                                                        // drag; bare hover moves must
                                                        // never touch a chip.
                                                        let mut pressed = false;
                                                        loop {
                                                            let event = await_scope
                                                                .await_pointer_event()
                                                                .await;
                                                            match event.kind {
                                                                PointerEventKind::Scroll => {
                                                                    // Map either axis of the
                                                                    // wheel to horizontal pan
                                                                    // (a vertical wheel emits
                                                                    // scroll_delta.y).
                                                                    let d = event.scroll_delta.x
                                                                        + event.scroll_delta.y;
                                                                    if d.abs() > f32::EPSILON {
                                                                        scroll_state
                                                                            .dispatch_raw_delta(-d);
                                                                        event.consume();
                                                                    }
                                                                }
                                                                PointerEventKind::Down => {
                                                                    let start_scroll = scroll_state
                                                                        .value_non_reactive();
                                                                    let content_x =
                                                                        event.position.x
                                                                            + start_scroll;
                                                                    idx = ((content_x / stride)
                                                                        .floor()
                                                                        as usize)
                                                                        .min(len.saturating_sub(1));
                                                                    start_x =
                                                                        event.global_position.x;
                                                                    dragging = false;
                                                                    pressed = true;
                                                                }
                                                                PointerEventKind::Move => {
                                                                    // Guard against a stale
                                                                    // press (missed Up) and
                                                                    // bare hover.
                                                                    if !pressed
                                                                        || event.buttons
                                                                            == PointerButtons::NONE
                                                                    {
                                                                        continue;
                                                                    }
                                                                    let dx =
                                                                        event.global_position.x
                                                                            - start_x;
                                                                    if dx.abs() > 6.0 {
                                                                        dragging = true;
                                                                    }
                                                                    if dragging {
                                                                        drag.set(Some((idx, dx)));
                                                                        event.consume();
                                                                    }
                                                                }
                                                                PointerEventKind::Up
                                                                | PointerEventKind::Cancel => {
                                                                    pressed = false;
                                                                    if dragging {
                                                                        let dx =
                                                                            event.global_position.x
                                                                                - start_x;
                                                                        let delta = (dx / stride)
                                                                            .round()
                                                                            as i64;
                                                                        let target = (idx as i64
                                                                            + delta)
                                                                            .clamp(
                                                                                0,
                                                                                len as i64 - 1,
                                                                            )
                                                                            as usize;
                                                                        if target != idx {
                                                                            session_queue_move(
                                                                                ui_preferences,
                                                                                idx,
                                                                                target,
                                                                            );
                                                                        }
                                                                    }
                                                                    drag.set(None);
                                                                    dragging = false;
                                                                }
                                                                _ => {}
                                                            }
                                                        }
                                                    },
                                                )
                                                .await;
                                        }
                                    }
                                }),
                            BoxSpec::default(),
                            {
                                let session_queue = session_queue.clone();
                                move || {
                                    let session_queue = session_queue.clone();
                                    Row(
                                        Modifier::empty()
                                            .fill_max_width()
                                            // `horizontal_scroll_guarded(.., || false)` used to keep
                                            // this Row's ScrollState-driven layout/clip/max-value
                                            // tracking while permanently disabling the modifier's own
                                            // pointer-driven drag/wheel gesture, so the single
                                            // `pointer_input` above (drag-reorder + wheel-pan) was the
                                            // only thing that ever called `scroll_state.scroll_to`.
                                            // cranpose-ui 0.1.106 removed the guarded scroll variants
                                            // (no caller inside the Cranpose repo itself) and kept no
                                            // public equivalent — `horizontal_scroll`/`vertical_scroll`
                                            // always attach their built-in gesture, and there is no
                                            // public way to get the layout/measurement side alone.
                                            // Falling back to plain `horizontal_scroll` here is
                                            // therefore not behavior-preserving: this Row can now also
                                            // react to raw drags as a scroll gesture, competing with
                                            // the reorder drag above exactly like the removed guard was
                                            // written to prevent. No equivalent exists upstream as of
                                            // 0.1.106; needs manual on-device verification of
                                            // drag-reorder vs. drag-to-scroll before this ships.
                                            .horizontal_scroll(scroll_state, false),
                                        RowSpec::default()
                                            .horizontal_arrangement(LinearArrangement::spaced_by(
                                                INTERACTIVE_QUEUE_CHIP_GAP,
                                            ))
                                            .vertical_alignment(
                                                VerticalAlignment::CenterVertically,
                                            ),
                                        move || {
                                            for (index, item_key) in
                                                session_queue.iter().enumerate()
                                            {
                                                SessionQueueChip(
                                                    index,
                                                    len,
                                                    item_key.clone(),
                                                    drag,
                                                    status,
                                                    ui_preferences,
                                                    theme,
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
}

/// Fixed chip width; a constant width lets the container drag handler map a
/// pointer x-position to a chip index.
const SESSION_CHIP_W: f32 = 210.0;

#[composable]
fn SessionQueueChip(
    index: usize,
    len: usize,
    item_key: String,
    drag: MutableState<Option<(usize, f32)>>,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let label = interactive_queue_label(&item_key, false, false);
    let stride = SESSION_CHIP_W + INTERACTIVE_QUEUE_CHIP_GAP;
    // Live reorder: while a chip is dragged the others slide to open a gap so the
    // list always shows its would-be final arrangement (never a hole, never a
    // fly). `from` is the grabbed index; `target` is where it would drop right now.
    let (from, drag_dx, active) = match drag.value() {
        Some((i, dx)) => (i, dx, true),
        None => (usize::MAX, 0.0, false),
    };
    let target = if active {
        (from as i64 + (drag_dx / stride).round() as i64).clamp(0, len as i64 - 1) as usize
    } else {
        from
    };
    let is_dragged = active && index == from;
    // Shift every chip between the grabbed slot and the target by one stride to
    // make room; the grabbed chip itself tracks the pointer directly. The shift is
    // instant (no spring): the offset is relative to the chip's laid-out position,
    // which jumps by a stride the moment the list reorders on drop — an animated
    // offset would lag that jump and produce a one-stride "pop". Instant offsets
    // stay frame-coherent, so the drop is seamless.
    let neighbor_shift = if !active || is_dragged {
        0.0
    } else if from < target && index > from && index <= target {
        -stride
    } else if target < from && index >= target && index < from {
        stride
    } else {
        0.0
    };
    let offset = if is_dragged { drag_dx } else { neighbor_shift };
    // Flat, fixed-width rounded surface. While dragged it follows the pointer
    // and lifts slightly.
    let mut surface = Modifier::empty()
        .width(SESSION_CHIP_W)
        .background(soft_button_surface(theme))
        .rounded_corners(9.0);
    if offset.abs() > 0.01 || is_dragged {
        surface = surface.graphics_layer_block(move |layer| {
            layer.translation_x = offset;
            if is_dragged {
                // A gentle lift while dragged. No shadow_elevation: the row clips to
                // its bounds, and a clipped drop shadow looked broken here.
                layer.scale_x = 1.04;
                layer.scale_y = 1.04;
            }
        });
    }
    ComposeBox(
        surface.padding_symmetric(8.0, 5.0),
        BoxSpec::default(),
        move || {
            let label = label.clone();
            Row(
                Modifier::empty().fill_max_width(),
                RowSpec::default()
                    .horizontal_arrangement(LinearArrangement::spaced_by(5.0))
                    .vertical_alignment(VerticalAlignment::CenterVertically),
                move || {
                    Text("⠿", Modifier::empty(), muted_style(theme));
                    BasicText(
                        label.clone(),
                        Modifier::empty().weight(1.0),
                        queue_text_style(theme),
                        TextOverflow::Ellipsis,
                        false,
                        1,
                        1,
                    );
                    queue_edit_button("✕", theme, move || {
                        session_queue_remove(ui_preferences, index, status);
                    });
                },
            );
        },
    );
}

#[composable]
fn remember_queue_button(
    count: usize,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let enabled = count > 0;
    let (button_spec, press) = animated_button_spec(enabled, "remember_queue_button_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty(),
            theme,
            enabled,
            false,
            if enabled {
                button_surface(theme)
            } else {
                soft_button_surface(theme)
            },
            9.0,
            press,
        )
        .padding_symmetric(13.0, 8.0),
        button_spec,
        move || {
            if !enabled {
                return;
            }
            remember_queue(ui_preferences, status);
        },
        move || {
            Text(
                "Remember queue",
                Modifier::empty(),
                if enabled {
                    focus_button_text_style(theme)
                } else {
                    subtle_button_text_style(theme)
                },
            );
        },
    );
}

#[composable]
fn queue_edit_button(glyph: &'static str, theme: ThemeMode, on_click: impl FnMut() + 'static) {
    let (button_spec, press) = animated_button_spec(true, "queue_edit_button_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty(),
            theme,
            true,
            false,
            soft_button_surface(theme),
            7.0,
            press,
        )
        .padding_symmetric(8.0, 5.0),
        button_spec,
        on_click,
        move || {
            Text(glyph, Modifier::empty(), subtle_button_text_style(theme));
        },
    );
}

/// Move a chip from one position to another (used by drag-and-drop reorder).
fn session_queue_move(ui_preferences: MutableState<UiPreferences>, from: usize, to: usize) {
    if from == to {
        return;
    }
    let preferences = ui_preferences.update(|preferences| {
        let mut queue = preferences.interactive_queue().to_vec();
        if from < queue.len() && to < queue.len() {
            let item = queue.remove(from);
            queue.insert(to, item);
        }
        preferences.set_interactive_queue(queue);
        preferences.clone()
    });
    let _ = persist_ui_preferences(&preferences);
}

fn session_queue_remove(
    ui_preferences: MutableState<UiPreferences>,
    index: usize,
    status: MutableState<String>,
) {
    let preferences = ui_preferences.update(|preferences| {
        let mut queue = preferences.interactive_queue().to_vec();
        if index < queue.len() {
            queue.remove(index);
        }
        preferences.set_interactive_queue(queue);
        preferences.clone()
    });
    let _ = persist_ui_preferences(&preferences);
    status.set("Removed a step from the new actions queue.".to_string());
}

fn remember_queue(ui_preferences: MutableState<UiPreferences>, status: MutableState<String>) {
    let preferences = ui_preferences.update(|preferences| {
        preferences.remember_current_queue();
        preferences.clone()
    });
    let count = preferences.remembered_queue().len();
    match persist_ui_preferences(&preferences) {
        Ok(_) => status.set(format!(
            "Remembered {count} step(s) — this becomes the replay queue on next launch."
        )),
        Err(error) => status.set(format!("Saving the queue failed: {error}")),
    }
}

#[composable]
fn InteractiveQueueChip(
    item_key: String,
    done: bool,
    glow: bool,
    fields: EditorFields,
    actions: ActionStates,
    theme: ThemeMode,
) {
    if glow {
        // Fixed-size chip: the flame is a transparent overlay stacked exactly over
        // the chip's 214×48 bounds, so it never shifts the row and hugs the edge.
        ComposeBox(
            Modifier::empty().width(INTERACTIVE_QUEUE_CHIP_WIDTH),
            BoxSpec::default().content_alignment(Alignment::TOP_START),
            move || {
                interactive_queue_chip_button(
                    item_key.clone(),
                    done,
                    fields.clone(),
                    actions,
                    theme,
                );
                FireBorderOverlay(10.0, 48.0);
            },
        );
        return;
    }
    interactive_queue_chip_button(item_key, done, fields, actions, theme);
}

#[composable]
fn interactive_queue_chip_button(
    item_key: String,
    done: bool,
    fields: EditorFields,
    actions: ActionStates,
    theme: ThemeMode,
) {
    let busy_action = actions.busy_action;
    let action = ActionButtonId::from_count_key(&item_key);
    let long_action = action.and_then(ActionButtonId::long_action);
    let action_busy = busy_action.value();
    let is_busy = long_action.is_some() && action_busy == long_action;
    let disabled = long_action.is_some() && action_busy.is_some();
    let invokes_button = queue_item_invokes_button(&item_key);
    let background = interactive_queue_surface(theme, done, disabled, is_busy, invokes_button);
    let (button_spec, press) = animated_button_spec(!disabled, "interactive_queue_chip_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty()
                .width(INTERACTIVE_QUEUE_CHIP_WIDTH)
                .height(48.0),
            theme,
            !disabled,
            done || is_busy,
            background,
            10.0,
            press,
        )
        .padding_symmetric(10.0, 8.0),
        button_spec,
        {
            let item_key = item_key.clone();
            move || {
                if disabled {
                    return;
                }
                handle_interactive_queue_press(&item_key, fields.clone(), actions, theme);
            }
        },
        move || {
            interactive_queue_content(
                interactive_queue_icon(&item_key),
                interactive_queue_label(&item_key, done, is_busy),
                interactive_queue_text_style(theme, done, disabled, is_busy),
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
            ButtonActivityIndicator(theme, busy);
        },
    );
}

#[composable]
fn ActionButtons(
    fields: EditorFields,
    session: EditorSession,
    actions: ActionStates,
    theme: ThemeMode,
    next_action: Option<ActionButtonId>,
) {
    let ordered_actions = ordered_action_buttons(&session.layout_preferences);
    BoxWithConstraints(Modifier::empty().fill_max_width(), {
        let fields = fields.clone();
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
                    let ordered_actions = ordered_actions.clone();
                    move || {
                        for row in ordered_actions.chunks(columns) {
                            let row_actions = row.to_vec();
                            let fields = fields.clone();
                            Row(
                                Modifier::empty().fill_max_width(),
                                RowSpec::default()
                                    .horizontal_arrangement(LinearArrangement::spaced_by(12.0)),
                                move || {
                                    let fields = fields.clone();
                                    ForEach(&row_actions, move |action| {
                                        ActionButton(
                                            *action,
                                            fields.clone(),
                                            actions,
                                            theme,
                                            next_action,
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
    actions: ActionStates,
    theme: ThemeMode,
    next_action: Option<ActionButtonId>,
) {
    let action_busy = actions.busy_action.value();
    let long_action = action.long_action();
    let is_busy = long_action.is_some() && action_busy == long_action;
    let disabled = long_action.is_some() && action_busy.is_some();
    // The action currently recommended by the hero glows here too, so the "do
    // this next" cue follows the button wherever it appears.
    if next_action == Some(action) {
        glow_grid_button(action, disabled, is_busy, fields, actions, theme);
        return;
    }
    primary_button(
        action,
        actions.ui_preferences,
        theme,
        disabled,
        is_busy,
        false,
        move || {
            handle_action_button(action, fields.clone(), actions);
        },
    );
}

/// A Quick Actions grid button wrapped in the same fire-shader glow as the hero,
/// used to highlight the current recommended action in the grid.
#[composable]
fn glow_grid_button(
    action: ActionButtonId,
    disabled: bool,
    is_busy: bool,
    fields: EditorFields,
    actions: ActionStates,
    theme: ThemeMode,
) {
    let button_h = rememberMutableStateOf(|| 0.0f32);
    // The WEIGHTED element is this outer ComposeBox, which participates in the
    // Row's weight distribution just like the non-glow buttons (which use
    // `.weight(1.0)`). The flame is a transparent overlay stacked on top of the
    // measured button, so it never changes the cell size or shifts neighbours.
    ComposeBox(
        Modifier::empty().weight(1.0),
        BoxSpec::default().content_alignment(Alignment::TOP_START),
        {
            let fields = fields.clone();
            move || {
                let fields = fields.clone();
                ComposeBox(
                    Modifier::empty().fill_max_width().draw_behind(move |draw| {
                        let h = draw.size().height;
                        if (button_h.get_non_reactive() - h).abs() > 0.5 {
                            button_h.set(h);
                        }
                    }),
                    BoxSpec::default(),
                    move || {
                        let fields = fields.clone();
                        primary_button(
                            action,
                            actions.ui_preferences,
                            theme,
                            disabled,
                            is_busy,
                            true,
                            move || {
                                handle_action_button(action, fields.clone(), actions);
                            },
                        );
                    },
                );
                let h = button_h.value();
                if h > 1.0 {
                    FireBorderOverlay(10.0, h);
                }
            }
        },
    );
}

#[composable]
fn focus_action_button(
    action: ActionButtonId,
    fields: EditorFields,
    actions: ActionStates,
    theme: ThemeMode,
) {
    let ui_preferences = actions.ui_preferences;
    let action_busy = actions.busy_action.value();
    let long_action = action.long_action();
    let is_busy = long_action.is_some() && action_busy == long_action;
    let disabled = long_action.is_some() && action_busy.is_some();
    let count_key = action.count_key().to_string();
    let count = ui_preferences.value().button_count(&count_key);
    let background = if is_busy {
        button_surface(theme).with_alpha(0.86)
    } else if disabled {
        disabled_button_surface(theme)
    } else {
        button_surface(theme)
    };
    let style = if disabled {
        disabled_button_text_style(theme)
    } else {
        focus_button_text_style(theme)
    };
    let (button_spec, press) = animated_button_spec(!disabled, "focus_action_button_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty().fill_max_width(),
            theme,
            !disabled,
            is_busy,
            background,
            14.0,
            press,
        )
        .height(64.0)
        .padding_symmetric(14.0, 16.0),
        button_spec,
        move || {
            if disabled {
                return;
            }
            record_button_press(ui_preferences, &count_key);
            handle_action_button(action, fields.clone(), actions);
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
    actions: ActionStates,
    theme: ThemeMode,
) {
    let ActionStates {
        status,
        ui_preferences,
        active_queue_target,
        ..
    } = actions;
    if let Some(action) = ActionButtonId::from_count_key(item_key) {
        record_button_press(ui_preferences, item_key);
        handle_action_button(action, fields, actions);
        return;
    }

    if item_key == "theme.toggle" {
        record_button_press(ui_preferences, item_key);
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
        EditorFieldId::Date => fields.date,
        EditorFieldId::ProblemTitle => fields.problem_title,
        EditorFieldId::ProblemUrl => fields.problem_url,
        EditorFieldId::Difficulty => fields.difficulty,
        EditorFieldId::SubstackUrl => fields.substack_url,
        EditorFieldId::YoutubeUrl => fields.youtube_url,
        EditorFieldId::ReferenceUrl => fields.reference_url,
        EditorFieldId::TelegramText => fields.telegram_text,
        EditorFieldId::ProblemTldr => fields.problem_tldr,
        EditorFieldId::Intuition => fields.intuition,
        EditorFieldId::Approach => fields.approach,
        EditorFieldId::TimeComplexity => fields.time_complexity,
        EditorFieldId::SpaceComplexity => fields.space_complexity,
        EditorFieldId::KotlinRuntimeMs => fields.kotlin_runtime_ms,
        EditorFieldId::KotlinCode => fields.kotlin_code,
        EditorFieldId::RustRuntimeMs => fields.rust_runtime_ms,
        EditorFieldId::RustCode => fields.rust_code,
    }
}

fn handle_action_button(action: ActionButtonId, fields: EditorFields, actions: ActionStates) {
    let status = actions.status;
    let draft = PostDraft::from_fields(&fields);
    if let Some(long_action) = action.long_action() {
        enqueue_long_action(
            long_action,
            draft,
            actions.telegram_post_link.value(),
            actions,
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
        | ActionButtonId::SaveRasterWebp
        | ActionButtonId::PublishBlog
        | ActionButtonId::PostTelegram
        | ActionButtonId::PostTelegramComment => {}
    }
}

fn enqueue_long_action(
    action: LongAction,
    draft: PostDraft,
    telegram_post_link: String,
    actions: ActionStates,
) {
    let ActionStates {
        status,
        pending_action,
        action_request_counter,
        busy_action,
        ..
    } = actions;
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
            Self::CopyLeetcode => UiIcon::Code,
            Self::CopyYoutube => UiIcon::Youtube,
            Self::CopyBlog => UiIcon::Document,
            Self::CopyTelegram => UiIcon::Telegram,
            Self::CopyTitle => UiIcon::Title,
            Self::CopySubtitle => UiIcon::Subtitle,
            Self::CopyRichText => UiIcon::RichText,
            Self::SaveRasterWebp => UiIcon::Save,
            Self::PublishBlog => UiIcon::Blog,
            Self::PostTelegram => UiIcon::Telegram,
            Self::PostTelegramComment => UiIcon::Comment,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::RefreshRasterPreview => "Refresh Raster",
            Self::CopyLeetcode => "Copy LeetCode",
            Self::CopyYoutube => "Copy YouTube",
            Self::CopyBlog => "Copy Blog",
            Self::CopyTelegram => "Copy Telegram",
            Self::CopyTitle => "Copy Title",
            Self::CopySubtitle => "Copy Subtitle",
            Self::CopyRichText => "Copy Rich Text",
            Self::SaveRasterWebp => "Save Raster WebP",
            Self::PublishBlog => "Publish Blog",
            Self::PostTelegram => "Post Telegram",
            Self::PostTelegramComment => "Post TG Comment",
        }
    }

    fn count_key(self) -> &'static str {
        match self {
            Self::RefreshRasterPreview => "preview.raster",
            Self::CopyLeetcode => "copy.leetcode",
            Self::CopyYoutube => "copy.youtube",
            Self::CopyBlog => "copy.blog",
            Self::CopyTelegram => "copy.telegram",
            Self::CopyTitle => "copy.title",
            Self::CopySubtitle => "copy.subtitle",
            Self::CopyRichText => "copy.rich_text",
            Self::SaveRasterWebp => "save.raster_webp",
            Self::PublishBlog => "publish.blog",
            Self::PostTelegram => "post.telegram",
            Self::PostTelegramComment => "post.telegram_comment",
        }
    }

    fn long_action(self) -> Option<LongAction> {
        match self {
            Self::RefreshRasterPreview => Some(LongAction::RefreshRasterPreview),
            #[cfg(not(target_arch = "wasm32"))]
            Self::CopyRichText => Some(LongAction::CopyRichText),
            Self::SaveRasterWebp => Some(LongAction::SaveRasterWebp),
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
            #[cfg(not(target_arch = "wasm32"))]
            Self::CopyRichText => "Copy Rich Text",
            Self::SaveRasterWebp => "Save Raster WebP",
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

/// Queue key a [`NextWorkItem`] maps to, used to match it against the skipped set.
fn next_work_item_key(item: NextWorkItem) -> String {
    match item {
        NextWorkItem::Action(action) => action.count_key().to_string(),
        NextWorkItem::Field(field) => field.component_key(),
    }
}

#[cfg(test)]
fn recommended_next_work(
    draft: &PostDraft,
    preview: &PreviewState,
    telegram_link: &str,
    preferences: &UiPreferences,
) -> NextWorkItem {
    recommended_next_work_excluding(draft, preview, telegram_link, preferences, &[])
}

/// Like [`recommended_next_work`], but skips any work item whose key is in
/// `skipped`, so the hero tile advances past items the user dismissed this session.
fn recommended_next_work_excluding(
    draft: &PostDraft,
    preview: &PreviewState,
    telegram_link: &str,
    preferences: &UiPreferences,
    skipped: &[String],
) -> NextWorkItem {
    work_queue(draft, preview, telegram_link, preferences)
        .into_iter()
        .find(|item| !skipped.contains(&next_work_item_key(*item)))
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
        EditorFieldId::SubstackUrl
        | EditorFieldId::YoutubeUrl
        | EditorFieldId::ReferenceUrl
        | EditorFieldId::TelegramText => WorkStage::Ship,
    }
}

fn action_stage(action: ActionButtonId) -> WorkStage {
    match action {
        ActionButtonId::RefreshRasterPreview | ActionButtonId::SaveRasterWebp => WorkStage::Review,
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
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
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
fn ProblemMetaCard(
    fields: EditorFields,
    session: EditorSession,
    actions: ActionStates,
    theme: ThemeMode,
    compact: bool,
    current_field: Option<EditorFieldId>,
) {
    let ActionStates {
        status,
        ui_preferences,
        ..
    } = actions;
    section_card(theme, {
        let fields = fields.clone();
        let saved_draft = session.saved_draft.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let saved_draft = saved_draft.clone();
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
                                ],
                                fields.clone(),
                                saved_draft.clone(),
                                status,
                                ui_preferences,
                                theme,
                                false,
                                current_field,
                            );
                        } else {
                            Row(
                                Modifier::empty().fill_max_width(),
                                RowSpec::default()
                                    .horizontal_arrangement(LinearArrangement::spaced_by(18.0)),
                                {
                                    let fields = fields.clone();
                                    let saved_draft = saved_draft.clone();
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
                                            status,
                                            ui_preferences,
                                            theme,
                                            true,
                                            current_field,
                                        );
                                        MetaFieldColumn(
                                            vec![
                                                EditorFieldId::SubstackUrl,
                                                EditorFieldId::Date,
                                                EditorFieldId::Difficulty,
                                            ],
                                            fields.clone(),
                                            saved_draft.clone(),
                                            status,
                                            ui_preferences,
                                            theme,
                                            true,
                                            current_field,
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
#[allow(clippy::too_many_arguments)]
fn MetaFieldColumn(
    field_ids: Vec<EditorFieldId>,
    fields: EditorFields,
    saved_draft: PostDraft,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
    weighted: bool,
    current_field: Option<EditorFieldId>,
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
                move |field| {
                    EditorField(
                        *field,
                        fields.clone(),
                        saved_draft.clone(),
                        status,
                        ui_preferences,
                        theme,
                        current_field == Some(*field),
                    );
                }
            });
        },
    );
}

#[composable]
fn WriteupCard(
    fields: EditorFields,
    session: EditorSession,
    actions: ActionStates,
    theme: ThemeMode,
    current_field: Option<EditorFieldId>,
) {
    FieldSectionCard(
        "Writeup",
        &WRITEUP_FIELDS,
        fields,
        session,
        actions,
        theme,
        current_field,
    );
}

#[composable]
fn CodeCard(
    fields: EditorFields,
    session: EditorSession,
    actions: ActionStates,
    theme: ThemeMode,
    current_field: Option<EditorFieldId>,
) {
    FieldSectionCard(
        "Code Blocks",
        &CODE_FIELDS,
        fields,
        session,
        actions,
        theme,
        current_field,
    );
}

#[composable]
fn FieldSectionCard(
    title: &'static str,
    section_fields: &'static [EditorFieldId],
    fields: EditorFields,
    session: EditorSession,
    actions: ActionStates,
    theme: ThemeMode,
    current_field: Option<EditorFieldId>,
) {
    let ActionStates {
        status,
        ui_preferences,
        ..
    } = actions;
    section_card(theme, {
        let fields = fields.clone();
        let saved_draft = session.saved_draft.clone();
        let layout_preferences = session.layout_preferences.clone();
        move || {
            Column(
                Modifier::empty().fill_max_width(),
                ColumnSpec::default().vertical_arrangement(LinearArrangement::spaced_by(14.0)),
                {
                    let fields = fields.clone();
                    let saved_draft = saved_draft.clone();
                    let ordered_fields = ordered_fields(section_fields, &layout_preferences);
                    move || {
                        Text(title, Modifier::empty(), heading_style(28.0, theme));
                        ForEach(&ordered_fields, {
                            let fields = fields.clone();
                            let saved_draft = saved_draft.clone();
                            move |field| {
                                EditorField(
                                    *field,
                                    fields.clone(),
                                    saved_draft.clone(),
                                    status,
                                    ui_preferences,
                                    theme,
                                    current_field == Some(*field),
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
    glow: bool,
) {
    if glow {
        glow_field_wrap(field, fields, saved_draft, status, ui_preferences, theme);
    } else {
        editor_field_inner(field, fields, saved_draft, status, ui_preferences, theme);
    }
}

#[composable]
fn editor_field_inner(
    field: EditorFieldId,
    fields: EditorFields,
    saved_draft: PostDraft,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    if field == EditorFieldId::Difficulty {
        DifficultyField(
            field_state(&fields, field),
            saved_field_text(&saved_draft, field),
            status,
            ui_preferences,
            theme,
        );
        return;
    }
    let (min_lines, max_lines) = field.editor_line_bounds();
    labeled_field(
        FieldSpec {
            label: field.label(),
            field_id: field.field_id(),
            state: field_state(&fields, field),
            saved_text: saved_field_text(&saved_draft, field),
            min_lines,
            max_lines,
            allow_paste: field.editor_allows_paste(),
            code: field.is_code(),
        },
        status,
        ui_preferences,
        theme,
    );
}

/// Stacks the animated fire border over the current hero field's editor without
/// changing its layout. The editor's height is measured so the transparent
/// overlay matches it exactly; only the overlay recomposes for the animation.
#[composable]
fn glow_field_wrap(
    field: EditorFieldId,
    fields: EditorFields,
    saved_draft: PostDraft,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let field_h = rememberMutableStateOf(|| 0.0f32);
    ComposeBox(
        Modifier::empty().fill_max_width(),
        BoxSpec::default().content_alignment(Alignment::TOP_START),
        {
            let fields = fields.clone();
            let saved_draft = saved_draft.clone();
            move || {
                let fields = fields.clone();
                let saved_draft = saved_draft.clone();
                ComposeBox(
                    Modifier::empty().fill_max_width().draw_behind(move |draw| {
                        let h = draw.size().height;
                        if (field_h.get_non_reactive() - h).abs() > 0.5 {
                            field_h.set(h);
                        }
                    }),
                    BoxSpec::default(),
                    move || {
                        editor_field_inner(
                            field,
                            fields.clone(),
                            saved_draft.clone(),
                            status,
                            ui_preferences,
                            theme,
                        );
                    },
                );
                let h = field_h.value();
                if h > 1.0 {
                    FireBorderOverlay(12.0, h);
                }
            }
        },
    );
}

const DIFFICULTY_OPTIONS: [(&str, &str); 3] =
    [("Easy", "easy"), ("Medium", "medium"), ("Hard", "hard")];

/// Difficulty picker rendered as a three-way Easy/Medium/Hard segmented toggle.
/// The chosen option is stored back into the field as lowercase text.
#[composable]
fn DifficultyField(
    state: TextFieldState,
    saved_text: String,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let current_text = state.text();
    track_field_interaction("difficulty", current_text.clone(), ui_preferences);
    let selected = current_text.trim().to_ascii_lowercase();
    let is_changed = current_text != saved_text;
    ComposeBox(
        glass_panel_modifier(Modifier::empty().fill_max_width(), theme, 12.0)
            .padding_symmetric(12.0, 10.0),
        BoxSpec::default(),
        move || {
            let state = state;
            let selected = selected.clone();
            Row(
                icon_overlay_modifier(
                    Modifier::empty().fill_max_width(),
                    UiIcon::Difficulty,
                    44.0,
                    0.0,
                    theme,
                    false,
                ),
                RowSpec::default().horizontal_arrangement(LinearArrangement::spaced_by(16.0)),
                move || {
                    Spacer(Size::new(44.0, 0.0));
                    let state = state;
                    let selected = selected.clone();
                    Column(
                        Modifier::empty().weight(1.0),
                        ColumnSpec::default()
                            .vertical_arrangement(LinearArrangement::spaced_by(6.0)),
                        move || {
                            Text(
                                "Difficulty",
                                Modifier::empty(),
                                label_style(theme, is_changed),
                            );
                            let state = state;
                            let selected = selected.clone();
                            Row(
                                Modifier::empty().fill_max_width(),
                                RowSpec::default()
                                    .horizontal_arrangement(LinearArrangement::spaced_by(8.0)),
                                move || {
                                    for (label, value) in DIFFICULTY_OPTIONS {
                                        DifficultySegment(
                                            label,
                                            value,
                                            selected == value,
                                            state,
                                            status,
                                            theme,
                                        );
                                    }
                                },
                            );
                        },
                    );
                },
            );
        },
    );
}

#[composable]
fn DifficultySegment(
    label: &'static str,
    value: &'static str,
    selected: bool,
    state: TextFieldState,
    status: MutableState<String>,
    theme: ThemeMode,
) {
    let (button_spec, press) = animated_button_spec(true, "difficulty_segment_press");
    let surface = if selected {
        button_surface(theme)
    } else {
        soft_button_surface(theme)
    };
    let mut text_style = if selected {
        focus_button_text_style(theme)
    } else {
        subtle_button_text_style(theme)
    };
    text_style.paragraph_style.text_align = cranpose::text::TextAlign::Center;
    Button(
        glass_button_modifier_with_press(
            Modifier::empty().weight(1.0),
            theme,
            true,
            selected,
            surface,
            10.0,
            press,
        )
        .padding_symmetric(12.0, 11.0),
        button_spec,
        move || {
            if state.text().trim().eq_ignore_ascii_case(value) {
                return;
            }
            state.set_text(value.to_string());
            status.set(format!("Difficulty set to {value}."));
        },
        move || {
            Text(
                label,
                Modifier::empty().fill_max_width(),
                text_style.clone(),
            );
        },
    );
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

    fn is_code(self) -> bool {
        matches!(self, Self::KotlinCode | Self::RustCode)
    }

    /// Line bounds in the full editor layout.
    fn editor_line_bounds(self) -> (usize, usize) {
        match self {
            Self::TelegramText | Self::TimeComplexity | Self::SpaceComplexity => (1, 2),
            Self::ProblemTldr => (3, 6),
            Self::Intuition | Self::Approach => (6, 14),
            Self::KotlinCode | Self::RustCode => (10, 18),
            _ => (1, 1),
        }
    }

    /// Compact line bounds for the interactive-queue "Current" row.
    fn queue_line_bounds(self) -> (usize, usize) {
        match self {
            Self::KotlinCode | Self::RustCode => (6, 14),
            Self::ProblemTldr | Self::Intuition | Self::Approach => (3, 8),
            Self::TimeComplexity | Self::SpaceComplexity => (2, 4),
            _ => (1, 1),
        }
    }

    fn editor_allows_paste(self) -> bool {
        !matches!(
            self,
            Self::TimeComplexity
                | Self::SpaceComplexity
                | Self::KotlinRuntimeMs
                | Self::RustRuntimeMs
        )
    }
}

fn saved_field_text(draft: &PostDraft, field: EditorFieldId) -> String {
    match field {
        EditorFieldId::Date => draft.date.clone(),
        EditorFieldId::ProblemTitle => draft.problem_title.clone(),
        EditorFieldId::ProblemUrl => draft.problem_url.clone(),
        EditorFieldId::Difficulty => draft.difficulty.clone(),
        EditorFieldId::SubstackUrl => draft.substack_url.clone(),
        EditorFieldId::YoutubeUrl => draft.youtube_url.clone(),
        EditorFieldId::ReferenceUrl => draft.reference_url.clone(),
        EditorFieldId::TelegramText => draft.telegram_text.clone(),
        EditorFieldId::ProblemTldr => draft.problem_tldr.clone(),
        EditorFieldId::Intuition => draft.intuition.clone(),
        EditorFieldId::Approach => draft.approach.clone(),
        EditorFieldId::TimeComplexity => draft.time_complexity.clone(),
        EditorFieldId::SpaceComplexity => draft.space_complexity.clone(),
        EditorFieldId::KotlinRuntimeMs => draft.kotlin_runtime_ms.clone(),
        EditorFieldId::KotlinCode => draft.kotlin_code.clone(),
        EditorFieldId::RustRuntimeMs => draft.rust_runtime_ms.clone(),
        EditorFieldId::RustCode => draft.rust_code.clone(),
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

fn draw_app_background<S: DrawScope + ?Sized>(scope: &mut S, theme: ThemeMode) {
    let size = scope.size();
    let gradient_stops = match theme {
        ThemeMode::Dark => vec![
            Color::from_rgb_u8(8, 14, 28),
            Color::from_rgb_u8(14, 24, 46),
            Color::from_rgb_u8(20, 32, 58),
            Color::from_rgb_u8(10, 18, 32),
        ],
        ThemeMode::Light => vec![
            Color::from_rgb_u8(218, 244, 255),
            Color::from_rgb_u8(236, 251, 255),
            Color::from_rgb_u8(188, 242, 249),
            Color::from_rgb_u8(120, 226, 209),
        ],
    };
    scope.draw_rect(Brush::linear_gradient_range(
        gradient_stops,
        Point::new(0.0, 0.0),
        Point::new(size.width, size.height),
    ));

    if matches!(theme, ThemeMode::Light) {
        if let Some(bitmap) = app_background_bitmap() {
            draw_stretchable_app_background(scope, bitmap, size, app_background_slices());
        }
    } else {
        scope.draw_rect(Brush::radial_gradient(
            vec![Color::from_rgba_u8(46, 86, 148, 70), Color::TRANSPARENT],
            Point::new(size.width * 0.5, size.height * 0.32),
            (size.width.max(size.height)) * 0.7,
        ));
    }
}

#[composable]
fn BottomListGapMask(theme: ThemeMode) {
    ComposeBox(
        Modifier::empty()
            .fill_max_width()
            .height(APP_BOTTOM_LIST_GAP)
            .draw_behind(move |scope| draw_bottom_list_gap_mask(scope, theme)),
        BoxSpec::default(),
        || {},
    );
}

fn draw_bottom_list_gap_mask<S: DrawScope + ?Sized>(scope: &mut S, theme: ThemeMode) {
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
    let (gradient_stops, highlight) = match theme {
        ThemeMode::Dark => (
            vec![
                Color::from_rgba_u8(28, 44, 70, 255),
                Color::from_rgba_u8(20, 34, 58, 255),
                Color::from_rgba_u8(14, 24, 44, 255),
            ],
            Color::from_rgba_u8(110, 170, 230, 110),
        ),
        ThemeMode::Light => (
            vec![
                Color::from_rgba_u8(209, 247, 252, 255),
                Color::from_rgba_u8(153, 236, 231, 255),
                Color::from_rgba_u8(71, 218, 218, 255),
            ],
            Color::from_rgba_u8(255, 255, 255, 172),
        ),
    };
    scope.draw_rect_at(
        Rect {
            x: horizontal_padding,
            y,
            width: content_width,
            height: APP_BOTTOM_LIST_GAP,
        },
        Brush::linear_gradient_range(
            gradient_stops,
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
            vec![Color::TRANSPARENT, highlight, Color::TRANSPARENT],
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
            Self::StageShip => Color::from_rgb_u8(139, 169, 188),
            Self::Comment => Color::from_rgb_u8(109, 205, 58),
            Self::RichText => Color::from_rgb_u8(48, 143, 177),
            Self::Document | Self::Subtitle => Color::from_rgb_u8(38, 151, 226),
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
    let (button_spec, press) = animated_button_spec(true, "field_suggestion_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty().fill_max_width(),
            theme,
            true,
            false,
            soft_button_surface(theme),
            11.0,
            press,
        )
        .padding_symmetric(13.0, 11.0),
        button_spec,
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
            let gloss_stops = match theme {
                ThemeMode::Dark => vec![
                    Color::from_rgba_u8(120, 168, 220, 92),
                    Color::from_rgba_u8(60, 96, 148, 30),
                    Color::from_rgba_u8(8, 18, 36, 0),
                ],
                ThemeMode::Light => vec![
                    Color::from_rgba_u8(255, 255, 255, 205),
                    Color::from_rgba_u8(255, 255, 255, 62),
                    Color::from_rgba_u8(17, 144, 212, 42),
                ],
            };
            scope.draw_round_rect(
                Brush::linear_gradient_range(
                    gloss_stops,
                    Point::new(0.0, 0.0),
                    Point::new(size.width, size.height),
                ),
                radii,
            );
            let highlight = match theme {
                ThemeMode::Dark => Color::from_rgba_u8(140, 188, 240, 110),
                ThemeMode::Light => Color::from_rgba_u8(255, 255, 255, 180),
            };
            scope.draw_rect_at(
                Rect {
                    x: 2.0,
                    y: 2.0,
                    width: (size.width - 4.0).max(0.0),
                    height: 2.0,
                },
                Brush::horizontal_gradient(
                    vec![Color::TRANSPARENT, highlight, Color::TRANSPARENT],
                    0.0,
                    size.width,
                ),
            );
        })
        .inner_shadow(shape, move |shadow| {
            shadow.radius = 8.0;
            shadow.spread = -1.0;
            shadow.offset = Point::new(0.0, 2.0);
            shadow.color = match theme {
                ThemeMode::Dark => Color::from_rgba_u8(150, 195, 240, 110),
                ThemeMode::Light => Color::from_rgba_u8(255, 255, 255, 150),
            };
            shadow.alpha = match theme {
                ThemeMode::Dark => 0.45,
                ThemeMode::Light => 0.72,
            };
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
    glass_button_modifier_with_press(modifier, theme, enabled, active, base, radius, 0.0)
}

#[composable]
fn animated_button_spec(enabled: bool, label: &'static str) -> (ButtonSpec, f32) {
    let interaction_source = rememberMutableInteractionSource();
    let is_pressed = interaction_source.collectIsPressedAsState().value();
    let target = if enabled && is_pressed { 1.0 } else { 0.0 };
    let animation = if target > 0.0 {
        spring(Spring::DampingRatioMediumBouncy, Spring::StiffnessMedium)
    } else {
        tween(
            BUTTON_PRESS_RELEASE_DURATION_MS,
            Easing::FastOutSlowInEasing,
        )
    };
    let press = animateFloatAsState(target, animation, label)
        .value()
        .clamp(0.0, 1.0);

    (
        ButtonSpec::new().interaction_source(interaction_source),
        press,
    )
}

fn glass_button_modifier_with_press(
    modifier: Modifier,
    theme: ThemeMode,
    enabled: bool,
    active: bool,
    base: Color,
    radius: f32,
    press: f32,
) -> Modifier {
    let press = if enabled { press.clamp(0.0, 1.0) } else { 0.0 };
    let shape = LayerShape::Rounded(RoundedCornerShape::uniform(radius));
    let shadow_alpha = if enabled { 0.64 } else { 0.18 };
    let scale = 1.0 - press * BUTTON_PRESS_SCALE_DELTA;
    let translation_y = press * BUTTON_PRESS_TRANSLATION_Y;
    modifier
        .graphics_layer_block(move |layer| {
            layer.scale_x = scale;
            layer.scale_y = scale;
            layer.translation_y = translation_y;
        })
        .drop_shadow(shape, move |shadow| {
            let base_radius = if active { 13.0 } else { 9.0 };
            let base_spread = if active { 1.0 } else { 0.0 };
            let base_offset = if active { 6.0 } else { 4.0 };
            shadow.radius = (base_radius - press * 3.0).max(2.0);
            shadow.spread = (base_spread - press * 0.45).max(0.0);
            shadow.offset = Point::new(0.0, (base_offset - press * 1.8).max(1.0));
            shadow.color = shadow_color(theme);
            shadow.alpha = shadow_alpha * (1.0 - press * 0.24);
        })
        .draw_behind(move |scope| {
            let size = scope.size();
            let radii = CornerRadii::uniform(radius);
            let top = if enabled {
                lighten_color(base, (if active { 0.56 } else { 0.38 }) - press * 0.08)
            } else {
                base.with_alpha(0.5)
            };
            let bottom = if enabled {
                darken_color(base, (if active { 0.16 } else { 0.06 }) + press * 0.08)
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
            shadow.radius = 5.0 + press * 2.0;
            shadow.spread = -1.0;
            shadow.offset = Point::new(0.0, 1.0 + press);
            shadow.color = Color::from_rgba_u8(255, 255, 255, if active { 180 } else { 115 });
            shadow.alpha = if enabled { 0.72 - press * 0.18 } else { 0.3 };
        })
        .rounded_corners(radius)
}

#[composable]
fn primary_button(
    action: ActionButtonId,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
    disabled: bool,
    busy: bool,
    fill_width: bool,
    on_click: impl FnMut() + 'static,
) {
    let icon = action.icon();
    let label = action.label();
    let count = ui_preferences.value().button_count(action.count_key());
    let count_key = action.count_key().to_string();
    let base = if fill_width {
        Modifier::empty().fill_max_width()
    } else {
        Modifier::empty().weight(1.0)
    };
    let background = if busy {
        button_surface(theme).with_alpha(0.86)
    } else if disabled {
        disabled_button_surface(theme)
    } else {
        button_surface(theme)
    };
    let text_style = if busy {
        busy_button_text_style(theme)
    } else if disabled {
        disabled_button_text_style(theme)
    } else {
        button_text_style(theme)
    };
    let (button_spec, press) = animated_button_spec(!disabled, "primary_button_press");
    Button(
        glass_button_modifier_with_press(base, theme, !disabled, busy, background, 10.0, press)
            .height(46.0)
            .padding_symmetric(8.0, 9.0),
        button_spec,
        move || {
            if disabled {
                return;
            }
            record_button_press(ui_preferences, &count_key);
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
    let (button_spec, press) = animated_button_spec(true, "subtle_button_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty(),
            theme,
            true,
            false,
            soft_button_surface(theme),
            9.0,
            press,
        )
        .padding_symmetric(9.0, 7.0),
        button_spec,
        move || {
            record_button_press(ui_preferences, &count_key);
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
    let (button_spec, press) = animated_button_spec(true, "theme_button_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty(),
            theme,
            true,
            false,
            soft_button_surface(theme),
            9.0,
            press,
        )
        .padding_symmetric(10.0, 7.0),
        button_spec,
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

/// Small "Skip" button on the hero tile. Drops the current expected action for
/// this session only and lets the next item surface, without recording anything.
#[composable]
fn skip_work_button(
    skip_key: String,
    label: String,
    skipped_queue: MutableState<Vec<String>>,
    status: MutableState<String>,
    theme: ThemeMode,
) {
    let (button_spec, press) = animated_button_spec(true, "skip_work_button_press");
    Button(
        glass_button_modifier_with_press(
            Modifier::empty(),
            theme,
            true,
            false,
            soft_button_surface(theme),
            9.0,
            press,
        )
        .padding_symmetric(10.0, 6.0),
        button_spec,
        move || {
            skip_work_item(skipped_queue, status, &skip_key, &label);
        },
        move || {
            Text("Skip", Modifier::empty(), subtle_button_text_style(theme));
        },
    );
}

/// Records a transient skip: appends the key to the session-only skipped set so
/// next-item selection moves past it. Nothing is persisted or counted.
fn skip_work_item(
    skipped_queue: MutableState<Vec<String>>,
    status: MutableState<String>,
    skip_key: &str,
    label: &str,
) {
    let mut skipped = skipped_queue.value();
    if !skipped.iter().any(|key| key == skip_key) {
        skipped.push(skip_key.to_string());
        skipped_queue.set(skipped);
    }
    status.set(format!("Skipped {label} for now."));
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
            let label = label.clone();
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
            ButtonActivityIndicator(theme, busy);
            button_badge(count, theme);
        },
    );
}

#[composable]
fn ButtonActivityIndicator(theme: ThemeMode, active: bool) {
    // Only run the infinite transition while the indicator is active: an
    // always-running transition keeps the frame loop animating forever and
    // burns a full GPU at idle. Late-created infinite transitions advance
    // correctly since Cranpose 0.1.6 (issue #262).
    let phase = if active {
        rememberInfiniteTransition("button_activity_indicator")
            .animateFloat(
                0.0,
                1.0,
                infiniteRepeatable(
                    AnimationSpec::tween(900, Easing::EaseInOut),
                    RepeatMode::Reverse,
                    StartOffset::default(),
                ),
                "button_activity_indicator_phase",
            )
            .value()
    } else {
        0.0
    };
    let indicator_width = if active {
        BUTTON_ACTIVITY_INDICATOR_WIDTH
    } else {
        0.0
    };
    ComposeBox(
        Modifier::empty()
            .size(Size::new(indicator_width, BUTTON_ACTIVITY_INDICATOR_HEIGHT))
            .clip_to_bounds()
            .draw_behind(move |scope| {
                if !active {
                    return;
                }
                let size = scope.size();
                let phase = phase.clamp(0.0, 1.0);
                let track_color = match theme {
                    ThemeMode::Dark => Color::from_rgba_u8(150, 195, 240, 86),
                    ThemeMode::Light => Color::from_rgba_u8(4, 73, 124, 78),
                };
                let highlight_start = match theme {
                    ThemeMode::Dark => Color::from_rgba_u8(106, 226, 255, 245),
                    ThemeMode::Light => Color::from_rgba_u8(0, 119, 216, 245),
                };
                let travel = (size.width - 24.0).max(0.0);
                let marker_x = travel * phase;
                let counter_x = travel * (1.0 - phase);
                scope.draw_rect_at(
                    Rect {
                        x: 0.0,
                        y: 7.0,
                        width: size.width,
                        height: 5.0,
                    },
                    Brush::solid(track_color),
                );
                scope.draw_rect_at(
                    Rect {
                        x: marker_x,
                        y: 3.0,
                        width: 24.0,
                        height: 12.0,
                    },
                    Brush::horizontal_gradient(
                        vec![highlight_start, Color::from_rgba_u8(255, 255, 255, 250)],
                        marker_x,
                        marker_x + 24.0,
                    ),
                );
                scope.draw_rect_at(
                    Rect {
                        x: counter_x + 9.0,
                        y: 0.0,
                        width: 5.0,
                        height: 18.0,
                    },
                    Brush::solid(Color::from_rgba_u8(62, 219, 111, 240)),
                );
            }),
        BoxSpec::default(),
        || {},
    );
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
    spec: FieldSpec,
    status: MutableState<String>,
    ui_preferences: MutableState<UiPreferences>,
    theme: ThemeMode,
) {
    let FieldSpec {
        label,
        field_id,
        state,
        saved_text,
        min_lines,
        max_lines,
        allow_paste,
        code,
    } = spec;
    let current_text = state.text();
    track_field_interaction(field_id, current_text.clone(), ui_preferences);
    if field_is_numeric(field_id) {
        enforce_numeric_input(state, current_text.clone());
    }
    let is_changed = current_text != saved_text;
    let icon = UiIcon::for_field_id(field_id);
    let input_box_modifier = {
        let base = Modifier::empty()
            .fill_max_width()
            .background(input_surface(theme))
            .rounded_corners(8.0);
        if code {
            base.padding(12.0)
        } else {
            base.padding_symmetric(11.0, 7.0)
        }
    };
    let text_style = if code {
        code_field_style(theme)
    } else {
        field_text_style(theme)
    };
    ComposeBox(
        glass_panel_modifier(Modifier::empty().fill_max_width(), theme, 12.0)
            .padding_symmetric(12.0, 10.0),
        BoxSpec::default(),
        move || {
            let state = state;
            let input_box_modifier = input_box_modifier.clone();
            let text_style = text_style.clone();
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
                    let field_state = state;
                    let input_box_modifier = input_box_modifier.clone();
                    let text_style = text_style.clone();
                    Column(
                        Modifier::empty().weight(1.0),
                        ColumnSpec::default()
                            .vertical_arrangement(LinearArrangement::spaced_by(6.0)),
                        {
                            move || {
                                Text(label, Modifier::empty(), label_style(theme, is_changed));
                                let field_state = field_state;
                                let text_style = text_style.clone();
                                ComposeBox(
                                    input_box_modifier.clone(),
                                    BoxSpec::default(),
                                    move || {
                                        BasicTextFieldWithOptions(
                                            field_state,
                                            Modifier::empty().fill_max_width(),
                                            BasicTextFieldOptions {
                                                text_style: text_style.clone(),
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
                        state,
                        status,
                        allow_paste,
                        ui_preferences,
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
            move || {
                if allow_paste {
                    let paste_state = state;
                    let paste_status = status;
                    subtle_button(
                        "Paste".to_string(),
                        format!("field.{field_id}.paste"),
                        ui_preferences,
                        theme,
                        move || {
                            paste_text_from_clipboard(paste_state, paste_status, label);
                        },
                    );
                }

                let clear_state = state;
                let clear_status = status;
                subtle_button(
                    "Clear".to_string(),
                    format!("field.{field_id}.clear"),
                    ui_preferences,
                    theme,
                    move || {
                        clear_field(clear_state, clear_status, label);
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
    let last_text = rememberMutableStateOf(|| current_text.clone());
    cranpose_core::LaunchedEffect!(current_text.clone(), {
        let current_text = current_text.clone();
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

/// Fields whose contents must stay strictly numeric (digits only).
fn field_is_numeric(field_id: &str) -> bool {
    matches!(field_id, "kotlin_runtime_ms" | "rust_runtime_ms")
}

/// Reactively strips any non-digit character the user types into a numeric
/// field, so runtime (ms) inputs can only ever hold a plain number.
#[composable]
fn enforce_numeric_input(state: TextFieldState, current_text: String) {
    cranpose_core::LaunchedEffect!(current_text.clone(), {
        let current_text = current_text.clone();
        move |_scope| {
            let filtered: String = current_text.chars().filter(char::is_ascii_digit).collect();
            if filtered != current_text {
                state.set_text(filtered);
            }
        }
    });
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
pub fn run_app_capture_cli(output_path: &Path, size: Option<(u32, u32)>) -> Result<()> {
    let (tx, rx) = mpsc::channel::<std::result::Result<PreviewFrame, String>>();
    let (width, height) = size.unwrap_or((APP_WIDTH, APP_HEIGHT));

    let launch_result = launcher_with_size(width, height)
        .with_headless(true)
        .with_test_driver({
            let tx = tx.clone();
            move |robot| {
                let result = (|| -> std::result::Result<PreviewFrame, String> {
                    // Not wait_for_idle: the animated fire never idles.
                    robot.pump_frames(20)?;
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

/// Headless robot harness used to validate interaction behaviour (drag, scroll)
/// by driving the real app and dumping screenshots to `output_dir`.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_robot_cli(scenario: &str, output_dir: &Path) -> Result<()> {
    let _ = fs::create_dir_all(output_dir);
    let (tx, rx) = mpsc::channel::<std::result::Result<(), String>>();
    let scenario = scenario.to_string();
    let output_dir = output_dir.to_path_buf();

    let launch_result = launcher_with_size(APP_WIDTH, APP_HEIGHT)
        .with_headless(true)
        .with_test_driver({
            let tx = tx.clone();
            move |robot| {
                let result = (|| -> std::result::Result<(), String> {
                    let save = |name: &str,
                                w: u32,
                                h: u32,
                                px: Vec<u8>|
                     -> std::result::Result<(), String> {
                        let img = RgbaImage::from_raw(w, h, px).ok_or("bad frame")?;
                        img.save_with_format(
                            output_dir.join(format!("{name}.png")),
                            ImageFormat::Png,
                        )
                        .map_err(|e| e.to_string())
                    };
                    robot.pump_frames(20)?;
                    let s = robot.screenshot_with_scale(1.0)?;
                    save("00_before", s.width, s.height, s.pixels)?;
                    match scenario.as_str() {
                        "anim" => {
                            std::thread::sleep(Duration::from_millis(700));
                            robot.pump_frames(4)?;
                            let s = robot.screenshot_with_scale(1.0)?;
                            save("01_later", s.width, s.height, s.pixels)?;
                        }
                        "drag" => {
                            robot.mouse_move(150.0, 585.0)?;
                            robot.mouse_down()?;
                            robot.mouse_move(230.0, 585.0)?;
                            robot.pump_frames(3)?;
                            let s = robot.screenshot_with_scale(1.0)?;
                            save("01_drag_80px", s.width, s.height, s.pixels)?;
                            robot.mouse_move(430.0, 585.0)?;
                            robot.pump_frames(3)?;
                            let s = robot.screenshot_with_scale(1.0)?;
                            save("02_drag_280px", s.width, s.height, s.pixels)?;
                            robot.mouse_up()?;
                            robot.pump_frames(4)?;
                            let s = robot.screenshot_with_scale(1.0)?;
                            save("03_after", s.width, s.height, s.pixels)?;
                        }
                        "scroll" => {
                            robot.mouse_move(720.0, 1050.0)?;
                            for i in 0..10u32 {
                                robot.mouse_scroll_and_wait_for_frame(0.0, -48.0)?;
                                robot.pump_frames(1)?;
                                let s = robot.screenshot_with_scale(1.0)?;
                                save(&format!("dn_{i:02}"), s.width, s.height, s.pixels)?;
                            }
                            for i in 0..10u32 {
                                robot.mouse_scroll_and_wait_for_frame(0.0, 48.0)?;
                                robot.pump_frames(1)?;
                                let s = robot.screenshot_with_scale(1.0)?;
                                save(&format!("up_{i:02}"), s.width, s.height, s.pixels)?;
                            }
                        }
                        _ => {}
                    }
                    robot.exit()?;
                    Ok(())
                })();
                let _ = tx.send(result);
            }
        })
        .try_run(App);

    launch_result.map_err(|error| anyhow::anyhow!(error.to_string()))?;
    rx.recv_timeout(Duration::from_secs(180))
        .map_err(|error| anyhow::anyhow!("robot scenario timed out: {error}"))?
        .map_err(anyhow::Error::msg)?;
    Ok(())
}

fn run_long_action(pending: PendingAction) -> LongActionResult {
    #[cfg(not(target_arch = "wasm32"))]
    let started_at = Instant::now();
    let result = match pending.action {
        LongAction::RefreshRasterPreview => {
            LongActionResult::RefreshRasterPreview(render_raster_preview_result(&pending.draft))
        }
        #[cfg(not(target_arch = "wasm32"))]
        LongAction::CopyRichText => {
            LongActionResult::CopyRichText(copy_rich_text_result(&pending.draft))
        }
        LongAction::SaveRasterWebp => LongActionResult::SaveRasterWebp(
            save_webp(&pending.draft).map_err(|error| error.to_string()),
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
    };
    #[cfg(not(target_arch = "wasm32"))]
    hold_long_action_busy_state(started_at);
    result
}

#[cfg(not(target_arch = "wasm32"))]
fn hold_long_action_busy_state(started_at: Instant) {
    let minimum = Duration::from_millis(MIN_LONG_ACTION_BUSY_MS);
    if let Some(remaining) = minimum.checked_sub(started_at.elapsed()) {
        std::thread::sleep(remaining);
    }
}

fn finish_long_action(
    result: LongActionResult,
    previews: PreviewStates,
    actions: ActionStates,
    fields: EditorFields,
) {
    let PreviewStates { preview_state, .. } = previews;
    let ActionStates {
        status,
        telegram_post_link,
        ..
    } = actions;
    actions.busy_action.set(None);
    actions.pending_action.set(None);

    match result {
        LongActionResult::RefreshRasterPreview(result) => match result {
            Ok(preview) => {
                preview_state.set(preview);
                status.set("Raster preview refreshed.".to_string());
            }
            Err(error) => status.set(format!("Raster preview failed: {error}")),
        },
        #[cfg(not(target_arch = "wasm32"))]
        LongActionResult::CopyRichText(result) => match result {
            Ok(()) => status.set("Rich text copied to the clipboard.".to_string()),
            Err(error) => status.set(format!("Rich text copy failed: {error}")),
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
                let link = outcome.link;
                preview_state.set(outcome.preview);
                telegram_post_link.set(link.clone());
                fields.telegram_text.set_text(link.clone());
                status.set(format!("Telegram post published and CTA updated: {link}"));
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

#[cfg(not(target_arch = "wasm32"))]
fn copy_rich_text_result(draft: &PostDraft) -> std::result::Result<(), String> {
    let image_data_url = preview_webp_data_url(draft).ok();
    let html = draft.rich_html_with_image(image_data_url.as_deref());
    let fallback = draft.rich_text_fallback();
    copy_rich_text(&html, &fallback).map_err(|error| error.to_string())
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

fn panel_brush(theme: ThemeMode, size: Size) -> Brush {
    match theme {
        ThemeMode::Dark => Brush::linear_gradient_range(
            vec![
                Color::from_rgba_u8(36, 52, 80, 215),
                Color::from_rgba_u8(24, 38, 62, 195),
                Color::from_rgba_u8(18, 28, 50, 175),
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
        ThemeMode::Dark => Color::from_rgba_u8(0, 0, 0, 175),
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

fn focus_button_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(button_text_color(theme).with_alpha(0.82)),
            font_size: cranpose::text::TextUnit::Sp(15.0),
            font_weight: Some(cranpose::text::FontWeight::BOLD),
            ..SpanStyle::default()
        },
        paragraph_style: ParagraphStyle::default(),
    }
}

fn busy_button_text_style(theme: ThemeMode) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(button_text_color(theme).with_alpha(0.72)),
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
) -> TextStyle {
    TextStyle {
        span_style: SpanStyle {
            color: Some(if disabled {
                muted_text_color(theme)
            } else if done || busy {
                button_text_color(theme).with_alpha(0.82)
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

fn panel_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(223, 245, 255, 190),
        ThemeMode::Light => Color::from_rgba_u8(238, 252, 255, 205),
    }
}

fn input_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(28, 42, 66, 220),
        ThemeMode::Light => Color::from_rgba_u8(255, 255, 255, 226),
    }
}

fn button_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(98, 220, 130),
        ThemeMode::Light => Color::from_rgb_u8(39, 145, 224),
    }
}

fn disabled_button_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(58, 76, 100, 200),
        ThemeMode::Light => Color::from_rgba_u8(213, 230, 236, 170),
    }
}

fn interactive_queue_surface(
    theme: ThemeMode,
    done: bool,
    disabled: bool,
    busy: bool,
    invokes_button: bool,
) -> Color {
    if busy {
        return button_surface(theme).with_alpha(0.66);
    }
    if disabled {
        return disabled_button_surface(theme);
    }
    if invokes_button && !done {
        return button_surface(theme);
    }
    if done {
        return match theme {
            ThemeMode::Dark => Color::from_rgb_u8(70, 178, 100),
            ThemeMode::Light => Color::from_rgb_u8(77, 184, 91),
        };
    }
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(38, 56, 82, 215),
        ThemeMode::Light => Color::from_rgba_u8(255, 255, 255, 218),
    }
}

fn badge_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(48, 70, 102, 225),
        ThemeMode::Light => Color::from_rgba_u8(226, 245, 252, 230),
    }
}

fn soft_button_surface(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgba_u8(38, 58, 88, 215),
        ThemeMode::Light => Color::from_rgba_u8(237, 250, 255, 185),
    }
}

fn primary_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(228, 240, 252),
        ThemeMode::Light => Color::from_rgb_u8(14, 58, 96),
    }
}

fn body_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(202, 218, 238),
        ThemeMode::Light => Color::from_rgb_u8(52, 84, 107),
    }
}

fn muted_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(150, 175, 205),
        ThemeMode::Light => Color::from_rgb_u8(60, 96, 128),
    }
}

fn label_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(170, 205, 240),
        ThemeMode::Light => Color::from_rgb_u8(18, 87, 145),
    }
}

fn changed_label_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(98, 224, 224),
        ThemeMode::Light => Color::from_rgb_u8(9, 131, 154),
    }
}

fn accent_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(94, 198, 255),
        ThemeMode::Light => Color::from_rgb_u8(13, 117, 181),
    }
}

fn button_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(8, 36, 22),
        ThemeMode::Light => Color::from_rgb_u8(9, 58, 103),
    }
}

fn badge_text_color(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Dark => Color::from_rgb_u8(220, 235, 250),
        ThemeMode::Light => Color::from_rgb_u8(18, 81, 116),
    }
}

#[cfg(test)]
mod tests {
    use crate::draft::{PostDraft, UiPreferences};
    use crate::export::PreviewState;

    use super::{
        APP_HEIGHT, APP_WIDTH, ActionButtonId, EditorFieldId, FieldQueueCommand,
        INTERACTIVE_QUEUE_CHIP_GAP, INTERACTIVE_QUEUE_CHIP_WIDTH, LongAction, META_FIELDS,
        NextWorkItem, WEB_SURFACE_MAX_DIM, compute_web_canvas_size, interactive_queue_label,
        interactive_queue_next_key, interactive_queue_scroll_target,
        interactive_queue_selected_key, interactive_queue_should_auto_scroll, next_work_item_key,
        ordered_action_buttons, ordered_fields, parse_field_queue_key, queue_item_invokes_button,
        recommended_next_work, recommended_next_work_excluding,
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn copy_rich_text_uses_long_action_pipeline_on_desktop() {
        assert_eq!(
            ActionButtonId::CopyRichText.long_action(),
            Some(LongAction::CopyRichText)
        );
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

    #[test]
    fn next_work_item_key_matches_action_and_field_keys() {
        assert_eq!(
            next_work_item_key(NextWorkItem::Field(EditorFieldId::ProblemTitle)),
            "field.problem_title"
        );
        assert_eq!(
            next_work_item_key(NextWorkItem::Action(ActionButtonId::SaveRasterWebp)),
            ActionButtonId::SaveRasterWebp.count_key()
        );
    }

    #[test]
    fn skipping_recommended_item_advances_to_next() {
        let draft = PostDraft::default();
        let preview = PreviewState::placeholder();
        let preferences = UiPreferences::default();

        let first = recommended_next_work_excluding(&draft, &preview, "", &preferences, &[]);
        assert_eq!(first, NextWorkItem::Action(ActionButtonId::SaveRasterWebp));

        // Skipping the current item surfaces the next one without touching the draft.
        let skipped = vec![next_work_item_key(first)];
        let next = recommended_next_work_excluding(&draft, &preview, "", &preferences, &skipped);
        assert_ne!(next, first);
        assert_eq!(next, NextWorkItem::Action(ActionButtonId::CopyBlog));
    }
}
