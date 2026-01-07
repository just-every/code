#!/usr/bin/env node
// Unified entry point for the Code CLI (fork of OpenAI Codex).

import path from "path";
import { fileURLToPath } from "url";
import { platform as nodePlatform, arch as nodeArch } from "os";
import { execSync } from "child_process";
import { get as httpsGet } from "https";
import { runPostinstall } from "../postinstall.js";

// __dirname equivalent in ESM
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const { platform, arch } = process;

// Important: Never delegate to another system's `code` binary (e.g., VS Code).
// When users run via `npx @just-every/code`, we must always execute our
// packaged native binary by absolute path to avoid PATH collisions.

function isWSL() {
  if (platform !== "linux") return false;
  try {
    const txt = readFileSync("/proc/version", "utf8").toLowerCase();
    return txt.includes("microsoft");
  } catch {
    return false;
  }
}

let targetTriple = null;
switch (platform) {
  case "linux":
  case "android":
    switch (arch) {
      case "x64":
        targetTriple = "x86_64-unknown-linux-musl";
        break;
      case "arm64":
        targetTriple = "aarch64-unknown-linux-musl";
        break;
      default:
        break;
    }
    break;
  case "darwin":
    switch (arch) {
      case "x64":
        targetTriple = "x86_64-apple-darwin";
        break;
      case "arm64":
        targetTriple = "aarch64-apple-darwin";
        break;
      default:
        break;
    }
    break;
  case "win32":
    switch (arch) {
      case "x64":
        targetTriple = "x86_64-pc-windows-msvc.exe";
        break;
      case "arm64":
        // We do not build this today, fall through...
      default:
        break;
    }
    break;
  default:
    break;
}

if (!targetTriple) {
  throw new Error(`Unsupported platform: ${platform} (${arch})`);
}

// Prefer new 'code-*' binary names; fall back to legacy 'coder-*' if missing.
let binaryPath = path.join(__dirname, "..", "bin", `code-${targetTriple}`);
let legacyBinaryPath = path.join(__dirname, "..", "bin", `coder-${targetTriple}`);

// --- Bootstrap helper (runs if the binary is missing, e.g. Bun blocked postinstall) ---
import { existsSync, chmodSync, statSync, openSync, readSync, closeSync, mkdirSync, copyFileSync, readFileSync, unlinkSync, createWriteStream } from "fs";

const validateBinary = (p) => {
  try {
    const st = statSync(p);
    if (!st.isFile() || st.size === 0) {
      return { ok: false, reason: "empty or not a regular file" };
    }
    const fd = openSync(p, "r");
    try {
      const buf = Buffer.alloc(4);
      const n = readSync(fd, buf, 0, 4, 0);
      if (n < 2) return { ok: false, reason: "too short" };
      if (platform === "win32") {
        if (!(buf[0] === 0x4d && buf[1] === 0x5a)) return { ok: false, reason: "invalid PE header (missing MZ)" };
      } else if (platform === "linux" || platform === "android") {
        if (!(buf[0] === 0x7f && buf[1] === 0x45 && buf[2] === 0x4c && buf[3] === 0x46)) return { ok: false, reason: "invalid ELF header" };
      } else if (platform === "darwin") {
        const isMachO = (buf[0] === 0xcf && buf[1] === 0xfa && buf[2] === 0xed && buf[3] === 0xfe) ||
                        (buf[0] === 0xca && buf[1] === 0xfe && buf[2] === 0xba && buf[3] === 0xbe);
        if (!isMachO) return { ok: false, reason: "invalid Mach-O header" };
      }
    } finally {
      closeSync(fd);
    }
    return { ok: true };
  } catch (e) {
    return { ok: false, reason: e.message };
  }
};

const getCacheDir = (version) => {
  const plt = nodePlatform();
  const home = process.env.HOME || process.env.USERPROFILE || "";
  let base = "";
  if (plt === "win32") {
    base = process.env.LOCALAPPDATA || path.join(home, "AppData", "Local");
  } else if (plt === "darwin") {
    base = path.join(home, "Library", "Caches");
  } else {
    base = process.env.XDG_CACHE_HOME || path.join(home, ".cache");
  }
  const dir = path.join(base, "just-every", "code", version);
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
  return dir;
};

const getCachedBinaryPath = (version) => {
  // targetTriple already includes the proper extension on Windows ("...msvc.exe").
  // Do not append another suffix; just use the exact targetTriple-derived name.
  const cacheDir = getCacheDir(version);
  return path.join(cacheDir, `code-${targetTriple}`);
};

let lastBootstrapError = null;

const httpsDownload = (url, dest) => new Promise((resolve, reject) => {
  const req = httpsGet(url, (res) => {
    const status = res.statusCode || 0;
    if (status >= 300 && status < 400 && res.headers.location) {
      // follow one redirect recursively
      return resolve(httpsDownload(res.headers.location, dest));
    }
    if (status !== 200) {
      return reject(new Error(`HTTP ${status}`));
    }
    const out = createWriteStream(dest);
    res.pipe(out);
    out.on("finish", () => out.close(resolve));
    out.on("error", (e) => {
      try { unlinkSync(dest); } catch {}
      reject(e);
    });
  });
  req.on("error", (e) => {
    try { unlinkSync(dest); } catch {}
    reject(e);
  });
  req.setTimeout(120000, () => {
    req.destroy(new Error("download timed out"));
  });
});

const tryBootstrapBinary = async () => {
  try {
    // 1) Read our published version
    const pkg = JSON.parse(readFileSync(path.join(__dirname, "..", "package.json"), "utf8"));
    const version = pkg.version;

    const binDir = path.join(__dirname, "..", "bin");
    if (!existsSync(binDir)) mkdirSync(binDir, { recursive: true });

    // 2) Fast path: user cache
    const cachePath = getCachedBinaryPath(version);
    if (existsSync(cachePath)) {
      const v = validateBinary(cachePath);
      if (v.ok) {
        // Prefer running directly from cache; mirror into node_modules on Unix
        if (platform !== "win32") {
          copyFileSync(cachePath, binaryPath);
          try { chmodSync(binaryPath, 0o755); } catch {}
        }
        return true;
      }
    }

    // 3) Try platform package (if present)
    try {
      const req = (await import("module")).createRequire(import.meta.url);
      const name = (() => {
        if (platform === "win32") return "@just-every/code-win32-x64"; // may be unpublished; falls through
        const plt = nodePlatform();
        const cpu = nodeArch();
        if (plt === "darwin" && cpu === "arm64") return "@just-every/code-darwin-arm64";
        if (plt === "darwin" && cpu === "x64") return "@just-every/code-darwin-x64";
        if (plt === "linux" && cpu === "x64") return "@just-every/code-linux-x64-musl";
        if (plt === "linux" && cpu === "arm64") return "@just-every/code-linux-arm64-musl";
        return null;
      })();
      if (name) {
        try {
          const pkgJson = req.resolve(`${name}/package.json`);
          const pkgDir = path.dirname(pkgJson);
          const src = path.join(pkgDir, "bin", `code-${targetTriple}`);
          if (existsSync(src)) {
            // Always ensure cache has the binary; on Unix mirror into node_modules
            copyFileSync(src, cachePath);
            if (platform !== "win32") {
              copyFileSync(cachePath, binaryPath);
              try { chmodSync(binaryPath, 0o755); } catch {}
            }
            return true;
          }
        } catch { /* ignore and fall back */ }
      }
    } catch { /* ignore */ }

    // 4) Download from GitHub release
    const isWin = platform === "win32";
    const archiveName = isWin
      ? `code-${targetTriple}.zip`
      : (() => { try { execSync("zstd --version", { stdio: "ignore", shell: true }); return `code-${targetTriple}.zst`; } catch { return `code-${targetTriple}.tar.gz`; } })();
    const url = `https://github.com/just-every/code/releases/download/v${version}/${archiveName}`;
    const tmp = path.join(binDir, `.${archiveName}.part`);
    return httpsDownload(url, tmp)
      .then(() => {
        if (isWin) {
          // Extract zip with robust fallbacks and use a safe temp dir, then move to cache
          try {
            const sysRoot = process.env.SystemRoot || process.env.windir || 'C:\\\Windows';
            const psFull = path.join(sysRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
            const unzipDest = getCacheDir(version); // extract directly to cache location
            const psCmd = `Expand-Archive -Path '${tmp}' -DestinationPath '${unzipDest}' -Force`;
            let ok = false;
            try { execSync(`"${psFull}" -NoProfile -NonInteractive -Command "${psCmd}"`, { stdio: 'ignore' }); ok = true; } catch {}
            if (!ok) { try { execSync(`powershell -NoProfile -NonInteractive -Command "${psCmd}"`, { stdio: 'ignore' }); ok = true; } catch {} }
            if (!ok) { try { execSync(`pwsh -NoProfile -NonInteractive -Command "${psCmd}"`, { stdio: 'ignore' }); ok = true; } catch {} }
            if (!ok) { execSync(`tar -xf "${tmp}" -C "${unzipDest}"`, { stdio: 'ignore', shell: true }); }
          } catch (e) {
            throw new Error(`failed to unzip: ${e.message}`);
          } finally { try { unlinkSync(tmp); } catch {} }
        } else {
          if (archiveName.endsWith(".zst")) {
            try { execSync(`zstd -d '${tmp}' -o '${binaryPath}'`, { stdio: 'ignore', shell: true }); }
            catch (e) { try { unlinkSync(tmp); } catch {}; throw new Error(`failed to decompress zst: ${e.message}`); }
            try { unlinkSync(tmp); } catch {}
          } else {
            try { execSync(`tar -xzf '${tmp}' -C '${binDir}'`, { stdio: 'ignore', shell: true }); }
            catch (e) { try { unlinkSync(tmp); } catch {}; throw new Error(`failed to extract tar.gz: ${e.message}`); }
            try { unlinkSync(tmp); } catch {}
          }
        }
        // On Windows, the file was extracted directly into the cache dir
        if (platform === "win32") {
          // Ensure the expected filename exists in cache; Expand-Archive extracts exact name
          // No action required here; validation occurs below against cachePath
        } else {
          try { copyFileSync(binaryPath, cachePath); } catch {}
        }

        const v = validateBinary(platform === "win32" ? cachePath : binaryPath);
        if (!v.ok) throw new Error(`invalid binary (${v.reason})`);
        if (platform !== "win32") try { chmodSync(binaryPath, 0o755); } catch {}
        return true;
      })
      .catch((e) => { lastBootstrapError = e; return false; });
  } catch {
    return false;
  }
};

// If missing, attempt to bootstrap into place (helps when Bun blocks postinstall)
let binaryReady = existsSync(binaryPath) || existsSync(legacyBinaryPath);
if (!binaryReady) {
  let runtimePostinstallError = null;
  try {
    await runPostinstall({ invokedByRuntime: true, skipGlobalAlias: true });
  } catch (err) {
    runtimePostinstallError = err;
  }

  binaryReady = existsSync(binaryPath) || existsSync(legacyBinaryPath);
  if (!binaryReady) {
    const ok = await tryBootstrapBinary();
    if (!ok) {
      if (runtimePostinstallError && !lastBootstrapError) {
        lastBootstrapError = runtimePostinstallError;
      }
      // retry legacy name in case archive provided coder-*
      if (existsSync(legacyBinaryPath) && !existsSync(binaryPath)) {
        binaryPath = legacyBinaryPath;
      }
    }
  }
}

// Prefer cached binary when available
try {
  const pkg = JSON.parse(readFileSync(path.join(__dirname, "..", "package.json"), "utf8"));
  const version = pkg.version;
  const cached = getCachedBinaryPath(version);
  const v = existsSync(cached) ? validateBinary(cached) : { ok: false };
  if (v.ok) {
    binaryPath = cached;
  } else if (!existsSync(binaryPath) && existsSync(legacyBinaryPath)) {
    binaryPath = legacyBinaryPath;
  }
} catch {
  // ignore
}

// Check if binary exists and try to fix permissions if needed
// fs imports are above; keep for readability if tree-shaken by bundlers
import { spawnSync } from "child_process";
if (existsSync(binaryPath)) {
  try {
    // Ensure binary is executable on Unix-like systems
    if (platform !== "win32") {
      chmodSync(binaryPath, 0o755);
    }
  } catch (e) {
    // Ignore permission errors, will be caught below if it's a real problem
  }
} else {
  console.error(`Binary not found: ${binaryPath}`);
  if (lastBootstrapError) {
    const msg = (lastBootstrapError && (lastBootstrapError.message || String(lastBootstrapError))) || 'unknown bootstrap error';
    console.error(`Bootstrap error: ${msg}`);
  }
  console.error(`Please try reinstalling the package:`);
  console.error(`  npm uninstall -g @just-every/code`);
  console.error(`  npm install -g @just-every/code`);
  if (isWSL()) {
    console.error("Detected WSL. Install inside WSL (Ubuntu) separately:");
    console.error("  npx -y @just-every/code@latest  (run inside WSL)");
    console.error("If installed globally on Windows, those binaries are not usable from WSL.");
  }
  process.exit(1);
}

// Lightweight header validation to provide clearer errors before spawn
// Reuse the validateBinary helper defined above in the bootstrap section.

const validation = validateBinary(binaryPath);
if (!validation.ok) {
  console.error(`The native binary at ${binaryPath} appears invalid: ${validation.reason}`);
  console.error("This can happen if the download failed or was modified by antivirus/proxy.");
  console.error("Please try reinstalling:");
  console.error("  npm uninstall -g @just-every/code");
  console.error("  npm install -g @just-every/code");
  if (platform === "win32") {
    console.error("If the issue persists, clear npm cache and disable antivirus temporarily:");
    console.error("  npm cache clean --force");
  }
  if (isWSL()) {
    console.error("Detected WSL. Ensure you install/run inside WSL, not Windows:");
    console.error("  npx -y @just-every/code@latest  (inside WSL)");
  }
  process.exit(1);
}

// If running under npx/npm, emit a concise notice about which binary path is used
try {
  const ua = process.env.npm_config_user_agent || "";
  const isNpx = ua.includes("npx");
  if (isNpx && process.stderr && process.stderr.isTTY) {
    // Best-effort discovery of another 'code' on PATH for user clarity
    let otherCode = "";
    try {
      const cmd = process.platform === "win32" ? "where code" : "command -v code || which code || true";
      const out = spawnSync(process.platform === "win32" ? "cmd" : "bash", [
        process.platform === "win32" ? "/c" : "-lc",
        cmd,
      ], { encoding: "utf8" });
      const line = (out.stdout || "").split(/\r?\n/).map((s) => s.trim()).filter(Boolean)[0];
      if (line && !line.includes("@just-every/code")) {
        otherCode = line;
      }
    } catch {}
    if (otherCode) {
      console.error(`@just-every/code: running bundled binary -> ${binaryPath}`);
      console.error(`Note: a different 'code' exists at ${otherCode}; not delegating.`);
    } else {
      console.error(`@just-every/code: running bundled binary -> ${binaryPath}`);
    }
  }
} catch {}

// Use an asynchronous spawn instead of spawnSync so that Node is able to
// respond to signals (e.g. Ctrl-C / SIGINT) while the native binary is
// executing. This allows us to forward those signals to the child process
// and guarantees that when either the child terminates or the parent
// receives a fatal signal, both processes exit in a predictable manner.
const { spawn } = await import("child_process");

// Make the resolved native binary path visible to spawned agents/subprocesses.
process.env.CODE_BINARY_PATH = binaryPath;

const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  env: { ...process.env, CODER_MANAGED_BY_NPM: "1", CODEX_MANAGED_BY_NPM: "1", CODE_BINARY_PATH: binaryPath },
});

child.on("error", (err) => {
  // Typically triggered when the binary is missing or not executable.
  const code = err && err.code;
  if (code === 'EACCES') {
    console.error(`Permission denied: ${binaryPath}`);
    console.error(`Try running: chmod +x "${binaryPath}"`);
    console.error(`Or reinstall the package with: npm install -g @just-every/code`);
  } else if (code === 'EFTYPE' || code === 'ENOEXEC') {
    console.error(`Failed to execute native binary: ${binaryPath}`);
    console.error("The file may be corrupt or of the wrong type. Reinstall usually fixes this:");
    console.error("  npm uninstall -g @just-every/code && npm install -g @just-every/code");
    if (platform === 'win32') {
      console.error("On Windows, ensure the .exe downloaded correctly (proxy/AV can interfere).");
      console.error("Try clearing cache: npm cache clean --force");
    }
    if (isWSL()) {
      console.error("Detected WSL. Windows binaries cannot be executed from WSL.");
      console.error("Install inside WSL and run there: npx -y @just-every/code@latest");
    }
  } else {
    console.error(err);
  }
  process.exit(1);
});

// Forward common termination signals to the child so that it shuts down
// gracefully. In the handler we temporarily disable the default behavior of
// exiting immediately; once the child has been signaled we simply wait for
// its exit event which will in turn terminate the parent (see below).
const forwardSignal = (signal) => {
  if (child.killed) {
    return;
  }
  try {
    child.kill(signal);
  } catch {
    /* ignore */
  }
};

["SIGINT", "SIGTERM", "SIGHUP"].forEach((sig) => {
  process.on(sig, () => forwardSignal(sig));
});

// When the child exits, mirror its termination reason in the parent so that
// shell scripts and other tooling observe the correct exit status.
// Wrap the lifetime of the child process in a Promise so that we can await
// its termination in a structured way. The Promise resolves with an object
// describing how the child exited: either via exit code or due to a signal.
const childResult = await new Promise((resolve) => {
  child.on("exit", (code, signal) => {
    if (signal) {
      resolve({ type: "signal", signal });
    } else {
      resolve({ type: "code", exitCode: code ?? 1 });
    }
  });
});

if (childResult.type === "signal") {
  // Re-emit the same signal so that the parent terminates with the expected
  // semantics (this also sets the correct exit code of 128 + n).
  process.kill(process.pid, childResult.signal);
} else {
  process.exit(childResult.exitCode);
}
