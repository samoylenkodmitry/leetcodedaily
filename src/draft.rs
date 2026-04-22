use chrono::Local;
use cranpose_foundation::text::TextFieldState;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

const DEFAULT_REFERENCE_URL: &str = "https://dmitrysamoylenko.com/2023/07/14/leetcode_daily.html";

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
        Self {
            date: TextFieldState::new(Local::now().format("%d.%m.%Y").to_string()),
            problem_title: TextFieldState::new("Words Within Two Edits of Dictionary"),
            problem_url: TextFieldState::new(
                "https://leetcode.com/problems/words-within-two-edits-of-dictionary/description/",
            ),
            difficulty: TextFieldState::new("medium"),
            blog_post_url: TextFieldState::new(""),
            substack_url: TextFieldState::new(""),
            youtube_url: TextFieldState::new(""),
            reference_url: TextFieldState::new(DEFAULT_REFERENCE_URL),
            telegram_text: TextFieldState::new(""),
            problem_tldr: TextFieldState::new("Words in dictionary with 2 edits"),
            intuition: TextFieldState::new(
                "Compare every query word against the dictionary and keep it if any dictionary word differs in fewer than three positions.",
            ),
            approach: TextFieldState::new(
                "Use a direct scan. For each query word, count character mismatches against every dictionary candidate and stop as soon as one candidate stays under three mismatches.",
            ),
            time_complexity: TextFieldState::new("n * m * k"),
            space_complexity: TextFieldState::new("1"),
            kotlin_runtime_ms: TextFieldState::new("28"),
            kotlin_code: TextFieldState::new(
                "fun twoEditWords(q: Array<String>, d: Array<String>) =\n    q.filter { q -> d.any { d -> d.indices.count { d[it] != q[it] } < 3 } }",
            ),
            rust_runtime_ms: TextFieldState::new("1"),
            rust_code: TextFieldState::new(
                "pub fn two_edit_words(mut q: Vec<String>, d: Vec<String>) -> Vec<String> {\n    q.retain(|q| d.iter().any(|d| d.bytes().zip(q.bytes()).filter(|(d, q)| d != q).count() < 3));\n    q\n}",
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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

impl PostDraft {
    pub fn from_fields(fields: &EditorFields) -> Self {
        Self {
            date: trim_or(
                fields.date.text(),
                &Local::now().format("%d.%m.%Y").to_string(),
            ),
            problem_title: trim_or(fields.problem_title.text(), ""),
            problem_url: trim_or(fields.problem_url.text(), ""),
            difficulty: trim_or(fields.difficulty.text(), "medium"),
            blog_post_url: trim_or(fields.blog_post_url.text(), ""),
            substack_url: trim_or(fields.substack_url.text(), ""),
            youtube_url: trim_or(fields.youtube_url.text(), ""),
            reference_url: trim_or(fields.reference_url.text(), DEFAULT_REFERENCE_URL),
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

    pub fn markdown(&self) -> String {
        format!(
            "# {date}\n{problem_line} {difficulty}\n[blog post]({blog_post_url})\n[substack]({substack_url})\n[youtube]({youtube_url})\n\n\n\n{reference_url}\n\n#### Join me on Telegram\n\n{telegram_text}\n\n#### Problem TLDR\n\n{problem_tldr}\n\n#### Intuition\n\n{intuition}\n\n#### Approach\n\n{approach}\n\n#### Complexity\n\n- Time complexity:\n$$O({time_complexity})$$\n\n- Space complexity:\n$$O({space_complexity})$$\n\n#### Code\n\n```kotlin [-Kotlin {kotlin_runtime_label}]\n{kotlin_code}\n```\n```rust [-Rust {rust_runtime_label}]\n{rust_code}\n```\n",
            date = self.date_or_placeholder(),
            problem_line = markdown_link(&self.problem_title, &self.problem_url),
            difficulty = self.difficulty_or_placeholder(),
            blog_post_url = self.blog_post_url,
            substack_url = self.substack_url,
            youtube_url = self.youtube_url,
            reference_url = if self.reference_url.is_empty() {
                DEFAULT_REFERENCE_URL.to_string()
            } else {
                self.reference_url.clone()
            },
            telegram_text = self.telegram_text,
            problem_tldr = self.problem_tldr,
            intuition = self.intuition,
            approach = self.approach,
            time_complexity = self.complexity_value(&self.time_complexity),
            space_complexity = self.complexity_value(&self.space_complexity),
            kotlin_runtime_label = runtime_label(&self.kotlin_runtime_ms),
            kotlin_code = self.kotlin_code,
            rust_runtime_label = runtime_label(&self.rust_runtime_ms),
            rust_code = self.rust_code,
        )
    }

    pub fn preview_title(&self) -> String {
        if self.problem_title.is_empty() {
            "LeetCode Daily".to_string()
        } else {
            self.problem_title.clone()
        }
    }

    pub fn preview_badge(&self) -> String {
        let title = if self.problem_tldr.is_empty() {
            self.preview_title()
        } else {
            self.problem_tldr.clone()
        };
        let difficulty = self.difficulty_or_placeholder();
        format!("{title} #{difficulty}")
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
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let slug = slugify(if self.problem_title.is_empty() {
            "leetcode-daily"
        } else {
            &self.problem_title
        });
        let date_slug = slugify(&self.date_or_placeholder());
        manifest_dir
            .join("output")
            .join(format!("{date_slug}-{slug}.webp"))
    }

    pub fn date_or_placeholder(&self) -> String {
        if self.date.is_empty() {
            Local::now().format("%d.%m.%Y").to_string()
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
}

fn trim_or(value: String, fallback: &str) -> String {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

fn normalize_block(value: String) -> String {
    value.trim().to_string()
}

fn normalize_code(value: String) -> String {
    value.trim_matches('\n').to_string()
}

fn markdown_link(label: &str, url: &str) -> String {
    let safe_label = label.trim();
    let safe_url = url.trim();
    if safe_label.is_empty() && safe_url.is_empty() {
        "[]()".to_string()
    } else {
        format!("[{safe_label}]({safe_url})")
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

#[cfg(test)]
mod tests {
    use super::{PostDraft, slugify};

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
        assert!(markdown.contains("#### Problem TLDR"));
        assert!(markdown.contains("```kotlin [-Kotlin 28ms]"));
        assert!(markdown.contains("```rust [-Rust 1ms]"));
    }
}
