#!/usr/bin/env python3
"""Render Spec Kit prompts with variable substitution for consensus runner."""

import argparse
import json
import pathlib
import sys
import re


def load_prompts(path: pathlib.Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        sys.exit(f"ERROR: prompts file not found: {path}")
    except json.JSONDecodeError as exc:
        sys.exit(f"ERROR: failed to parse prompts JSON: {exc}")


def stage_data(prompts: dict, stage: str) -> dict:
    if stage not in prompts:
        sys.exit(f"ERROR: unknown stage '{stage}' in prompts file")
    return prompts[stage]


def agent_prompt(stage_payload: dict, agent: str) -> str:
    if agent not in stage_payload:
        sys.exit(f"ERROR: stage does not define agent '{agent}'")
    data = stage_payload[agent]
    prompt = data.get("prompt")
    if prompt is None:
        sys.exit(f"ERROR: agent '{agent}' prompt missing")
    return prompt


def replace_placeholders(template: str, values: dict) -> str:
    # Replace ${VAR} occurrences. Missing variables become empty strings.
    pattern = re.compile(r"\$\{([^}]+)\}")

    def repl(match: re.Match) -> str:
        key = match.group(1)
        return values.get(key, "")

    return pattern.sub(repl, template)


def main() -> None:
    parser = argparse.ArgumentParser(description="Render Spec Kit prompt")
    subparsers = parser.add_subparsers(dest="command", required=True)

    agents_parser = subparsers.add_parser("agents", help="List agents for a stage")
    agents_parser.add_argument("stage")
    agents_parser.add_argument("prompts_file", type=pathlib.Path)

    render_parser = subparsers.add_parser("render", help="Render a prompt")
    render_parser.add_argument("stage")
    render_parser.add_argument("agent")
    render_parser.add_argument("prompts_file", type=pathlib.Path)
    render_parser.add_argument("spec_id")
    render_parser.add_argument("prompt_version")
    render_parser.add_argument("model_id")
    render_parser.add_argument("model_release")
    render_parser.add_argument("reasoning_mode")
    render_parser.add_argument("context", help="Context string", nargs="?")
    render_parser.add_argument("previous_outputs", help="JSON string of previous outputs", nargs="?")
    render_parser.add_argument("previous_outputs_agent", help="JSON string for agent-specific previous output", nargs="?")

    args = parser.parse_args()

    if args.command == "agents":
        prompts = load_prompts(args.prompts_file)
        payload = stage_data(prompts, args.stage)
        agents = [key for key in payload.keys() if key != "version"]
        print(" ".join(agents))
        return

    prompts = load_prompts(args.prompts_file)
    payload = stage_data(prompts, args.stage)
    template = agent_prompt(payload, args.agent)

    context = args.context or ""

    previous_outputs = {}
    if args.previous_outputs:
        try:
            previous_outputs = json.loads(args.previous_outputs)
        except json.JSONDecodeError:
            sys.exit("ERROR: invalid JSON passed for PREVIOUS_OUTPUTS")

    agent_previous = {}
    if args.previous_outputs_agent:
        try:
            agent_previous = json.loads(args.previous_outputs_agent)
        except json.JSONDecodeError:
            sys.exit("ERROR: invalid JSON passed for PREVIOUS_OUTPUTS.<agent>")

    substitutions = {
        "SPEC_ID": args.spec_id,
        "PROMPT_VERSION": args.prompt_version,
        "MODEL_ID": args.model_id,
        "MODEL_RELEASE": args.model_release,
        "REASONING_MODE": args.reasoning_mode,
        "CONTEXT": context,
        "PREVIOUS_OUTPUTS": json.dumps(previous_outputs, ensure_ascii=False, indent=2),
    }

    for agent_name, payload_json in previous_outputs.items():
        substitutions[f"PREVIOUS_OUTPUTS.{agent_name}"] = json.dumps(
            payload_json, ensure_ascii=False, indent=2
        )

    if agent_previous:
        substitutions[f"PREVIOUS_OUTPUTS.{args.agent}"] = json.dumps(
            agent_previous, ensure_ascii=False, indent=2
        )

    rendered = replace_placeholders(template, substitutions)
    print(rendered)


if __name__ == "__main__":
    main()
