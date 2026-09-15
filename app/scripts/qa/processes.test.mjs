import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { run, assertions } from './processes.mjs';

test('runner rejects failures, empty success, timeout and surviving children', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oi-runner-'));
  try {
    const cases = [
      'process.exit(1)',
      'process.exit(0)',
      'setInterval(() => {}, 1000)',
      "require('child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio: 'ignore'}).unref()",
    ];
    for (const [index, script] of cases.entries()) {
      const result = await run(process.execPath, ['-e', script], { log: join(directory, `${index}.log`), timeout: 200 });
      assert.throws(() => assertions(result));
      if (index === 2) assert.equal(result.timedOut, true);
      if (index === 3) assert.equal(result.orphan, true);
    }
    assert.throws(() => assertions({code: 0, output: 'test result: ok. 1 passed; 0 failed; 1 ignored'}));
    assert.equal(assertions({code: 0, output: 'test result: ok. 2 passed; 0 failed; 0 ignored'}), 2);
    assert.equal(assertions({code: 0, output: ' 258 pass\nRan 258 tests across 51 files.'}), 258);
    assert.equal(assertions({code: 0, output: 'Ran 3 tests in 0.006s\n\nOK'}), 3);
    assert.equal(assertions({code: 0, output: 'ℹ tests 4\nℹ pass 4\nℹ fail 0'}), 4);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
