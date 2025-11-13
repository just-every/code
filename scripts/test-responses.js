#!/usr/bin/env node
/*
Quick Responses API test harness using credentials from $CODEX_HOME/auth.json.

Usage examples:
  # Auto-detect ChatGPT vs API key from auth.json; run all tests non-streaming
  node scripts/test-responses.js --model gpt-4o-mini

  # Force ChatGPT auth and only run the json-schema test (non-streaming)
  node scripts/test-responses.js --auth chatgpt --tests json-schema --model gpt-4o-mini

  # Force API key auth via env; override base URL (e.g., proxy)
  OPENAI_API_KEY=x OPENAI_BASE_URL=http://127.0.0.1:5055/v1 \
    node scripts/test-responses.js --auth api-key --tests base,json-schema --model gpt-4o-mini

Flags:
  --auth chatgpt|api-key|auto   Default: auto (prefer ChatGPT if tokens exist)
  --model <slug>                Default: gpt-4o-mini
  --tests <csv>                 One or more of: base,json-schema,tools-web-search,tools-web-search-preview,store
                                Default: base,json-schema
  --stream true|false           Use streaming SSE (true) or JSON (false). Default: false
  --store true|false            Force store flag; default depends on test case

Notes:
  - ChatGPT auth uses https://chatgpt.com/backend-api/codex/responses
  - API-key auth uses $OPENAI_BASE_URL or https://api.openai.com/v1/responses
  - Sends OpenAI-Beta: responses=v1; Accept: application/json (stream=false) or text/event-stream (stream=true)
  - Prints status, x-request-id, and first 400 chars of error text for failures
*/

const fs = require('fs');
const path = require('path');
const { randomUUID } = require('crypto');

const argv = parseArgs(process.argv.slice(2));

function parseArgs(args) {
  const out = { auth: 'auto', model: 'gpt-5.1', tests: ['base','json-schema'], stream: undefined, store: undefined };
  for (let i=0; i<args.length; i++) {
    const a = args[i];
    const next = () => args[++i];
    if (a === '--auth') out.auth = String(next()||'auto');
    else if (a === '--model') out.model = String(next()||'gpt-4o-mini');
    else if (a === '--tests') out.tests = String(next()||'').split(',').map(s=>s.trim()).filter(Boolean);
    else if (a === '--stream') out.stream = /^true$/i.test(String(next()||''));
    else if (a === '--store') out.store = /^true$/i.test(String(next()||''));
  }
  return out;
}

function codexHome() {
  if (process.env.CODEX_HOME) return process.env.CODEX_HOME;
  if (process.env.CODE_HOME) return process.env.CODE_HOME;

  const home = require('os').homedir();
  const primary = path.join(home, '.code');
  const legacy = path.join(home, '.codex');

  if (fs.existsSync(primary)) return primary;
  if (fs.existsSync(legacy)) return legacy;

  return primary;
}

function readAuthJson() {
  const p = path.join(codexHome(), 'auth.json');
  try { return JSON.parse(fs.readFileSync(p,'utf8')); } catch { return null; }
}

function getChatGptToken(auth) {
  const t = auth && auth.tokens;
  return t && t.access_token ? t.access_token : null;
}

function getAccountId(auth) {
  return auth && auth.tokens && auth.tokens.account_id || null;
}

function getApiKey(auth) {
  // Prefer env var to mirror production behavior
  if (process.env.OPENAI_API_KEY && process.env.OPENAI_API_KEY.trim()) return process.env.OPENAI_API_KEY.trim();
  return auth && auth.OPENAI_API_KEY || null;
}

function pickAuth(mode, auth) {
  if (mode === 'chatgpt') {
    const tok = getChatGptToken(auth);
    if (!tok) throw new Error('No ChatGPT token in auth.json');
    return { kind: 'chatgpt', token: tok, accountId: getAccountId(auth) };
  }
  if (mode === 'api-key') {
    const key = getApiKey(auth);
    if (!key) throw new Error('No OPENAI_API_KEY in env or auth.json');
    return { kind: 'api-key', token: key };
  }
  // auto
  const tok = getChatGptToken(auth);
  if (tok) return { kind: 'chatgpt', token: tok, accountId: getAccountId(auth) };
  const key = getApiKey(auth);
  if (key) return { kind: 'api-key', token: key };
  throw new Error('No usable credentials found');
}

function baseFor(authKind) {
  if (authKind === 'chatgpt') {
    return (process.env.CHATGPT_BASE_URL || 'https://chatgpt.com/backend-api/codex').replace(/\/$/, '');
  }
  return (process.env.OPENAI_BASE_URL || 'https://api.openai.com/v1').replace(/\/$/, '');
}

function readFileSafe(p) { try { return fs.readFileSync(p, 'utf8'); } catch { return null; } }

function loadCodexPrompts() {
  const root = path.resolve(__dirname, '..');
  const base =
    readFileSafe(path.join(root, 'code-rs', 'core', 'prompt.md')) ||
    readFileSafe(path.join(root, 'codex-rs', 'core', 'prompt.md')) ||
    'You are Codex.';
  const coder =
    readFileSafe(path.join(root, 'code-rs', 'core', 'prompt_coder.md')) ||
    readFileSafe(path.join(root, 'codex-rs', 'core', 'prompt_coder.md')) ||
    '';
  return { base, coder };
}

function makePayload(kind, model, stream, store, variant) {
  const { base: BASE_INSTRUCTIONS, coder: ADDITIONAL_INSTRUCTIONS } = loadCodexPrompts();
  const input = [];
  // Developer message with coder instructions
  if (ADDITIONAL_INSTRUCTIONS && ADDITIONAL_INSTRUCTIONS.trim()) {
    input.push({ type: 'message', role: 'developer', content: [{ type: 'input_text', text: ADDITIONAL_INSTRUCTIONS }] });
  }
  // Minimal environment context to mirror Codex shape (kept tiny)
  const envJson = { repo: path.basename(process.cwd()), cwd: process.cwd() };
  input.push({ type: 'message', role: 'user', content: [{ type: 'input_text', text: `<environment_context>\n\n${JSON.stringify(envJson, null, 2)}\n\n</environment_context>` }] });
  // Primary user message
  input.push({ type: 'message', role: 'user', content: [{ type: 'input_text', text: 'Say ok.' }] });
  let tools = undefined;
  if (variant === 'tools-web-search') {
    tools = [ { type: 'web_search' } ];
    if (kind === 'chatgpt') {
      // Match Codex behavior: convert to preview tool for ChatGPT backend
      tools = [ { type: 'web_search' } ];
    }
  } else if (variant === 'tools-web-search-preview') {
    tools = [ { type: 'web_search' } ];
  }
  let text = undefined;
  if (variant === 'json-schema') {
    text = {
      format: {
        type: 'json_schema',
        name: 'ok_schema',
        strict: true,
        schema: { type: 'object', additionalProperties: false, properties: { ok: { type: 'boolean' } }, required: ['ok'] }
      }
    };
  } else if (variant === 'spinner') {
    // Exact spinner JSON schema and developer message per Codex
    const developer = "You are performing a custom task to create a terminal spinner.\n\nRequirements:\n- Output JSON ONLY, no prose.\n- `interval` is the delay in milliseconds between frames; MUST be between 50 and 300 inclusive.\n- `frames` is an array of strings; each element is a frame displayed sequentially at the given interval.\n- The spinner SHOULD have between 2 and 60 frames.\n- Each frame SHOULD be between 1 and 30 characters wide. ALL frames MUST be the SAME width (same number of characters). If you propose frames with varying widths, PAD THEM ON THE LEFT with spaces so they are uniform.\n- You MAY use both ASCII and Unicode characters (e.g., box drawing, braille, arrows). Use EMOJIS ONLY if the user explicitly requests emojis in their prompt.\n- Be creative! You have the full range of Unicode to play with!\n";
    // Insert developer message at the start
    input.unshift({ type: 'message', role: 'developer', content: [{ type: 'input_text', text: developer }] });
    text = {
      format: {
        type: 'json_schema',
        name: 'custom_spinner',
        strict: true,
        schema: {
          type: 'object',
          properties: {
            name: { type: 'string', minLength: 1, maxLength: 40 },
            interval: { type: 'integer', minimum: 50, maximum: 300 },
            frames: { type: 'array', items: { type: 'string', minLength: 1, maxLength: 30 }, minItems: 2, maxItems: 60 }
          },
          required: ['name','interval','frames'],
          additionalProperties: false
        }
      }
    };
  }
  // For ChatGPT, server rejects some combos; keep store=false by default unless explicitly asked
  const useStore = typeof store === 'boolean' ? store : false;
  const payload = {
    model,
    input,
    tools,
    tool_choice: 'auto',
    parallel_tool_calls: true,
    reasoning: null,
    text,
    store: useStore,
    stream,
    include: [],
    prompt_cache_key: randomUUID(),
  };
  // Instructions: Codex always sends BASE_INSTRUCTIONS, including under ChatGPT auth.
  payload.instructions = BASE_INSTRUCTIONS;
  return payload;
}

async function sendOnce(auth, opts) {
  const base = baseFor(auth.kind);
  const url = base + '/responses';
  const headers = {
    'Authorization': `Bearer ${auth.token}`,
    'OpenAI-Beta': 'responses=v1',
  };
  if (opts.stream) headers['Accept'] = 'text/event-stream';
  if (auth.kind === 'chatgpt') headers['session_id'] = randomUUID();
  if (auth.kind === 'chatgpt' && auth.accountId) headers['chatgpt-account-id'] = auth.accountId;
  headers['Content-Type'] = 'application/json';
  headers['originator'] = 'codex_cli_rs';
  headers['version'] = '0.0.0';

  const res = await fetch(url, { method: 'POST', headers, body: JSON.stringify(opts.payload) });
  const rid = res.headers.get('x-request-id');
  const info = { status: res.status, ok: res.ok, requestId: rid };
  if (res.ok) {
    console.log(`[ok] ${opts.caseName}: ${res.status} (request-id: ${rid || '-'})`);
    if (opts.stream) {
      await printSSE(res.body);
      return info;
    } else {
      const data = await res.json();
      printJson(data);
      return info;
    }
  } else {
    let body = '';
    try { body = await res.text(); } catch {}
    const excerpt = body.length > 400 ? body.slice(0,400)+'…' : body;
    console.log(`[fail] ${opts.caseName}: ${res.status} (request-id: ${rid || '-'})\n${excerpt}`);
    return info;
  }
}

function printJson(obj) {
  try {
    console.log(JSON.stringify(obj, null, 2));
  } catch {
    console.log(String(obj));
  }
}

async function printSSE(readable) {
  const reader = readable.getReader();
  const decoder = new TextDecoder('utf-8');
  let buffer = '';
  let finalText = '';
  const flush = (chunk) => {
    buffer += chunk;
    let idx;
    while ((idx = buffer.indexOf('\n\n')) !== -1) {
      const raw = buffer.slice(0, idx);
      buffer = buffer.slice(idx + 2);
      handleEvent(raw);
    }
  };
  function handleEvent(raw) {
    if (!raw) return;
    const lines = raw.split(/\n/);
    let ev = '';
    let dataLines = [];
    for (const ln of lines) {
      if (ln.startsWith('event:')) ev = ln.slice(6).trim();
      if (ln.startsWith('data:')) dataLines.push(ln.slice(5).trim());
    }
    const dataStr = dataLines.join('\n');
    if (!dataStr) return;
    let obj; try { obj = JSON.parse(dataStr); } catch { obj = null; }
    // Print raw events for debugging
    // console.log('> event', ev, dataStr.slice(0,120));
    if (ev === 'response.output_text.delta' && obj && typeof obj.delta === 'string') {
      finalText += obj.delta;
      process.stdout.write(obj.delta);
      return;
    }
    if (ev === 'response.output_item.done' && obj && obj.item && obj.item.type === 'message') {
      const parts = obj.item.content || [];
      for (const p of parts) {
        if (p.type === 'output_text' && typeof p.text === 'string') {
          finalText += p.text;
          process.stdout.write(p.text);
        }
      }
      return;
    }
    if (ev === 'response.completed') {
      // Print a newline to end any inline deltas
      process.stdout.write('\n');
      if (obj && obj.response) printJson(obj.response);
      return;
    }
  }
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    flush(decoder.decode(value, { stream: true }));
  }
  if (buffer.trim()) handleEvent(buffer);
}

async function main() {
  const authJson = readAuthJson();
  const auth = pickAuth(argv.auth, authJson);
  const tests = new Set(argv.tests);
  // Default to streaming for ChatGPT auth unless explicitly set via --stream
  const stream = (typeof argv.stream === 'boolean') ? argv.stream : (auth.kind === 'chatgpt');
  const model = argv.model;

  const cases = [];
  if (tests.has('base')) cases.push({ name: 'base', variant: 'base' });
  if (tests.has('json-schema')) cases.push({ name: 'json-schema', variant: 'json-schema' });
  if (tests.has('tools-web-search')) cases.push({ name: 'tools-web-search', variant: 'tools-web-search' });
  if (tests.has('tools-web-search-preview')) cases.push({ name: 'tools-web-search-preview', variant: 'tools-web-search-preview' });
  if (tests.has('store')) cases.push({ name: 'store=true', variant: 'base', store: true });
  if (tests.has('spinner')) cases.push({ name: 'spinner(json-schema)', variant: 'spinner' });

  for (const c of cases) {
    const payload = makePayload(auth.kind, model, stream, c.store, c.variant);
    await sendOnce(auth, { payload, stream, caseName: c.name });
  }
}

main().catch(e => { console.error(e.stack||String(e)); process.exit(1); });
