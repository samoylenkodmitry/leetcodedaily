#!/usr/bin/env python3

from __future__ import annotations

import html
import json
import os
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

CONFIG_PATH = Path.home() / ".config" / "leetcodedaily" / "telegram.env"
STATE_PATH = Path.home() / ".config" / "leetcodedaily" / "telegram_last_post.json"
API_BASE = "https://api.telegram.org/bot"
DEFAULT_CHANNEL_ID = "@leetcode_daily_unstoppable"
DEFAULT_DISCUSSION_CHAT_ID = "@leetcode_daily_chat"


class TelegramError(RuntimeError):
    pass


def load_env_file(path: Path = CONFIG_PATH) -> None:
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        os.environ.setdefault(key, value)


def bot_token() -> str:
    load_env_file()
    token = os.environ.get("TELEGRAM_BOT_TOKEN", "").strip()
    if not token:
        raise TelegramError(
            f"TELEGRAM_BOT_TOKEN is missing; set it in the shell or {CONFIG_PATH}"
        )
    return token


def channel_id() -> str:
    load_env_file()
    return os.environ.get("TELEGRAM_CHANNEL_ID", DEFAULT_CHANNEL_ID).strip()


def discussion_chat_id() -> str:
    load_env_file()
    return os.environ.get("TELEGRAM_DISCUSSION_CHAT_ID", DEFAULT_DISCUSSION_CHAT_ID).strip()


def api_request(method: str, fields: dict[str, Any] | None = None, files: dict[str, Path] | None = None) -> dict[str, Any]:
    token = bot_token()
    url = f"{API_BASE}{token}/{method}"
    fields = fields or {}

    if files:
        body, content_type = multipart_body(fields, files)
        request = urllib.request.Request(
            url,
            data=body,
            headers={"Content-Type": content_type},
            method="POST",
        )
    else:
        data = urllib.parse.urlencode({key: encode_field(value) for key, value in fields.items()}).encode()
        request = urllib.request.Request(url, data=data, method="POST")

    try:
        with urllib.request.urlopen(request, timeout=45) as response:
            payload = response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        payload = error.read().decode("utf-8", errors="replace")
        raise TelegramError(f"{method} failed: {payload}") from error
    except urllib.error.URLError as error:
        raise TelegramError(f"{method} failed: {error}") from error

    data = json.loads(payload)
    if not data.get("ok"):
        raise TelegramError(f"{method} failed: {payload}")
    return data["result"]


def encode_field(value: Any) -> str:
    if isinstance(value, (dict, list)):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def multipart_body(fields: dict[str, Any], files: dict[str, Path]) -> tuple[bytes, str]:
    boundary = f"----leetcodedaily{int(time.time() * 1000)}"
    chunks: list[bytes] = []
    for key, value in fields.items():
        chunks.extend(
            [
                f"--{boundary}\r\n".encode(),
                f'Content-Disposition: form-data; name="{key}"\r\n\r\n'.encode(),
                encode_field(value).encode("utf-8"),
                b"\r\n",
            ]
        )
    for key, path in files.items():
        filename = path.name
        chunks.extend(
            [
                f"--{boundary}\r\n".encode(),
                f'Content-Disposition: form-data; name="{key}"; filename="{filename}"\r\n'.encode(),
                b"Content-Type: application/octet-stream\r\n\r\n",
                path.read_bytes(),
                b"\r\n",
            ]
        )
    chunks.append(f"--{boundary}--\r\n".encode())
    return b"".join(chunks), f"multipart/form-data; boundary={boundary}"


def chat_username(chat_id: str) -> str | None:
    if chat_id.startswith("@"):
        return chat_id[1:]
    return None


def channel_link(message_id: int, chat_id: str | None = None) -> str:
    username = chat_username(chat_id or channel_id())
    if username:
        return f"https://t.me/{username}/{message_id}"
    return f"channel message {message_id}"


def comment_link(channel_message_id: int, comment_message_id: int, chat_id: str | None = None) -> str:
    username = chat_username(chat_id or channel_id())
    if username:
        return f"https://t.me/{username}/{channel_message_id}?comment={comment_message_id}"
    return f"comment message {comment_message_id}"


def save_state(state: dict[str, Any]) -> None:
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    STATE_PATH.write_text(json.dumps(state, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    STATE_PATH.chmod(0o600)


def load_state() -> dict[str, Any]:
    if not STATE_PATH.exists():
        return {}
    return json.loads(STATE_PATH.read_text(encoding="utf-8"))


def normalize_url(value: str) -> str:
    return value.strip()


def build_channel_caption(args: Any) -> str:
    lines = [f"# {args.date.strip()}"]
    if args.title.strip():
        lines.append(args.title.strip())
    if args.difficulty.strip():
        lines.append(args.difficulty.strip())
    lines.append("")
    if args.tldr.strip():
        lines.append(args.tldr.strip())
        lines.append("")
    for label, url in [
        ("blog", args.blog_url),
        ("substack", args.substack_url),
        ("youtube", args.youtube_url),
    ]:
        url = normalize_url(url)
        if url:
            lines.append(f"{label}: {url}")
    return trim_caption("\n".join(lines))


def trim_caption(caption: str) -> str:
    caption = caption.strip()
    if len(caption) <= 1024:
        return caption
    return caption[:1018].rstrip() + " ..."


CODE_FENCE_RE = re.compile(r"```([A-Za-z0-9_+-]*)\n(.*?)\n```", re.DOTALL)
MARKDOWN_LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
IMAGE_RE = re.compile(r"^!\[[^\]]*\]\([^)]+\)$")


def markdown_to_telegram_html(markdown: str) -> str:
    output: list[str] = []
    cursor = 0
    for match in CODE_FENCE_RE.finditer(markdown):
        output.extend(convert_text_block(markdown[cursor : match.start()]))
        language = match.group(1).strip()
        output.extend(spoiler_code_blocks(language, match.group(2).strip("\n")))
        cursor = match.end()
    output.extend(convert_text_block(markdown[cursor:]))
    return "\n".join(line for line in output if line is not None).strip()


def spoiler_code_blocks(language: str, code: str, chunk_limit: int = 2800) -> list[str]:
    # Telegram does not allow spoiler entities to contain pre/code entities.
    prefix = f"{language}\n" if language else ""
    chunks = split_code_for_telegram(code, chunk_limit)
    return [f"<tg-spoiler>{html.escape(prefix + chunk)}</tg-spoiler>" for chunk in chunks]


def split_code_for_telegram(code: str, chunk_limit: int) -> list[str]:
    if len(code) <= chunk_limit:
        return [code]

    chunks: list[str] = []
    current: list[str] = []
    current_len = 0
    for line in code.splitlines():
        line_len = len(line) + 1
        if current and current_len + line_len > chunk_limit:
            chunks.append("\n".join(current))
            current = [line]
            current_len = line_len
        elif line_len > chunk_limit:
            if current:
                chunks.append("\n".join(current))
                current = []
                current_len = 0
            chunks.extend(
                line[index : index + chunk_limit]
                for index in range(0, len(line), chunk_limit)
            )
        else:
            current.append(line)
            current_len += line_len
    if current:
        chunks.append("\n".join(current))
    return chunks


def convert_text_block(block: str) -> list[str]:
    lines: list[str] = []
    for raw_line in block.splitlines():
        line = raw_line.rstrip()
        if IMAGE_RE.match(line):
            continue
        if line.startswith("#### "):
            lines.append(f"<b>{html.escape(line[5:].strip())}</b>")
        elif line.startswith("# "):
            lines.append(f"<b>{html.escape(line[2:].strip())}</b>")
        elif line.startswith("- "):
            lines.append("• " + convert_inline_markdown(line[2:]))
        else:
            lines.append(convert_inline_markdown(line))
    return lines


def convert_inline_markdown(line: str) -> str:
    parts: list[str] = []
    cursor = 0
    for match in MARKDOWN_LINK_RE.finditer(line):
        parts.append(html.escape(line[cursor : match.start()]))
        label = html.escape(match.group(1))
        url = html.escape(match.group(2), quote=True)
        parts.append(f'<a href="{url}">{label}</a>')
        cursor = match.end()
    parts.append(html.escape(line[cursor:]))
    return "".join(parts)


def split_html_messages(text: str, limit: int = 3800) -> list[str]:
    parts: list[str] = []
    current: list[str] = []
    current_len = 0
    for block in text.split("\n\n"):
        block = block.strip()
        if not block:
            continue
        block_len = len(block) + 2
        if current and current_len + block_len > limit:
            parts.append("\n\n".join(current))
            current = [block]
            current_len = block_len
        elif block_len > limit:
            if current:
                parts.append("\n\n".join(current))
                current = []
                current_len = 0
            parts.extend(split_long_block(block, limit))
        else:
            current.append(block)
            current_len += block_len
    if current:
        parts.append("\n\n".join(current))
    return parts


def split_long_block(block: str, limit: int) -> list[str]:
    return [block[index : index + limit] for index in range(0, len(block), limit)]
