#!/usr/bin/env python3
import json
import sys

def load_payload():
    try:
        return json.load(sys.stdin)
    except Exception:
        return {}

def command_from_payload(payload):
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
