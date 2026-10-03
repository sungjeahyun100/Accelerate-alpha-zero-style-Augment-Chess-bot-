import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

const schema = JSON.parse(await readFile(new URL('./schemas/adapter-v1.schema.json', import.meta.url), 'utf8'));

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

const digest = createHash('sha256').update(canonicalJson(schema), 'utf8').digest('hex');
if (process.argv.includes('--print-hash')) {
  process.stdout.write(`${digest}\n`);
} else {
  const manifest = JSON.parse(await readFile(new URL('./manifest.json', import.meta.url), 'utf8'));
  assert.equal(schema.$schema, 'https://json-schema.org/draft/2020-12/schema');
  assert.equal(schema.$id, manifest.schemaId);
  assert.equal(digest, manifest.sha256, 'adapter v1 schema hash differs from manifest');
  for (const name of ['AdapterDescriptor', 'AdapterRequest', 'AdapterResponse', 'AdapterError', 'AdapterOutcome']) {
    assert.ok(schema.$defs[name], `missing shared wire object ${name}`);
  }
  process.stdout.write(`adapter contract ${manifest.schemaId} verified (${digest})\n`);
}
