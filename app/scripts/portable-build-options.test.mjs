import { test } from 'node:test';
import assert from 'node:assert/strict';
import { qaBuild } from './portable-build-options.mjs';
test('all supported QA feature spellings select a separate build directory', () => {
  for (const args of [
    ['build', '--features', 'qa-harness'], ['build', '-F', 'qa-webdriver'],
    ['build', '-Fqa-webdriver'], ['build', '-F=qa-harness'],
    ['test', '--features=open-island/qa-harness,other'],
    ['test', '--features', 'other qa-webdriver'], ['build', '--all-features'],
  ]) assert.equal(qaBuild(args), true, args.join(' '));
  for (const args of [['build'], ['test', '--features', 'custom-protocol'], ['build', '--package', 'qa-harness']]) {
    assert.equal(qaBuild(args), false, args.join(' '));
  }
});
