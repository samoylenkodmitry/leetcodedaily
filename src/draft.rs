use anyhow::{Context, Result, anyhow};
use chrono::Local;
use cranpose_foundation::text::TextFieldState;
use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

const DEFAULT_REFERENCE_URL: &str = "https://dmitrysamoylenko.com/2023/07/14/leetcode_daily.html";
#[cfg(target_arch = "wasm32")]
const AUTOSAVE_STORAGE_KEY: &str = "leetcodedaily.autosave.v1";
const AUTOSAVE_FORMAT_VERSION: &str = "leetcodedaily-draft-v1";
#[cfg(target_arch = "wasm32")]
const UI_PREFERENCES_STORAGE_KEY: &str = "leetcodedaily.ui-preferences.v1";
const UI_PREFERENCES_FORMAT_VERSION: &str = "leetcodedaily-ui-preferences-v1";

#[derive(Clone, PartialEq)]
pub struct EditorFields {
    pub date: TextFieldState,
    pub problem_title: TextFieldState,
    pub problem_url: TextFieldState,
    pub difficulty: TextFieldState,
    pub blog_post_url: TextFieldState,
    pub substack_url: TextFieldState,
    pub youtube_url: TextFieldState,
    pub reference_url: TextFieldState,
    pub telegram_text: TextFieldState,
    pub problem_tldr: TextFieldState,
    pub intuition: TextFieldState,
    pub approach: TextFieldState,
    pub time_complexity: TextFieldState,
    pub space_complexity: TextFieldState,
    pub kotlin_runtime_ms: TextFieldState,
    pub kotlin_code: TextFieldState,
    pub rust_runtime_ms: TextFieldState,
    pub rust_code: TextFieldState,
}

impl Default for EditorFields {
    fn default() -> Self {
        Self::from_draft(&PostDraft::default())
    }
}

impl EditorFields {
    pub fn from_draft(draft: &PostDraft) -> Self {
        Self {
            date: TextFieldState::new(today_date()),
            problem_title: TextFieldState::new(draft.problem_title.clone()),
            problem_url: TextFieldState::new(draft.problem_url.clone()),
            difficulty: TextFieldState::new(draft.difficulty.clone()),
            blog_post_url: TextFieldState::new(draft.blog_post_url.clone()),
            substack_url: TextFieldState::new(draft.substack_url.clone()),
            youtube_url: TextFieldState::new(draft.youtube_url.clone()),
            reference_url: TextFieldState::new(draft.reference_url.clone()),
            telegram_text: TextFieldState::new(draft.telegram_text.clone()),
            problem_tldr: TextFieldState::new(draft.problem_tldr.clone()),
            intuition: TextFieldState::new(draft.intuition.clone()),
            approach: TextFieldState::new(draft.approach.clone()),
            time_complexity: TextFieldState::new(draft.time_complexity.clone()),
            space_complexity: TextFieldState::new(draft.space_complexity.clone()),
            kotlin_runtime_ms: TextFieldState::new(draft.kotlin_runtime_ms.clone()),
            kotlin_code: TextFieldState::new(draft.kotlin_code.clone()),
            rust_runtime_ms: TextFieldState::new(draft.rust_runtime_ms.clone()),
            rust_code: TextFieldState::new(draft.rust_code.clone()),
        }
    }
}

pub fn load_initial_draft() -> PostDraft {
    match load_autosave() {
        Ok(Some(draft)) => draft,
        _ => PostDraft::default(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

impl ThemeMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    fn from_storage(value: &str) -> Option<Self> {
        match value {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    fn as_storage(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct UiPreferences {
    pub theme: ThemeMode,
    button_counts: BTreeMap<String, u64>,
    component_order: BTreeMap<String, u64>,
}

impl UiPreferences {
    pub fn button_count(&self, key: &str) -> u64 {
        self.button_counts.get(key).copied().unwrap_or(0)
    }

    pub fn increment_button_count(&mut self, key: &str) -> u64 {
        let count = self.button_counts.entry(key.to_string()).or_default();
        *count = count.saturating_add(1);
        *count
    }

    pub fn component_order(&self, key: &str) -> u64 {
        self.component_order.get(key).copied().unwrap_or(0)
    }

    pub fn mark_component_used(&mut self, key: &str) -> u64 {
        let next_order = self
            .component_order
            .values()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.component_order.insert(key.to_string(), next_order);
        next_order
    }
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct PostDraft {
    pub date: String,
    pub problem_title: String,
    pub problem_url: String,
    pub difficulty: String,
    pub blog_post_url: String,
    pub substack_url: String,
    pub youtube_url: String,
    pub reference_url: String,
    pub telegram_text: String,
    pub problem_tldr: String,
    pub intuition: String,
    pub approach: String,
    pub time_complexity: String,
    pub space_complexity: String,
    pub kotlin_runtime_ms: String,
    pub kotlin_code: String,
    pub rust_runtime_ms: String,
    pub rust_code: String,
}

impl Default for PostDraft {
    fn default() -> Self {
        Self {
            date: today_date(),
            problem_title: "Words Within Two Edits of Dictionary".to_string(),
            problem_url: "https://leetcode.com/problems/words-within-two-edits-of-dictionary/description/".to_string(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: String::new(),
            reference_url: DEFAULT_REFERENCE_URL.to_string(),
            telegram_text: String::new(),
            problem_tldr: "Words in dictionary with 2 edits".to_string(),
            intuition: "Compare every query word against the dictionary and keep it if any dictionary word differs in fewer than three positions.".to_string(),
            approach: "Use a direct scan. For each query word, count character mismatches against every dictionary candidate and stop as soon as one candidate stays under three mismatches.".to_string(),
            time_complexity: "n * m * k".to_string(),
            space_complexity: "1".to_string(),
            kotlin_runtime_ms: "28".to_string(),
            kotlin_code: "fun twoEditWords(q: Array<String>, d: Array<String>) =\n    q.filter { q -> d.any { d -> d.indices.count { d[it] != q[it] } < 3 } }".to_string(),
            rust_runtime_ms: "1".to_string(),
            rust_code: "pub fn two_edit_words(mut q: Vec<String>, d: Vec<String>) -> Vec<String> {\n    q.retain(|q| d.iter().any(|d| d.bytes().zip(q.bytes()).filter(|(d, q)| d != q).count() < 3));\n    q\n}".to_string(),
        }
    }
}

impl PostDraft {
    pub fn from_fields(fields: &EditorFields) -> Self {
        Self {
            date: today_date(),
            problem_title: trim_or(fields.problem_title.text(), ""),
            problem_url: trim_or(fields.problem_url.text(), ""),
            difficulty: trim_or(fields.difficulty.text(), "medium"),
            blog_post_url: trim_or(fields.blog_post_url.text(), ""),
            substack_url: trim_or(fields.substack_url.text(), ""),
            youtube_url: trim_or(fields.youtube_url.text(), ""),
            reference_url: trim_or(fields.reference_url.text(), ""),
            telegram_text: normalize_block(fields.telegram_text.text()),
            problem_tldr: normalize_block(fields.problem_tldr.text()),
            intuition: normalize_block(fields.intuition.text()),
            approach: normalize_block(fields.approach.text()),
            time_complexity: trim_or(fields.time_complexity.text(), ""),
            space_complexity: trim_or(fields.space_complexity.text(), ""),
            kotlin_runtime_ms: trim_or(fields.kotlin_runtime_ms.text(), ""),
            kotlin_code: normalize_code(fields.kotlin_code.text()),
            rust_runtime_ms: trim_or(fields.rust_runtime_ms.text(), ""),
            rust_code: normalize_code(fields.rust_code.text()),
        }
    }

    fn with_today_date(mut self) -> Self {
        self.date = today_date();
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn markdown(&self) -> String {
        self.blog_template()
    }

    pub fn leetcode_template(&self) -> String {
        let mut lines = Vec::new();

        push_optional_plain_line(&mut lines, &self.youtube_url);
        if !lines.is_empty() {
            lines.push(String::new());
        }

        push_markdown_section(&mut lines, "#### Join me on ", &self.telegram_text);
        push_markdown_section(&mut lines, "#### Problem TLDR", &self.problem_tldr);
        push_markdown_section(&mut lines, "#### Intuition", &self.intuition);
        push_markdown_section(&mut lines, "#### Approach", &self.approach);
        push_math_complexity_section(&mut lines, &self.time_complexity, &self.space_complexity);
        push_markdown_heading(&mut lines, "#### Code");
        push_runtime_code_block(
            &mut lines,
            "kotlin",
            "Kotlin",
            &self.kotlin_runtime_ms,
            &self.kotlin_code,
        );
        push_runtime_code_block(
            &mut lines,
            "rust",
            "Rust",
            &self.rust_runtime_ms,
            &self.rust_code,
        );

        finalize_markdown(lines)
    }

    pub fn youtube_template(&self) -> String {
        let mut lines = vec![format!("# {}", self.date_or_placeholder())];

        let problem_line = self.problem_header_line();
        if !problem_line.is_empty() {
            lines.push(problem_line);
        }

        push_optional_link(&mut lines, "substack", &self.substack_url);
        lines.push(String::new());

        push_markdown_section(&mut lines, "#### Join me on Telegram", &self.telegram_text);
        push_markdown_section(&mut lines, "#### Problem TLDR", &self.problem_tldr);
        push_markdown_section(&mut lines, "#### Intuition", &self.intuition);
        push_markdown_section(&mut lines, "#### Approach", &self.approach);
        push_plain_complexity_section(&mut lines, &self.time_complexity, &self.space_complexity);
        push_markdown_section(&mut lines, "#### Code", self.reference_url.trim());

        finalize_markdown(lines)
    }

    pub fn blog_template(&self) -> String {
        let mut lines = vec![format!("# {}", self.date_or_placeholder())];

        let problem_line = self.problem_header_line();
        if !problem_line.is_empty() {
            lines.push(problem_line);
        }

        push_optional_link(&mut lines, "substack", &self.substack_url);
        push_optional_link(&mut lines, "youtube", &self.youtube_url);
        lines.push(String::new());

        push_optional_plain_line(&mut lines, &self.reference_url);
        if !self.reference_url.trim().is_empty() {
            lines.push(String::new());
        }
        lines.push(self.image_markdown_line());

        push_markdown_section(&mut lines, "#### Join me on Telegram", &self.telegram_text);
        push_markdown_section(&mut lines, "#### Problem TLDR", &self.problem_tldr);
        push_markdown_section(&mut lines, "#### Intuition", &self.intuition);
        push_markdown_section(&mut lines, "#### Approach", &self.approach);
        push_math_complexity_section(&mut lines, &self.time_complexity, &self.space_complexity);
        push_markdown_heading(&mut lines, "#### Code");
        push_code_block(&mut lines, "kotlin", &self.kotlin_code);
        push_code_block(&mut lines, "rust", &self.rust_code);

        finalize_markdown(lines)
    }

    pub fn rich_text_fallback(&self) -> String {
        let mut lines = vec![format!("# {}", self.date_or_placeholder())];

        let problem_line = self.problem_header_line();
        if !problem_line.is_empty() {
            lines.push(problem_line);
        }

        push_optional_link(&mut lines, "blog post", &self.blog_post_url);
        push_optional_link(&mut lines, "substack", &self.substack_url);
        lines.push(String::new());

        push_optional_plain_line(&mut lines, &self.reference_url);
        if !self.reference_url.trim().is_empty() {
            lines.push(String::new());
        }
        lines.push(self.image_markdown_line());

        push_optional_plain_line(&mut lines, &self.youtube_url);
        if !self.youtube_url.trim().is_empty() {
            lines.push(String::new());
        }

        push_markdown_section(&mut lines, "#### Join me on Telegram", &self.telegram_text);
        push_markdown_section(&mut lines, "#### Problem TLDR", &self.problem_tldr);
        push_markdown_section(&mut lines, "#### Intuition", &self.intuition);
        push_markdown_section(&mut lines, "#### Approach", &self.approach);
        push_math_complexity_section(&mut lines, &self.time_complexity, &self.space_complexity);
        push_markdown_heading(&mut lines, "#### Code");
        push_code_block(&mut lines, "kotlin", &self.kotlin_code);
        push_code_block(&mut lines, "rust", &self.rust_code);

        finalize_markdown(lines)
    }

    pub fn telegram_template(&self) -> String {
        let mut lines = vec![format!("# {}", self.date_or_placeholder())];

        let problem_line = self.problem_header_line();
        if !problem_line.is_empty() {
            lines.push(problem_line);
        }
        lines.push(String::new());
        if !self.problem_tldr.trim().is_empty() {
            lines.push(self.problem_tldr.trim().to_string());
        }

        finalize_markdown(lines)
    }

    pub fn title_text(&self) -> String {
        let title = self.problem_title.trim();
        if title.is_empty() {
            format!("# {}", self.date_or_placeholder())
        } else {
            format!("# {} [{}]", self.date_or_placeholder(), title)
        }
    }

    pub fn subtitle_text(&self) -> String {
        self.problem_tldr.trim().to_string()
    }

    pub fn rich_html(&self) -> String {
        let mut html = String::from("<article>");
        html.push_str(&format!(
            "<h1>{}</h1>",
            escape_html(&self.date_or_placeholder())
        ));

        let problem_line = self.problem_header_html();
        if !problem_line.is_empty() {
            html.push_str(&format!("<p>{problem_line}</p>"));
        }

        push_optional_html_link(&mut html, "blog post", &self.blog_post_url);
        push_optional_html_link(&mut html, "substack", &self.substack_url);

        if !self.reference_url.trim().is_empty() {
            let safe_url = escape_html(&self.reference_url);
            html.push_str(&format!("<p><a href=\"{safe_url}\">{safe_url}</a></p>"));
        }

        push_optional_html_plain_link(&mut html, &self.youtube_url);
        push_html_section(&mut html, "Join me on Telegram", &self.telegram_text);
        push_html_section(&mut html, "Problem TLDR", &self.problem_tldr);
        push_html_section(&mut html, "Intuition", &self.intuition);
        push_html_section(&mut html, "Approach", &self.approach);

        html.push_str("<h4>Complexity</h4>");
        html.push_str(&format!(
            "<ul><li><strong>Time complexity:</strong> O({})</li><li><strong>Space complexity:</strong> O({})</li></ul>",
            escape_html(&self.complexity_value(&self.time_complexity)),
            escape_html(&self.complexity_value(&self.space_complexity)),
        ));

        html.push_str("<h4>Code</h4>");
        html.push_str(&format!(
            "<pre><code class=\"language-kotlin\">{}</code></pre>",
            escape_html(&self.kotlin_code)
        ));
        html.push_str(&format!(
            "<pre><code class=\"language-rust\">{}</code></pre>",
            escape_html(&self.rust_code)
        ));
        html.push_str("</article>");
        html
    }

    pub fn preview_tldr(&self) -> String {
        if self.problem_tldr.is_empty() {
            "Summarize the problem in one sharp sentence.".to_string()
        } else {
            self.problem_tldr.clone()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn suggested_export_path(&self) -> PathBuf {
        export_dir().join(self.suggested_export_filename())
    }

    pub fn suggested_export_filename(&self) -> String {
        format!("{}.webp", self.date_or_placeholder())
    }

    pub fn date_or_placeholder(&self) -> String {
        if self.date.is_empty() {
            today_date()
        } else {
            self.date.clone()
        }
    }

    pub fn difficulty_or_placeholder(&self) -> String {
        if self.difficulty.is_empty() {
            "medium".to_string()
        } else {
            self.difficulty.clone()
        }
    }

    fn complexity_value(&self, value: &str) -> String {
        if value.trim().is_empty() {
            String::new()
        } else {
            value.trim().to_string()
        }
    }

    fn problem_header_line(&self) -> String {
        let title = self.problem_title.trim();
        let difficulty = self.difficulty_or_placeholder();
        if title.is_empty() {
            difficulty
        } else if self.problem_url.trim().is_empty() {
            format!("{title} {difficulty}")
        } else {
            format!("{} {difficulty}", markdown_link(title, &self.problem_url))
        }
    }

    fn problem_header_html(&self) -> String {
        let title = self.problem_title.trim();
        let difficulty = escape_html(&self.difficulty_or_placeholder());
        if title.is_empty() {
            difficulty
        } else if self.problem_url.trim().is_empty() {
            format!("{} {difficulty}", escape_html(title))
        } else {
            format!("{} {difficulty}", html_link(title, &self.problem_url))
        }
    }

    fn image_markdown_line(&self) -> String {
        let filename = self.suggested_export_filename();
        format!("![{filename}](/assets/leetcode_daily_images/{filename})")
    }
}

pub fn persist_autosave(draft: &PostDraft) -> Result<()> {
    let encoded = encode_autosave(draft);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = autosave_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating autosave directory {}", parent.display()))?;
        }
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, encoded.as_bytes())
            .with_context(|| format!("writing autosave temp file {}", temp_path.display()))?;
        fs::rename(&temp_path, &path)
            .with_context(|| format!("moving autosave into place at {}", path.display()))?;
        return Ok(());
    }

    #[cfg(target_arch = "wasm32")]
    {
        let storage = local_storage()?;
        storage
            .set_item(AUTOSAVE_STORAGE_KEY, &encoded)
            .map_err(|error| anyhow!("saving autosave to local storage failed: {error:?}"))?;
        Ok(())
    }
}

pub fn load_ui_preferences() -> UiPreferences {
    match try_load_ui_preferences() {
        Ok(Some(preferences)) => preferences,
        _ => UiPreferences::default(),
    }
}

pub fn persist_ui_preferences(preferences: &UiPreferences) -> Result<()> {
    let encoded = encode_ui_preferences(preferences);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = ui_preferences_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("creating UI preferences directory {}", parent.display())
            })?;
        }
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, encoded.as_bytes())
            .with_context(|| format!("writing UI preferences temp file {}", temp_path.display()))?;
        fs::rename(&temp_path, &path)
            .with_context(|| format!("moving UI preferences into place at {}", path.display()))?;
        return Ok(());
    }

    #[cfg(target_arch = "wasm32")]
    {
        let storage = local_storage()?;
        storage
            .set_item(UI_PREFERENCES_STORAGE_KEY, &encoded)
            .map_err(|error| anyhow!("saving UI preferences to local storage failed: {error:?}"))?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_draft_snapshot(path: &Path, draft: &PostDraft) -> Result<()> {
    fs::write(path, encode_autosave(draft))
        .with_context(|| format!("writing draft snapshot {}", path.display()))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_draft_snapshot(path: &Path) -> Result<PostDraft> {
    let encoded = fs::read_to_string(path)
        .with_context(|| format!("reading draft snapshot {}", path.display()))?;
    decode_autosave(&encoded)
}

pub fn autosave_destination_label() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        return format!("Autosave: {}", autosave_path().display());
    }

    #[cfg(target_arch = "wasm32")]
    {
        "Autosave: browser local storage".to_string()
    }
}

pub fn startup_status_message() -> String {
    match load_autosave() {
        Ok(Some(_)) => "Restored autosaved draft.".to_string(),
        Ok(None) => "Preview refreshes when you open the Output tab.".to_string(),
        Err(error) => format!("Autosave restore failed: {error}"),
    }
}

fn load_autosave() -> Result<Option<PostDraft>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = autosave_path();
        if !path.exists() {
            return Ok(None);
        }
        let encoded = fs::read_to_string(&path)
            .with_context(|| format!("reading autosave file {}", path.display()))?;
        return Ok(Some(decode_autosave(&encoded)?.with_today_date()));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let storage = local_storage()?;
        let encoded = storage
            .get_item(AUTOSAVE_STORAGE_KEY)
            .map_err(|error| anyhow!("reading autosave from local storage failed: {error:?}"))?;
        match encoded {
            Some(contents) => Ok(Some(decode_autosave(&contents)?.with_today_date())),
            None => Ok(None),
        }
    }
}

fn try_load_ui_preferences() -> Result<Option<UiPreferences>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = ui_preferences_path();
        if !path.exists() {
            return Ok(None);
        }
        let encoded = fs::read_to_string(&path)
            .with_context(|| format!("reading UI preferences file {}", path.display()))?;
        return Ok(Some(decode_ui_preferences(&encoded)?));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let storage = local_storage()?;
        let encoded = storage
            .get_item(UI_PREFERENCES_STORAGE_KEY)
            .map_err(|error| {
                anyhow!("reading UI preferences from local storage failed: {error:?}")
            })?;
        match encoded {
            Some(contents) => Ok(Some(decode_ui_preferences(&contents)?)),
            None => Ok(None),
        }
    }
}

fn encode_autosave(draft: &PostDraft) -> String {
    let mut encoded = String::new();
    encoded.push_str(AUTOSAVE_FORMAT_VERSION);
    encoded.push('\n');

    for (name, value) in autosave_fields(draft) {
        encoded.push_str(name);
        encoded.push('\n');
        encoded.push_str(&value.len().to_string());
        encoded.push('\n');
        encoded.push_str(value);
        encoded.push('\n');
    }

    encoded
}

fn encode_ui_preferences(preferences: &UiPreferences) -> String {
    let mut encoded = String::new();
    encoded.push_str(UI_PREFERENCES_FORMAT_VERSION);
    encoded.push('\n');

    push_encoded_field(&mut encoded, "theme", preferences.theme.as_storage());
    for (component_key, order) in &preferences.component_order {
        push_encoded_field(
            &mut encoded,
            "component_order",
            &format!("{component_key}\t{order}"),
        );
    }
    for (button_key, count) in &preferences.button_counts {
        push_encoded_field(
            &mut encoded,
            "button_count",
            &format!("{button_key}\t{count}"),
        );
    }

    encoded
}

fn push_encoded_field(encoded: &mut String, name: &str, value: &str) {
    encoded.push_str(name);
    encoded.push('\n');
    encoded.push_str(&value.len().to_string());
    encoded.push('\n');
    encoded.push_str(value);
    encoded.push('\n');
}

fn decode_autosave(encoded: &str) -> Result<PostDraft> {
    let mut cursor = 0usize;
    let version = take_line(encoded, &mut cursor)?;
    if version != AUTOSAVE_FORMAT_VERSION {
        return Err(anyhow!("unsupported autosave format: {version}"));
    }

    let mut draft = PostDraft::default();
    while cursor < encoded.len() {
        let name = take_line(encoded, &mut cursor)?;
        if name.is_empty() && cursor >= encoded.len() {
            break;
        }
        let length = take_line(encoded, &mut cursor)?
            .parse::<usize>()
            .with_context(|| format!("parsing autosave field length for {name}"))?;
        let value = take_exact(encoded, &mut cursor, length)?;
        consume_optional_newline(encoded, &mut cursor);
        set_autosave_field(&mut draft, name, value);
    }

    Ok(draft)
}

fn decode_ui_preferences(encoded: &str) -> Result<UiPreferences> {
    let mut cursor = 0usize;
    let version = take_line(encoded, &mut cursor)?;
    if version != UI_PREFERENCES_FORMAT_VERSION {
        return Err(anyhow!("unsupported UI preferences format: {version}"));
    }

    let mut preferences = UiPreferences::default();
    while cursor < encoded.len() {
        let name = take_line(encoded, &mut cursor)?;
        if name.is_empty() && cursor >= encoded.len() {
            break;
        }
        let length = take_line(encoded, &mut cursor)?
            .parse::<usize>()
            .with_context(|| format!("parsing UI preferences field length for {name}"))?;
        let value = take_exact(encoded, &mut cursor, length)?;
        consume_optional_newline(encoded, &mut cursor);
        set_ui_preference_field(&mut preferences, name, value);
    }

    Ok(preferences)
}

fn set_ui_preference_field(preferences: &mut UiPreferences, name: &str, value: &str) {
    match name {
        "theme" => {
            if let Some(theme) = ThemeMode::from_storage(value) {
                preferences.theme = theme;
            }
        }
        "button_count" => {
            if let Some((key, count)) = value.split_once('\t')
                && let Ok(count) = count.parse::<u64>()
            {
                preferences.button_counts.insert(key.to_string(), count);
            }
        }
        "component_order" => {
            if let Some((key, order)) = value.split_once('\t')
                && let Ok(order) = order.parse::<u64>()
            {
                preferences.component_order.insert(key.to_string(), order);
            }
        }
        _ => {}
    }
}

fn take_line<'a>(encoded: &'a str, cursor: &mut usize) -> Result<&'a str> {
    if *cursor > encoded.len() {
        return Err(anyhow!("autosave parse cursor moved past input"));
    }
    let remaining = &encoded[*cursor..];
    let Some(line_end) = remaining.find('\n') else {
        return Err(anyhow!("autosave input ended unexpectedly"));
    };
    let line = &remaining[..line_end];
    *cursor += line_end + 1;
    Ok(line)
}

fn take_exact<'a>(encoded: &'a str, cursor: &mut usize, length: usize) -> Result<&'a str> {
    let end = (*cursor).saturating_add(length);
    if end > encoded.len() {
        return Err(anyhow!("autosave field exceeded input length"));
    }
    let value = &encoded[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn consume_optional_newline(encoded: &str, cursor: &mut usize) {
    if encoded[*cursor..].starts_with('\n') {
        *cursor += 1;
    }
}

fn autosave_fields(draft: &PostDraft) -> [(&'static str, &str); 18] {
    [
        ("date", &draft.date),
        ("problem_title", &draft.problem_title),
        ("problem_url", &draft.problem_url),
        ("difficulty", &draft.difficulty),
        ("blog_post_url", &draft.blog_post_url),
        ("substack_url", &draft.substack_url),
        ("youtube_url", &draft.youtube_url),
        ("reference_url", &draft.reference_url),
        ("telegram_text", &draft.telegram_text),
        ("problem_tldr", &draft.problem_tldr),
        ("intuition", &draft.intuition),
        ("approach", &draft.approach),
        ("time_complexity", &draft.time_complexity),
        ("space_complexity", &draft.space_complexity),
        ("kotlin_runtime_ms", &draft.kotlin_runtime_ms),
        ("kotlin_code", &draft.kotlin_code),
        ("rust_runtime_ms", &draft.rust_runtime_ms),
        ("rust_code", &draft.rust_code),
    ]
}

fn set_autosave_field(draft: &mut PostDraft, name: &str, value: &str) {
    match name {
        "date" => draft.date = value.to_string(),
        "problem_title" => draft.problem_title = value.to_string(),
        "problem_url" => draft.problem_url = value.to_string(),
        "difficulty" => draft.difficulty = value.to_string(),
        "blog_post_url" => draft.blog_post_url = value.to_string(),
        "substack_url" => draft.substack_url = value.to_string(),
        "youtube_url" => draft.youtube_url = value.to_string(),
        "reference_url" => draft.reference_url = value.to_string(),
        "telegram_text" => draft.telegram_text = value.to_string(),
        "problem_tldr" => draft.problem_tldr = value.to_string(),
        "intuition" => draft.intuition = value.to_string(),
        "approach" => draft.approach = value.to_string(),
        "time_complexity" => draft.time_complexity = value.to_string(),
        "space_complexity" => draft.space_complexity = value.to_string(),
        "kotlin_runtime_ms" => draft.kotlin_runtime_ms = value.to_string(),
        "kotlin_code" => draft.kotlin_code = value.to_string(),
        "rust_runtime_ms" => draft.rust_runtime_ms = value.to_string(),
        "rust_code" => draft.rust_code = value.to_string(),
        _ => {}
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn autosave_path() -> PathBuf {
    #[cfg(test)]
    {
        return std::env::temp_dir()
            .join("leetcodedaily-tests")
            .join("autosave.draft");
    }

    #[cfg(not(test))]
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".leetcodedaily").join("autosave.draft"))
        .unwrap_or_else(|| PathBuf::from(".leetcodedaily").join("autosave.draft"))
}

#[cfg(not(target_arch = "wasm32"))]
fn ui_preferences_path() -> PathBuf {
    #[cfg(test)]
    {
        return std::env::temp_dir()
            .join("leetcodedaily-tests")
            .join("ui-preferences.draft");
    }

    #[cfg(not(test))]
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".leetcodedaily").join("ui-preferences.draft"))
        .unwrap_or_else(|| PathBuf::from(".leetcodedaily").join("ui-preferences.draft"))
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Result<web_sys::Storage> {
    let window = web_sys::window().ok_or_else(|| anyhow!("missing window"))?;
    let storage = window
        .local_storage()
        .map_err(|error| anyhow!("accessing local storage failed: {error:?}"))?
        .ok_or_else(|| anyhow!("local storage is unavailable"))?;
    Ok(storage)
}

fn trim_or(value: String, fallback: &str) -> String {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

fn today_date() -> String {
    Local::now().format("%d.%m.%Y").to_string()
}

fn normalize_block(value: String) -> String {
    value.trim().to_string()
}

fn normalize_code(value: String) -> String {
    value.trim_matches('\n').to_string()
}

fn markdown_link(label: &str, url: &str) -> String {
    format!("[{}]({})", label.trim(), url.trim())
}

fn html_link(label: &str, url: &str) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        escape_html(url.trim()),
        escape_html(label.trim()),
    )
}

fn push_optional_link(lines: &mut Vec<String>, label: &str, url: &str) {
    let safe_url = url.trim();
    if !safe_url.is_empty() {
        lines.push(format!("[{label}]({safe_url})"));
    }
}

fn push_optional_plain_line(lines: &mut Vec<String>, value: &str) {
    if !value.trim().is_empty() {
        lines.push(value.trim().to_string());
    }
}

fn push_markdown_heading(lines: &mut Vec<String>, title: &str) {
    lines.push(title.to_string());
    lines.push(String::new());
}

fn push_markdown_section(lines: &mut Vec<String>, title: &str, body: &str) {
    push_markdown_heading(lines, title);
    push_markdown_body(lines, body);
    lines.push(String::new());
}

fn push_markdown_body(lines: &mut Vec<String>, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    lines.extend(body.lines().map(|line| line.to_string()));
}

fn push_math_complexity_section(lines: &mut Vec<String>, time: &str, space: &str) {
    push_markdown_heading(lines, "#### Complexity");
    lines.push("- Time complexity:".to_string());
    lines.push(format!("$$O({})$$", time.trim()));
    lines.push(String::new());
    lines.push("- Space complexity:".to_string());
    lines.push(format!("$$O({})$$", space.trim()));
    lines.push(String::new());
}

fn push_plain_complexity_section(lines: &mut Vec<String>, time: &str, space: &str) {
    push_markdown_heading(lines, "#### Complexity");
    lines.push("- Time complexity:".to_string());
    lines.push(format!("O({})", time.trim()));
    lines.push(String::new());
    lines.push("- Space complexity:".to_string());
    lines.push(format!("O({})", space.trim()));
    lines.push(String::new());
}

fn push_runtime_code_block(
    lines: &mut Vec<String>,
    language: &str,
    runtime_language: &str,
    runtime_ms: &str,
    code: &str,
) {
    lines.push(format!(
        "```{language} [-{runtime_language} {}]\n{}\n```",
        runtime_label(runtime_ms),
        code
    ));
}

fn push_code_block(lines: &mut Vec<String>, language: &str, code: &str) {
    lines.push(format!("```{language}\n{}\n```", code));
}

fn finalize_markdown(mut lines: Vec<String>) -> String {
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    format!("{}\n", lines.join("\n"))
}

fn push_optional_html_link(html: &mut String, label: &str, url: &str) {
    let safe_url = url.trim();
    if !safe_url.is_empty() {
        html.push_str(&format!("<p>{}</p>", html_link(label, safe_url)));
    }
}

fn push_optional_html_plain_link(html: &mut String, url: &str) {
    let safe_url = url.trim();
    if !safe_url.is_empty() {
        html.push_str(&format!("<p>{}</p>", html_link(safe_url, safe_url)));
    }
}

fn push_html_section(html: &mut String, title: &str, body: &str) {
    html.push_str(&format!("<h4>{}</h4>", escape_html(title)));
    if !body.trim().is_empty() {
        html.push_str(&format!("<p>{}</p>", html_block(body)));
    }
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn html_block(value: &str) -> String {
    value
        .split('\n')
        .map(escape_html)
        .collect::<Vec<_>>()
        .join("<br>")
}

#[cfg(all(not(target_arch = "wasm32"), test))]
fn export_dir() -> PathBuf {
    std::env::temp_dir().join("leetcodedaily-tests")
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn export_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Downloads"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::{
        PostDraft, ThemeMode, UiPreferences, decode_autosave, decode_ui_preferences,
        encode_autosave, encode_ui_preferences, slugify, today_date,
    };

    #[test]
    fn slugify_keeps_only_ascii_words() {
        assert_eq!(slugify("05.10.2025 Daily Post"), "05-10-2025-daily-post");
        assert_eq!(
            slugify(" Words   Within Two Edits "),
            "words-within-two-edits"
        );
    }

    #[test]
    fn markdown_keeps_expected_sections() {
        let draft = PostDraft {
            date: "05.10.2025".to_string(),
            problem_title: "Words Within Two Edits of Dictionary".to_string(),
            problem_url: "https://leetcode.com/problems/words-within-two-edits-of-dictionary/"
                .to_string(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: String::new(),
            reference_url: "https://example.com".to_string(),
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

        let markdown = draft.markdown();

        assert!(markdown.contains("# 05.10.2025"));
        assert!(markdown.contains("[Words Within Two Edits of Dictionary]("));
        assert!(
            markdown.contains("![05.10.2025.webp](/assets/leetcode_daily_images/05.10.2025.webp)")
        );
        assert!(markdown.contains("#### Problem TLDR"));
        assert!(markdown.contains("```kotlin\nfun demo() {}\n```"));
        assert!(markdown.contains("```rust\nfn demo() {}\n```"));
        assert!(!markdown.contains("[substack]()"));
        assert!(!markdown.contains("[youtube]()"));
    }

    #[test]
    fn markdown_skips_empty_urls_and_uses_plain_title_without_problem_link() {
        let draft = PostDraft {
            date: "05.10.2025".to_string(),
            problem_title: "Words Within Two Edits of Dictionary".to_string(),
            problem_url: String::new(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: String::new(),
            reference_url: String::new(),
            telegram_text: String::new(),
            problem_tldr: String::new(),
            intuition: String::new(),
            approach: String::new(),
            time_complexity: String::new(),
            space_complexity: String::new(),
            kotlin_runtime_ms: String::new(),
            kotlin_code: String::new(),
            rust_runtime_ms: String::new(),
            rust_code: String::new(),
        };

        let markdown = draft.markdown();

        assert!(markdown.contains("Words Within Two Edits of Dictionary medium"));
        assert!(!markdown.contains("[Words Within Two Edits of Dictionary]("));
        assert!(!markdown.contains("https://dmitrysamoylenko.com/2023/07/14/leetcode_daily.html"));
    }

    #[test]
    fn template_export_filename_uses_date_only() {
        let draft = PostDraft {
            date: "23.04.2026".to_string(),
            problem_title: "Sum of Distances".to_string(),
            problem_url: String::new(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: String::new(),
            reference_url: String::new(),
            telegram_text: String::new(),
            problem_tldr: String::new(),
            intuition: String::new(),
            approach: String::new(),
            time_complexity: String::new(),
            space_complexity: String::new(),
            kotlin_runtime_ms: String::new(),
            kotlin_code: String::new(),
            rust_runtime_ms: String::new(),
            rust_code: String::new(),
        };

        assert_eq!(draft.suggested_export_filename(), "23.04.2026.webp");
    }

    #[test]
    fn title_and_subtitle_copy_text_match_expected_shape() {
        let draft = PostDraft {
            date: "24.04.2026".to_string(),
            problem_title: "2833. Furthest Point From Origin".to_string(),
            problem_tldr: "Max dist when replace _ with L or R".to_string(),
            ..PostDraft::default()
        };

        assert_eq!(
            draft.title_text(),
            "# 24.04.2026 [2833. Furthest Point From Origin]"
        );
        assert_eq!(draft.subtitle_text(), "Max dist when replace _ with L or R");
    }

    #[test]
    fn specialized_templates_match_expected_shapes() {
        let draft = PostDraft {
            date: "23.04.2026".to_string(),
            problem_title: "2615. Sum of Distances".to_string(),
            problem_url: "https://leetcode.com/problems/sum-of-distances/solutions/8069058/kotlin-rust-by-samoylenkodmitry-9mqf/".to_string(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: "https://open.substack.com/example".to_string(),
            youtube_url: "https://youtu.be/848sypoVAUs".to_string(),
            reference_url: "https://dmitrysamoylenko.com/2023/07/14/leetcode_daily.html".to_string(),
            telegram_text: "https://t.me/leetcode_daily_unstoppable/1337".to_string(),
            problem_tldr: "Sum of distances to each occurrence".to_string(),
            intuition: "Think".to_string(),
            approach: "Do".to_string(),
            time_complexity: "n".to_string(),
            space_complexity: "n".to_string(),
            kotlin_runtime_ms: "65".to_string(),
            kotlin_code: "fun demo() {}".to_string(),
            rust_runtime_ms: "11".to_string(),
            rust_code: "fn demo() {}".to_string(),
        };

        let leetcode = draft.leetcode_template();
        let youtube = draft.youtube_template();
        let telegram = draft.telegram_template();

        assert!(leetcode.starts_with("https://youtu.be/848sypoVAUs"));
        assert!(leetcode.contains("```kotlin [-Kotlin 65ms]"));
        assert!(!leetcode.contains("# 23.04.2026"));

        assert!(youtube.contains("[substack](https://open.substack.com/example)"));
        assert!(
            youtube.contains(
                "#### Code\n\nhttps://dmitrysamoylenko.com/2023/07/14/leetcode_daily.html"
            )
        );
        assert!(youtube.contains("O(n)"));
        assert!(!youtube.contains("$$O(n)$$"));

        assert!(telegram.contains("# 23.04.2026"));
        assert!(telegram.contains("Sum of distances to each occurrence"));
        assert!(!telegram.contains("#### Problem TLDR"));
    }

    #[test]
    fn rich_html_omits_empty_optional_links() {
        let draft = PostDraft {
            date: "05.10.2025".to_string(),
            problem_title: "Words Within Two Edits of Dictionary".to_string(),
            problem_url: String::new(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: String::new(),
            reference_url: String::new(),
            telegram_text: "Join".to_string(),
            problem_tldr: "TLDR".to_string(),
            intuition: "Think".to_string(),
            approach: "Do".to_string(),
            time_complexity: "n".to_string(),
            space_complexity: "1".to_string(),
            kotlin_runtime_ms: "28".to_string(),
            kotlin_code: "fun demo() {}".to_string(),
            rust_runtime_ms: "1".to_string(),
            rust_code: "fn demo() {}".to_string(),
        };

        let html = draft.rich_html();

        assert!(html.contains("<h1>05.10.2025</h1>"));
        assert!(html.contains("<h4>Problem TLDR</h4>"));
        assert!(html.contains("Words Within Two Edits of Dictionary medium"));
        assert!(!html.contains("blog post</a>"));
        assert!(html.contains("language-kotlin"));
        assert!(html.contains("language-rust"));
    }

    #[test]
    fn rich_text_copy_uses_visible_plain_youtube_url() {
        let draft = PostDraft {
            date: "05.10.2025".to_string(),
            problem_title: "Words Within Two Edits of Dictionary".to_string(),
            problem_url: String::new(),
            difficulty: "medium".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: "https://youtu.be/demo".to_string(),
            reference_url: String::new(),
            telegram_text: "Join".to_string(),
            problem_tldr: "TLDR".to_string(),
            intuition: "Think".to_string(),
            approach: "Do".to_string(),
            time_complexity: "n".to_string(),
            space_complexity: "1".to_string(),
            kotlin_runtime_ms: "28".to_string(),
            kotlin_code: "fun demo() {}".to_string(),
            rust_runtime_ms: "1".to_string(),
            rust_code: "fn demo() {}".to_string(),
        };

        let html = draft.rich_html();
        let fallback = draft.rich_text_fallback();

        assert!(html.contains(
            "<p><a href=\"https://youtu.be/demo\">https://youtu.be/demo</a></p><h4>Join me on Telegram</h4>"
        ));
        assert!(!html.contains(">youtube</a>"));
        assert!(fallback.contains("https://youtu.be/demo\n\n#### Join me on Telegram"));
        assert!(!fallback.contains("[youtube](https://youtu.be/demo)"));
    }

    #[test]
    fn autosave_roundtrip_preserves_multiline_content() {
        let draft = PostDraft {
            date: "23.04.2026".to_string(),
            problem_title: "2615. Sum of Distances".to_string(),
            problem_url: "https://leetcode.com/problems/sum-of-distances/".to_string(),
            difficulty: "medium".to_string(),
            blog_post_url: "https://example.com/blog".to_string(),
            substack_url: "https://example.com/substack".to_string(),
            youtube_url: "https://youtu.be/demo".to_string(),
            reference_url: "https://example.com/reference".to_string(),
            telegram_text: "t.me/demo/1337".to_string(),
            problem_tldr: "Sum of distances to each occurrence".to_string(),
            intuition: "First line\nSecond line".to_string(),
            approach: "Forward and backward".to_string(),
            time_complexity: "n".to_string(),
            space_complexity: "n".to_string(),
            kotlin_runtime_ms: "65".to_string(),
            kotlin_code: "fun demo() {\n    println(\"hi\")\n}".to_string(),
            rust_runtime_ms: "11".to_string(),
            rust_code: "fn demo() {\n    println!(\"hi\");\n}".to_string(),
        };

        let encoded = encode_autosave(&draft);
        let decoded = decode_autosave(&encoded).expect("decode autosave");

        assert_eq!(decoded, draft);
    }

    #[test]
    fn ui_preferences_roundtrip_preserves_theme_and_button_counts() {
        let mut preferences = UiPreferences {
            theme: ThemeMode::Light,
            ..UiPreferences::default()
        };
        preferences.increment_button_count("copy.leetcode");
        preferences.increment_button_count("copy.leetcode");
        preferences.increment_button_count("field.problem_title.clear");
        preferences.mark_component_used("field.problem_title");
        preferences.mark_component_used("copy.leetcode");

        let encoded = encode_ui_preferences(&preferences);
        let decoded = decode_ui_preferences(&encoded).expect("decode UI preferences");

        assert_eq!(decoded.theme, ThemeMode::Light);
        assert_eq!(decoded.button_count("copy.leetcode"), 2);
        assert_eq!(decoded.button_count("field.problem_title.clear"), 1);
        assert_eq!(decoded.button_count("missing"), 0);
        assert_eq!(decoded.component_order("field.problem_title"), 1);
        assert_eq!(decoded.component_order("copy.leetcode"), 2);
        assert_eq!(decoded.component_order("missing"), 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn desktop_ui_preferences_persist_and_restore_from_disk() {
        let mut preferences = UiPreferences {
            theme: ThemeMode::Light,
            ..UiPreferences::default()
        };
        preferences.increment_button_count("copy.blog");
        preferences.increment_button_count("copy.blog");
        preferences.increment_button_count("theme.toggle");
        preferences.mark_component_used("copy.blog");

        let path = super::ui_preferences_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(&path);

        super::persist_ui_preferences(&preferences).expect("persist UI preferences");
        let restored = super::try_load_ui_preferences()
            .expect("load UI preferences")
            .expect("UI preferences should exist");

        assert_eq!(restored.theme, ThemeMode::Light);
        assert_eq!(restored.button_count("copy.blog"), 2);
        assert_eq!(restored.button_count("theme.toggle"), 1);
        assert_eq!(restored.component_order("copy.blog"), 1);

        let _ = std::fs::remove_file(path);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn desktop_autosave_persists_and_restores_from_disk() {
        let draft = PostDraft {
            date: "01.01.2000".to_string(),
            problem_title: "Desktop Autosave".to_string(),
            problem_url: "https://example.com/problem".to_string(),
            difficulty: "easy".to_string(),
            blog_post_url: String::new(),
            substack_url: String::new(),
            youtube_url: String::new(),
            reference_url: "https://example.com/ref".to_string(),
            telegram_text: "t.me/example/1".to_string(),
            problem_tldr: "Restore from disk".to_string(),
            intuition: "Smoke test".to_string(),
            approach: "Write then read".to_string(),
            time_complexity: "1".to_string(),
            space_complexity: "1".to_string(),
            kotlin_runtime_ms: "1".to_string(),
            kotlin_code: "fun demo() = Unit".to_string(),
            rust_runtime_ms: "1".to_string(),
            rust_code: "fn demo() {}".to_string(),
        };

        let path = super::autosave_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(&path);

        super::persist_autosave(&draft).expect("persist autosave");
        let restored = super::load_autosave()
            .expect("load autosave")
            .expect("autosave should exist");

        let expected = PostDraft {
            date: today_date(),
            ..draft
        };
        assert_eq!(restored, expected);

        let _ = std::fs::remove_file(path);
    }
}
