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
        payload = None

    env_payload = os.environ.get("CODE_HOOK_PAYLOAD")
    if env_payload:
        try:
            return json.loads(env_payload)
        except Exception:
            return {}
    return payload or {}

def command_from_payload(payload):
    cmd = payload.get("command")
    if cmd is None:
        tool_input = payload.get("tool_input") or {}
        cmd = tool_input.get("command")
    if isinstance(cmd, list):
        return " ".join(cmd)
    if isinstance(cmd, str):
        return cmd
    return ""

def is_risky(command):
    lowered = command.lower()
    risky_terms = [
        "rm -rf",
        "rm -r",
        "mkfs",
        "dd if=",
        ":(){:|:&};:",
        "shutdown -h",
        "reboot",
    ]
    return any(term in lowered for term in risky_terms)

def main():
    payload = load_payload()
    command = command_from_payload(payload)
    if not command:
        print(json.dumps({"hookSpecificOutput": {"permissionDecision": "allow"}}))
        return

    if is_risky(command):
        print(
            json.dumps(
                {
                    "hookSpecificOutput": {"permissionDecision": "ask"},
                    "systemMessage": (
                        "Command guard: risky command requires explicit confirmation.\n\n"
                        f"Command: {command}"
                    ),
                }
            )
        )
        return

    print(json.dumps({"hookSpecificOutput": {"permissionDecision": "allow"}}))

if __name__ == "__main__":
    main()
