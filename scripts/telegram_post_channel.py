#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

from telegram_common import (
    TelegramError,
    api_request,
    build_channel_caption,
    channel_id,
    channel_link,
    chat_username,
    discussion_chat_id,
    save_state,
)


def main() -> int:
    parser = argparse.ArgumentParser(description="Post the daily LeetCode card to Telegram channel.")
    parser.add_argument("--date", required=True)
    parser.add_argument("--title", default="")
    parser.add_argument("--difficulty", default="")
    parser.add_argument("--tldr", default="")
    parser.add_argument("--blog-url", default="")
    parser.add_argument("--substack-url", default="")
    parser.add_argument("--youtube-url", default="")
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--discussion-timeout", type=float, default=15.0)
    args = parser.parse_args()

    try:
        if not args.image.exists():
            raise TelegramError(f"image does not exist: {args.image}")

        caption = build_channel_caption(args)
        message = api_request(
            "sendPhoto",
            {
                "chat_id": channel_id(),
                "caption": caption,
                "show_caption_above_media": True,
                "has_spoiler": True,
            },
            {"photo": args.image},
        )

        channel_message_id = int(message["message_id"])
        link = channel_link(channel_message_id)
        discussion_message_id = find_discussion_message(channel_message_id, args.discussion_timeout)
        state = {
            "channel_chat_id": channel_id(),
            "channel_username": chat_username(channel_id()),
            "channel_message_id": channel_message_id,
            "channel_link": link,
            "discussion_chat_id": discussion_chat_id(),
            "discussion_message_id": discussion_message_id,
        }
        save_state(state)

        print(
            json.dumps(
                {
                    "link": link,
                    "channel_message_id": channel_message_id,
                    "discussion_message_id": discussion_message_id,
                },
                ensure_ascii=False,
            )
        )
        return 0
    except TelegramError as error:
        print(str(error), file=sys.stderr)
        return 1


def find_discussion_message(channel_message_id: int, timeout: float) -> int | None:
    deadline = time.time() + max(timeout, 0.0)
    offset = None
    while time.time() <= deadline:
        fields = {"timeout": 2, "allowed_updates": '["message","channel_post"]'}
        if offset is not None:
            fields["offset"] = offset
        updates = api_request("getUpdates", fields)
        for update in updates:
            offset = int(update["update_id"]) + 1
            message = update.get("message")
            if message and is_discussion_forward(message, channel_message_id):
                return int(message["message_id"])
        time.sleep(0.5)
    return None


def is_discussion_forward(message: dict, channel_message_id: int) -> bool:
    expected_username = chat_username(channel_id())
    if not expected_username:
        return False

    old_forward_chat = message.get("forward_from_chat") or {}
    if old_forward_chat.get("username") == expected_username:
        if message.get("forward_from_message_id") == channel_message_id:
            return True

    origin = message.get("forward_origin") or {}
    if origin.get("type") == "channel":
        chat = origin.get("chat") or {}
        if chat.get("username") == expected_username and origin.get("message_id") == channel_message_id:
            return True

    sender_chat = message.get("sender_chat") or {}
    if message.get("is_automatic_forward") and sender_chat.get("username") == expected_username:
        return message.get("forward_from_message_id") in (None, channel_message_id)

    return False


if __name__ == "__main__":
    raise SystemExit(main())
