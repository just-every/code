## FAQ

### OpenAI released a model called Codex in 2021 - is this related?

Only by name. The 2021 Codex model was deprecated in March 2023. Every Code is a community fork of the `openai/codex` CLI and continues to evolve separately.

### Which models are supported?

We recommend the built-in Code presets that wrap GPT-5.2 (for example `code-gpt-5.2-codex`). The default reasoning level is medium, and you can upgrade to high for complex tasks with `/model`.

You can also use older models by using API-based auth and launching Code with the `--model` flag.

### Why does `o3` or `o4-mini` not work for me?

It's possible that your [API account needs to be verified](https://help.openai.com/en/articles/10910291-api-organization-verification) in order to start streaming responses and seeing chain of thought summaries from the API. If you're still running into issues, please let us know!

### How do I stop Code from editing my files?

By default, Code can modify files in your current working directory (Auto mode). To prevent edits, run `code` in read-only mode with the CLI flag `--sandbox read-only`. Alternatively, you can change the approval level mid-conversation with `/approvals`.

### Does it work on Windows?

Running Code directly on Windows may work, but is not officially supported. We recommend using [Windows Subsystem for Linux (WSL2)](https://learn.microsoft.com/en-us/windows/wsl/install).

### Why can't Code find my agents on Windows?

On Windows, agent discovery can be affected by PATH configuration and file extensions. If you see errors like `Agent 'xyz' could not be found`, try these solutions:

**1. Use absolute paths (recommended):**

Edit your `~/.code/config.toml` to use full paths to agent executables:

```toml
[[agents]]
name = "claude"
command = "C:\\Users\\YourUser\\AppData\\Roaming\\npm\\claude.cmd"
enabled = true

[[agents]]
name = "gemini"
command = "C:\\Users\\YourUser\\AppData\\Roaming\\npm\\gemini.cmd"
enabled = true
```

Replace `YourUser` with your actual Windows username.

**2. Find your npm global install location:**

Run this command to find where npm installs global packages:
```cmd
npm config get prefix
```

The executables will be in the returned directory. For example, if it returns `C:\Users\YourUser\AppData\Roaming\npm`, your agent commands will be at:
- `C:\Users\YourUser\AppData\Roaming\npm\claude.cmd`
- `C:\Users\YourUser\AppData\Roaming\npm\gemini.cmd`
- `C:\Users\YourUser\AppData\Roaming\npm\coder.cmd`

**3. Verify PATH includes npm directory:**

In PowerShell:
```powershell
$env:PATH -split ';' | Select-String "npm"
```

In Command Prompt:
```cmd
echo %PATH% | findstr npm
```

If npm's directory isn't in your PATH, you can either:
- Add it to your system PATH (requires restart)
- Use absolute paths in your config (recommended)

**4. Check file extensions:**

On Windows, Code looks for executables with these extensions: `.exe`, `.cmd`, `.bat`, `.com`. Ensure your agent command includes the correct extension when using absolute paths.

**Related:** See the [Agent Configuration Guide](https://github.com/just-every/code/blob/main/code-rs/config.md#agents) for more details.
