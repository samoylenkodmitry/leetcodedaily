use crate::draft::PostDraft;
use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const DEFAULT_BLOG_REPO: &str = "/home/s/develop/projects/s-a--m.github.io";
const BLOG_REPO_ENV: &str = "LEETCODE_DAILY_BLOG_REPO";
const ARCHIVE_RELATIVE: &str = "_leetcode_source/2023-07-14-leetcode_daily.md";
const IMAGE_DIR_RELATIVE: &str = "assets/leetcode_daily_images";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ArchiveEdit {
    Inserted,
    Replaced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlogPublishResult {
    pub archive_path: PathBuf,
    pub image_path: PathBuf,
    pub edit: ArchiveEdit,
    pub commit_sha: Option<String>,
}

pub(crate) fn publish_blog_post(
    draft: &PostDraft,
    source_image_path: impl AsRef<Path>,
) -> Result<BlogPublishResult> {
    let repo = blog_repo_path();
    let source_image_path = source_image_path.as_ref();
    let archive_path = repo.join(ARCHIVE_RELATIVE);
    let image_path = repo
        .join(IMAGE_DIR_RELATIVE)
        .join(draft.suggested_export_filename());

    ensure_publish_preconditions(&repo, source_image_path)?;

    let archive = fs::read_to_string(&archive_path)
        .with_context(|| format!("reading blog archive {}", archive_path.display()))?;
    let (updated_archive, edit) = upsert_archive_entry(
        &archive,
        &draft.blog_template(),
        &draft.date_or_placeholder(),
    )?;

    if let Some(parent) = image_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating image directory {}", parent.display()))?;
    }
    fs::write(&archive_path, updated_archive)
        .with_context(|| format!("writing blog archive {}", archive_path.display()))?;
    fs::copy(source_image_path, &image_path).with_context(|| {
        format!(
            "copying WebP from {} to {}",
            source_image_path.display(),
            image_path.display()
        )
    })?;

    git(
        &repo,
        ["add", ARCHIVE_RELATIVE, &image_relative_path(draft)],
    )?;

    let commit_sha = if git_quiet(&repo, ["diff", "--cached", "--quiet"])? {
        None
    } else {
        let verb = match edit {
            ArchiveEdit::Inserted => "Add",
            ArchiveEdit::Replaced => "Update",
        };
        git(
            &repo,
            [
                "commit",
                "-m",
                &format!("{verb} LeetCode daily {}", draft.date_or_placeholder()),
            ],
        )?;
        Some(
            git(&repo, ["rev-parse", "--short", "HEAD"])?
                .trim()
                .to_string(),
        )
    };

    Ok(BlogPublishResult {
        archive_path,
        image_path,
        edit,
        commit_sha,
    })
}

fn blog_repo_path() -> PathBuf {
    std::env::var_os(BLOG_REPO_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_BLOG_REPO))
}

fn ensure_publish_preconditions(repo: &Path, source_image_path: &Path) -> Result<()> {
    if !repo.join(".git").exists() {
        return Err(anyhow!(
            "blog repo is not a git checkout: {}",
            repo.display()
        ));
    }
    if !source_image_path.exists() {
        return Err(anyhow!(
            "source WebP does not exist: {}",
            source_image_path.display()
        ));
    }

    let staged = git(repo, ["diff", "--cached", "--name-only"])?;
    if !staged.trim().is_empty() {
        return Err(anyhow!(
            "blog repo already has staged changes; commit or unstage them first:\n{}",
            staged.trim()
        ));
    }

    Ok(())
}

pub(crate) fn upsert_archive_entry(
    archive: &str,
    post: &str,
    date: &str,
) -> Result<(String, ArchiveEdit)> {
    let headings = post_heading_offsets(archive);
    let target = format!("# {date}");
    let entry = format!("{}\n\n", post.trim_end());

    if let Some((index, (start, _))) = headings
        .iter()
        .enumerate()
        .find(|(_, (_, line))| *line == target)
    {
        let end = headings
            .get(index + 1)
            .map(|(offset, _)| *offset)
            .unwrap_or(archive.len());
        let mut updated = String::with_capacity(archive.len() + entry.len());
        updated.push_str(&archive[..*start]);
        updated.push_str(&entry);
        updated.push_str(&archive[end..]);
        return Ok((updated, ArchiveEdit::Replaced));
    }

    let Some((first_post_start, _)) = headings.first() else {
        return Err(anyhow!(
            "could not find first dated post heading in archive"
        ));
    };

    let mut updated = String::with_capacity(archive.len() + entry.len() + 2);
    updated.push_str(archive[..*first_post_start].trim_end());
    updated.push_str("\n\n");
    updated.push_str(&entry);
    updated.push_str(&archive[*first_post_start..]);
    Ok((updated, ArchiveEdit::Inserted))
}

fn post_heading_offsets(archive: &str) -> Vec<(usize, String)> {
    let mut offsets = Vec::new();
    let mut offset = 0usize;

    for raw_line in archive.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if is_post_heading(line) {
            offsets.push((offset, line.to_string()));
        }
        offset += raw_line.len();
    }

    offsets
}

fn is_post_heading(line: &str) -> bool {
    let Some(date) = line.strip_prefix("# ") else {
        return false;
    };
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b'.'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && bytes[5] == b'.'
        && bytes[6].is_ascii_digit()
        && bytes[7].is_ascii_digit()
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
}

fn image_relative_path(draft: &PostDraft) -> String {
    format!("{IMAGE_DIR_RELATIVE}/{}", draft.suggested_export_filename())
}

fn git<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git in {}", repo.display()))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        Err(anyhow!("git failed in {}: {message}", repo.display()))
    }
}

fn git_quiet<const N: usize>(repo: &Path, args: [&str; N]) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git in {}", repo.display()))?;

    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "git failed in {}: {}",
                repo.display(),
                stderr.trim()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ArchiveEdit, upsert_archive_entry};

    #[test]
    fn upsert_archive_entry_inserts_before_latest_post() {
        let archive =
            "---\nlayout: post\n---\n\nIntro\n\n# 24.04.2026\nold\n\n# 23.04.2026\nolder\n";
        let post = "# 25.04.2026\nnew";

        let (updated, edit) =
            upsert_archive_entry(archive, post, "25.04.2026").expect("insert entry");

        assert_eq!(edit, ArchiveEdit::Inserted);
        assert!(updated.contains("Intro\n\n# 25.04.2026\nnew\n\n# 24.04.2026\nold"));
    }

    #[test]
    fn upsert_archive_entry_replaces_existing_post() {
        let archive = "Intro\n\n# 25.04.2026\nold\nold body\n\n# 24.04.2026\nprevious\n";
        let post = "# 25.04.2026\nnew body";

        let (updated, edit) =
            upsert_archive_entry(archive, post, "25.04.2026").expect("replace entry");

        assert_eq!(edit, ArchiveEdit::Replaced);
        assert!(updated.contains("Intro\n\n# 25.04.2026\nnew body\n\n# 24.04.2026\nprevious"));
        assert!(!updated.contains("old body"));
    }
}
