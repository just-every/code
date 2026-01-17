#!/usr/bin/env python3
import json
import os
import sys


def load_payload():
    try:
        payload = json.load(sys.stdin)
        if payload:
            return payload
    except Exception:
        pass

    env_payload = os.environ.get("CODE_HOOK_PAYLOAD")
    if env_payload:
        try:
            return json.loads(env_payload)
        except Exception:
            return {}
    return {}

def main():
    payload = load_payload()

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
