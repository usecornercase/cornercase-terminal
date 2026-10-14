import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { MAX_BYTES, installCommand, sourceUrl, validate } from '../src/lib/plugins/catalogue';

const fixture = readFileSync('../tests/fixtures/plugins/index-v1.json', 'utf8');
const published = readFileSync('plugins/index.json', 'utf8');

const changed = (field: string, value: unknown) => {
  const index = JSON.parse(fixture);
  if (value === undefined) delete index.plugins[0][field];
  else index.plugins[0][field] = value;
  return JSON.stringify(index);
};

test('the shared fixture and the published index are valid', () => {
  const index = validate(fixture);
  assert.equal(index.plugins.length, 2);
  assert.deepEqual(index.plugins[0].permissions, ['status', 'notify']);
  assert.ok(validate(published));
});

test('every bad index is refused', () => {
  const bad: [string, string][] = [
    ['newer version', fixture.replace('"version": 1', '"version": 2')],
    ['unknown top field', JSON.stringify({ ...JSON.parse(fixture), stars: 1 })],
    ['unknown entry field', changed('homepage', 'https://example.com')],
    ['missing commit', changed('commit', undefined)],
    ['short commit', changed('commit', 'abc123')],
    ['number for a name', changed('name', 3)],
    ['url for a repo', changed('repo', 'https://github.com/example/hello')],
    ['repo with three parts', changed('repo', 'a/b/c')],
    ['option as ref', changed('ref', '--upload-pack=x')],
    ['escape in a name', changed('name', 'Hi\u001b[31m')],
    ['bidi override', changed('description', 'safe ‮txt')],
    ['unknown platform', changed('platforms', ['windows'])],
    ['unknown target', changed('sha256', { riscv64: 'a'.repeat(64) })],
    ['upper-case id', changed('id', 'Hello')],
    ['duplicate id', fixture.replace('example.minimal', 'example.hello')],
    ['oversize', ' '.repeat(MAX_BYTES + 1)],
  ];
  for (const [what, source] of bad) assert.throws(() => validate(source), Error, what);
});

test('the page gives each plugin its pinned install command and source', () => {
  const [hello] = validate(fixture).plugins;
  assert.equal(installCommand(hello), 'cornercase plugin install example/cornercase-hello@0123456789abcdef0123456789abcdef01234567');
  assert.equal(sourceUrl(hello), 'https://github.com/example/cornercase-hello/tree/0123456789abcdef0123456789abcdef01234567');
});
