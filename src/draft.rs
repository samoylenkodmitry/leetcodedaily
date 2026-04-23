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
        push_optional_html_link(&mut html, "youtube", &self.youtube_url);

        if !self.reference_url.trim().is_empty() {
            let safe_url = escape_html(&self.reference_url);
            html.push_str(&format!("<p><a href=\"{safe_url}\">{safe_url}</a></p>"));
        }

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
}
