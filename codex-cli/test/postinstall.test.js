import { test } from 'node:test';
import assert from 'node:assert/strict';

test('runPostinstall resolves in dry-run mode', async () => {
  const { runPostinstall } = await import('../postinstall.js');
  process.env.CODE_POSTINSTALL_DRY_RUN = '1';
  try {
    const result = await runPostinstall({ invokedByRuntime: true, skipGlobalAlias: true });
    assert.ok(result && result.skipped === true);
  } finally {
    delete process.env.CODE_POSTINSTALL_DRY_RUN;
    delete process.env.CODE_RUNTIME_POSTINSTALL;
  }
});
