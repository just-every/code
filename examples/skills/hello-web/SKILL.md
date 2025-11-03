---
name: hello-web
description: Open example.com in the browser and capture a screenshot.
allowed-tools:
  - browser
metadata:
  owner: examples
---

# Hello Web Skill

## Overview

Use this skill when you need to quickly browse to a URL, take a screenshot, and summarise the
page.

## Workflow

1. Call the `browser` action with `{"action":"open","url":"https://example.com"}`.
2. Once the page loads, capture a screenshot: `{"action":"screenshot"}`.
3. Optionally use `{"action":"type","text":"..."}` and `{"action":"click",...}` for
   basic navigation.
4. Close the browser with `{"action":"close"}` when finished.

## Notes

- This manifest demonstrates the required YAML frontmatter plus instructional Markdown.
- Copy the entire `hello-web` directory into `~/.claude/skills/` or your project’s
  `.claude/skills/` directory so Code can discover it.
