import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { checkRecords, inspectRepository, parseIndex, parseObjectMetadata, validatePolicy } from './check-repository-policy.mjs';

const basePolicy = {
  schemaVersion: 1,
  allowedRootFiles: ['README.md'],
  allowedRootDirectories: ['docs', 'python', 'tests', 'rust-engine'],
  forbiddenDirectoryNames: ['models', 'target', '.cache', '.venv', '__pycache__', 'node_modules'],
  forbiddenFileNames: ['engine_test'],
  forbiddenFileSuffixes: ['.onnx', '.safetensors', '.whl', '.so', '.pyc'],
  maxTrackedFileBytes: 100,
  exceptions: [],
};
const record = (path, size = 10, mode = '100644') => ({ path, size, mode });
const policy = changes => ({ ...structuredClone(basePolicy), ...changes });
const violations = (entries, changes = {}) => checkRecords(policy(changes), entries);

test('source, existing-style fixture and Unicode/space paths are permitted', () => {
  assert.deepEqual(violations([record('README.md'), record('rust-engine/build.rs', 100),
    record('tests/differential/fixtures/cards.jsonl'), record('docs/설계 기록.md'),
    record('python/network/model.py', 20, '100755')]), []);
});

test('unregistered directories and root files are rejected, including case variants', () => {
  for (const path of ['experiment-a/network.py', 'scratch.md', 'Docs/design.md']) {
    assert.match(violations([record(path)]).join('\n'), /unregistered root/u);
  }
});

test('generated weights, wheels, binaries, caches and versioned shared libraries are rejected', () => {
  for (const path of ['python/network/candidate.ONNX', 'python/adapter.safetensors', 'python/pkg.whl',
    'python/lib.so.1.2', 'python/network/cache.pyc', 'rust-engine/engine_test',
    'rust-engine/target/release/app', 'python/.venv/config', 'python/__pycache__/module',
    'python/MODELS/notes.md', 'python/node_modules/package', 'docs/.cache/report']) {
    assert.match(violations([record(path)]).join('\n'), /generated rule/u, path);
  }
});

test('size limit is inclusive and explicit size exceptions are exact', () => {
  assert.deepEqual(violations([record('docs/reference.json', 100)]), []);
  assert.match(violations([record('docs/reference.json', 101)]).join('\n'), /size rule/u);
  const changes = { exceptions: [{ path: 'docs/reference.json', reason: 'Bounded comparison fixture', allow: ['size'] }] };
  assert.deepEqual(violations([record('docs/reference.json', 101)], changes), []);
  assert.match(violations([record('docs/reference.json', 101), record('docs/other.json', 101)], changes).join('\n'), /other.json.*size/u);
});

test('configured suffix case also covers versioned shared libraries', () => {
  assert.match(violations([record('python/library.SO.2')], { forbiddenFileSuffixes: ['.SO'] }).join('\n'), /generated rule/u);
});

test('generated fixture exemption does not also waive size or unknown root', () => {
  const exception = { path: 'tests/tiny.onnx', reason: 'Reviewed numerical fixture', allow: ['generated'] };
  assert.deepEqual(violations([record(exception.path)], { exceptions: [exception] }), []);
  assert.match(violations([record(exception.path, 101)], { exceptions: [exception] }).join('\n'), /size rule/u);
  const rootException = { ...exception, path: 'scratch/tiny.onnx' };
  assert.match(violations([record(rootException.path)], { exceptions: [rootException] }).join('\n'), /unregistered root/u);
});

test('portable path collisions include directory case and file/directory overlap', () => {
  for (const paths of [['docs/A.md', 'docs/a.md'], ['docs/Area/a.md', 'docs/area/b.md'],
    ['docs/file', 'docs/file/child'], ['docs/é.md', 'docs/e\u0301.md']]) {
    assert.match(violations(paths.map(path => record(path))).join('\n'), /collision/u);
  }
});

test('symlinks and submodules cannot be exempted', () => {
  for (const mode of ['120000', '160000']) {
    assert.match(violations([record('docs/link', 10, mode)]).join('\n'), /symlink\/submodule/u);
  }
  assert.throws(() => validatePolicy(policy({ exceptions: [{ path: 'docs/link', reason: 'not permitted', allow: ['mode'] }] })), /only generated or size/u);
});

test('traversal, absolute, Windows-style and control-character Git paths fail', () => {
  for (const path of ['../docs/file', '/docs/file', 'C:/docs/file', 'docs\\file', 'docs/./file',
    'docs//file', 'docs/file\nname', 'docs/file.', 'docs/file ', 'docs/file?.md', 'docs/CON.txt', 'docs/com1']) {
    assert.ok(violations([record(path)]).length > 0, JSON.stringify(path));
  }
});

test('malformed, unknown-version and overly broad policies fail closed', () => {
  for (const changes of [{ schemaVersion: 2 }, { maxTrackedFileBytes: 0 }, { maxTrackedFileBytes: Infinity },
    { exceptions: {} }, { extra: true }, { allowedRootDirectories: ['docs', 'Docs'] },
    { forbiddenDirectoryNames: ['../cache'] }, { forbiddenFileSuffixes: ['*'] },
    { exceptions: [{ path: 'docs/*', reason: 'Broad waiver', allow: ['size'] }] },
    { exceptions: [{ path: 'docs/file', reason: ' ', allow: ['size'] }] }]) {
    assert.throws(() => validatePolicy(policy(changes)), undefined, JSON.stringify(changes));
  }
  const missing = policy();
  delete missing.exceptions;
  assert.throws(() => validatePolicy(missing), /missing or unknown/u);
  assert.throws(() => checkRecords(policy(), [record('docs/file', -1)]), /invalid index/u);
});

test('stale, missing and duplicate exact exceptions do not silently accumulate', () => {
  const exception = { path: 'docs/file', reason: 'Oversize fixture', allow: ['size'] };
  assert.match(violations([record(exception.path)], { exceptions: [exception] }).join('\n'), /stale size/u);
  assert.match(violations([], { exceptions: [exception] }).join('\n'), /not in the index/u);
  assert.throws(() => validatePolicy(policy({ exceptions: [exception, exception] })), /duplicate/u);
});

test('NUL index parsing preserves spaces and tabs, and rejects truncation and conflicts', () => {
  const sha = 'a'.repeat(40);
  assert.deepEqual(parseIndex(`100644 ${sha} 0\tdocs/한글 파일\tname.md\0`),
    [{ path: 'docs/한글 파일\tname.md', mode: '100644', oid: sha }]);
  assert.throws(() => parseIndex(`100644 ${sha} 0\tdocs/file`), /truncated/u);
  assert.throws(() => parseIndex(`100644 ${sha} 2\tdocs/file\0`), /unmerged/u);
  assert.throws(() => parseIndex('bad header\tdocs/file\0'), /malformed/u);
});

test('object metadata rejects missing objects, reordered results and unsafe sizes', () => {
  const sha = 'b'.repeat(40);
  assert.equal(parseObjectMetadata(`${sha} blob 123\n`, [sha]).get(sha).size, 123);
  for (const text of [`${sha} missing\n`, `${'c'.repeat(40)} blob 1\n`, `${sha} blob 9007199254740992\n`, '']) {
    assert.throws(() => parseObjectMetadata(text, [sha]));
  }
});

test('real staged policy/index passes from any working directory; CLI errors are nonzero', () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const result = inspectRepository(root);
  assert.ok(result.fileCount > 0);
  assert.deepEqual(result.violations, []);
  const script = fileURLToPath(new URL('./check-repository-policy.mjs', import.meta.url));
  const pass = spawnSync(process.execPath, [script], { cwd: dirname(root), encoding: 'utf8' });
  assert.equal(pass.status, 0, pass.stderr);
  assert.match(pass.stdout, /metadata only; staged policy/u);
  const fail = spawnSync(process.execPath, [script, '--unknown'], { cwd: root, encoding: 'utf8' });
  assert.equal(fail.status, 1);
  assert.match(fail.stderr, /usage/u);
});
