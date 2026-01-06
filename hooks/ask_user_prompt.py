#!/usr/bin/env python3
import json
import sys

def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        payload = {}

    prompt = (payload.get("user_prompt") or "").strip()
    lowered = prompt.lower()
    trigger = lowered.startswith("hook:")

    if not trigger:
        print(json.dumps({"hookSpecificOutput": {"permissionDecision": "allow"}}))
        return

    shown = prompt
    max_len = 280
    if len(shown) > max_len:
        shown = shown[:max_len].rstrip() + "..."

    message = "Hook gate: approve this prompt to continue."
    if shown:
        message = f"Hook gate: approve this prompt to continue.\n\nUser prompt: {shown}"

    print(
        json.dumps(
            {
                "hookSpecificOutput": {"permissionDecision": "ask"},
                "systemMessage": message,
            }
        )
    )

if __name__ == "__main__":
    main()
