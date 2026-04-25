#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from pathlib import Path

from telegram_common import (
    TelegramError,
    api_request,
    channel_id,
    comment_link,
    discussion_chat_id,
    load_state,
    markdown_to_telegram_html,
    save_state,
    split_html_messages,
)
from telegram_post_channel import find_discussion_message


POST_LINK_RE = re.compile(r"/(\d+)(?:\?.*)?$")


def main() -> int:
    parser = argparse.ArgumentParser(description="Post the daily rich text into Telegram comments.")
    parser.add_argument("--post-link", default="")
    parser.add_argument("--channel-message-id", type=int)
    parser.add_argument("--discussion-message-id", type=int)
    parser.add_argument("--body-file", required=True, type=Path)
    parser.add_argument("--discussion-timeout", type=float, default=20.0)
    args = parser.parse_args()

    try:
        state = load_state()
        channel_message_id = (
            args.channel_message_id
            or channel_message_id_from_link(args.post_link)
            or state.get("channel_message_id")
        )
        if not channel_message_id:
            raise TelegramError("missing channel message id; post to channel first")
        channel_message_id = int(channel_message_id)

        discussion_message_id = (
            args.discussion_message_id
            or state.get("discussion_message_id")
            or find_discussion_message(channel_message_id, args.discussion_timeout)
        )
        if not discussion_message_id:
            raise TelegramError(
                "could not find the discussion message for the channel post; "
                "open the channel post in the linked chat once, then retry"
            )
        discussion_message_id = int(discussion_message_id)

        markdown = args.body_file.read_text(encoding="utf-8")
        html = markdown_to_telegram_html(markdown)
        chunks = split_html_messages(html)
        if not chunks:
            raise TelegramError("comment body is empty")

        first_message_id = None
        reply_to = discussion_message_id
        for chunk in chunks:
            message = send_comment_chunk(chunk, reply_to)
            if first_message_id is None:
                first_message_id = int(message["message_id"])
            reply_to = first_message_id
            time.sleep(0.35)

        link = comment_link(channel_message_id, first_message_id, channel_id())
        state.update(
            {
                "channel_chat_id": channel_id(),
                "channel_message_id": channel_message_id,
                "channel_link": args.post_link or state.get("channel_link"),
                "discussion_chat_id": discussion_chat_id(),
                "discussion_message_id": discussion_message_id,
                "comment_message_id": first_message_id,
                "comment_link": link,
            }
        )
        save_state(state)
        print(json.dumps({"link": link, "message_id": first_message_id}, ensure_ascii=False))
        return 0
    except TelegramError as error:
        print(str(error), file=sys.stderr)
        return 1


def channel_message_id_from_link(link: str) -> int | None:
    if not link:
        return None
    match = POST_LINK_RE.search(link.strip())
    if not match:
        return None
    return int(match.group(1))


def send_comment_chunk(text: str, reply_to_message_id: int) -> dict:
    fields = {
        "chat_id": discussion_chat_id(),
        "text": text,
        "parse_mode": "HTML",
        "disable_web_page_preview": True,
        "reply_parameters": {
            "message_id": reply_to_message_id,
            "allow_sending_without_reply": False,
        },
    }
    try:
        return api_request("sendMessage", fields)
    except TelegramError:
        fields.pop("reply_parameters")
        fields["reply_to_message_id"] = reply_to_message_id
        return api_request("sendMessage", fields)


if __name__ == "__main__":
    raise SystemExit(main())
