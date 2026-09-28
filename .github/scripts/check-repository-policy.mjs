import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const POLICY_PATH = '.github/repository-policy.json';
const REPOSITORY_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const REGULAR_MODES = new Set(['100644', '100755']);

function requireCondition(condition, message) {
  if (!condition) throw new Error(message);
}

function exactFields(value, fields, label) {
  requireCondition(value && typeof value === 'object' && !Array.isArray(value), `${label}: expected object`);
  const actual = Object.keys(value);
  requireCondition(actual.length === fields.length && actual.every(key => fields.includes(key)),
    `${label}: missing or unknown fields`);
}

function uniqueStrings(value, label) {
  requireCondition(Array.isArray(value) && value.every(item => typeof item === 'string' && item.trim()),
    `${label}: expected nonempty strings`);
  requireCondition(new Set(value.map(item => item.normalize('NFC').toLowerCase())).size === value.length,
    `${label}: duplicate names`);
}

function pathParts(path) {
  requireCondition(typeof path === 'string' && path.length > 0, 'invalid repository path');
  requireCondition(!/[\\:<>"|?*\x00-\x1f\x7f]/u.test(path), 'path must use portable Git separators and characters');
  const parts = path.split('/');
  requireCondition(parts.every(part => part && part !== '.' && part !== '..' && !/[. ]$/u.test(part)),
    'path must be relative, canonical and portable');
  requireCondition(parts.every(part => !/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/iu.test(part)),
    'path uses a Windows reserved device name');
  return parts;
}

export function validatePolicy(policy) {
  exactFields(policy, ['schemaVersion', 'allowedRootFiles', 'allowedRootDirectories',
    'forbiddenDirectoryNames', 'forbiddenFileNames', 'forbiddenFileSuffixes',
    'maxTrackedFileBytes', 'exceptions'], 'policy');
  requireCondition(policy.schemaVersion === 1, 'unsupported policy schemaVersion');
  for (const key of ['allowedRootFiles', 'allowedRootDirectories', 'forbiddenDirectoryNames', 'forbiddenFileNames']) {
    uniqueStrings(policy[key], key);
    requireCondition(policy[key].every(name => pathParts(name).length === 1 && !/[*?\[\]]/u.test(name)),
      `${key}: expected exact single-component names`);
  }
  uniqueStrings(policy.forbiddenFileSuffixes, 'forbiddenFileSuffixes');
  requireCondition(policy.forbiddenFileSuffixes.every(suffix =>
    suffix.startsWith('.') && suffix.length > 1 && !/[\\/:*?\[\]\s]/u.test(suffix)), 'invalid file suffix');
  const roots = [...policy.allowedRootFiles, ...policy.allowedRootDirectories];
  uniqueStrings(roots, 'root paths');
  requireCondition(policy.allowedRootFiles.length && policy.allowedRootDirectories.length,
    'root allowlists must not be empty');
  requireCondition(Number.isSafeInteger(policy.maxTrackedFileBytes) && policy.maxTrackedFileBytes > 0,
    'maxTrackedFileBytes must be a positive safe integer');
  requireCondition(Array.isArray(policy.exceptions), 'exceptions must be an array');
  const exceptionPaths = [];
  for (const exception of policy.exceptions) {
    exactFields(exception, ['path', 'reason', 'allow'], 'exception');
    pathParts(exception.path);
    requireCondition(!/[*?\[\]]/u.test(exception.path), 'exceptions must use exact paths, not patterns');
    requireCondition(typeof exception.reason === 'string' && exception.reason.trim(), 'exception needs a reason');
    uniqueStrings(exception.allow, 'exception.allow');
    requireCondition(exception.allow.length && exception.allow.every(rule => ['generated', 'size'].includes(rule)),
      'exception can waive only generated or size rules');
    exceptionPaths.push(exception.path);
  }
  uniqueStrings(exceptionPaths, 'exception paths');
  return policy;
}

function generatedPath(parts, policy) {
  const lower = parts.map(part => part.toLowerCase());
  const name = lower.at(-1);
  return lower.slice(0, -1).some(part => policy.forbiddenDirectoryNames.some(item => item.toLowerCase() === part))
    || policy.forbiddenFileNames.some(item => item.toLowerCase() === name)
    || policy.forbiddenFileSuffixes.some(suffix => name.endsWith(suffix.toLowerCase()))
    || (policy.forbiddenFileSuffixes.some(suffix => suffix.toLowerCase() === '.so') && /\.so(?:\.\d+)+$/u.test(name));
}

export function checkRecords(policy, records) {
  validatePolicy(policy);
  requireCondition(Array.isArray(records), 'expected index records');
  const violations = [];
  const seenPaths = new Set();
  const portableNodes = new Map();
  const exceptions = new Map(policy.exceptions.map(item => [item.path, item]));
  for (const record of records) {
    requireCondition(record && typeof record.path === 'string' && Number.isSafeInteger(record.size)
      && record.size >= 0 && typeof record.mode === 'string', 'invalid index metadata');
    const label = JSON.stringify(record.path);
    let parts;
    try { parts = pathParts(record.path); } catch (error) {
      violations.push(`${label}: ${error.message}`);
      continue;
    }
    if (seenPaths.has(record.path)) violations.push(`${label}: duplicate index entry`);
    seenPaths.add(record.path);
    for (let end = 1; end <= parts.length; end++) {
      const node = parts.slice(0, end).join('/');
      const key = node.normalize('NFC').toLowerCase();
      const kind = end === parts.length ? 'file' : 'directory';
      const previous = portableNodes.get(key);
      if (previous && (previous.path !== node || previous.kind !== kind)) {
        violations.push(`${label}: case/Unicode or file/directory collision at ${JSON.stringify(node)}`);
      } else {
        portableNodes.set(key, { path: node, kind });
      }
    }
    const rootAllowed = parts.length === 1
      ? policy.allowedRootFiles.includes(parts[0]) : policy.allowedRootDirectories.includes(parts[0]);
    if (!rootAllowed) violations.push(`${label}: unregistered root path`);
    if (!REGULAR_MODES.has(record.mode)) violations.push(`${label}: symlink/submodule or unsupported Git mode`);
    const failures = new Set();
    if (generatedPath(parts, policy)) failures.add('generated');
    if (record.size > policy.maxTrackedFileBytes) failures.add('size');
    const exception = exceptions.get(record.path);
    for (const rule of failures) {
      if (!exception?.allow.includes(rule)) violations.push(`${label}: ${rule} rule violated`);
    }
    for (const rule of exception?.allow ?? []) {
      if (!failures.has(rule)) violations.push(`${label}: stale ${rule} exception`);
    }
  }
  for (const path of exceptions.keys()) {
    if (!seenPaths.has(path)) violations.push(`${JSON.stringify(path)}: exception path is not in the index`);
  }
  return violations;
}

export function parseIndex(text) {
  requireCondition(typeof text === 'string' && (!text || text.endsWith('\0')), 'truncated Git index metadata');
  return text.split('\0').filter(Boolean).map(entry => {
    const tab = entry.indexOf('\t');
    requireCondition(tab > 0, 'malformed Git index entry');
    const header = /^(\d{6}) ([0-9a-f]{40}|[0-9a-f]{64}) ([0-3])$/u.exec(entry.slice(0, tab));
    requireCondition(header, 'malformed Git index header');
    requireCondition(header[3] === '0', 'unmerged index: resolve conflicts before checking');
    return { mode: header[1], oid: header[2], path: entry.slice(tab + 1) };
  });
}

export function parseObjectMetadata(text, objectIds) {
  const lines = text.trimEnd().split('\n');
  requireCondition(lines.length === objectIds.length, 'incomplete Git object metadata');
  const metadata = new Map();
  lines.forEach((line, index) => {
    const match = /^([0-9a-f]{40}|[0-9a-f]{64}) (blob|commit) (\d+)\r?$/u.exec(line);
    requireCondition(match && match[1] === objectIds[index], 'missing or unexpected Git object metadata');
    const size = Number(match[3]);
    requireCondition(Number.isSafeInteger(size) && size >= 0, 'invalid Git object size');
    metadata.set(match[1], { type: match[2], size });
  });
  return metadata;
}

function git(args, cwd, input) {
  const result = spawnSync('git', args, { cwd, input, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  requireCondition(!result.error && result.status === 0,
    `Git metadata query failed: ${result.error?.message ?? result.stderr.trim()}`);
  return result.stdout;
}

export function readIndex(cwd) {
  const records = parseIndex(git(['ls-files', '--stage', '-z'], cwd));
  requireCondition(records.length > 0, 'empty Git index');
  const ids = [...new Set(records.map(record => record.oid))];
  const metadata = parseObjectMetadata(git(['cat-file', '--batch-check=%(objectname) %(objecttype) %(objectsize)'],
    cwd, `${ids.join('\n')}\n`), ids);
  return records.map(record => {
    const object = metadata.get(record.oid);
    requireCondition(object.type === (record.mode === '160000' ? 'commit' : 'blob'), 'unexpected indexed object type');
    return { ...record, size: object.size };
  });
}

export function inspectRepository(cwd) {
  const records = readIndex(cwd);
  const policyRecord = records.find(record => record.path === POLICY_PATH);
  requireCondition(policyRecord && REGULAR_MODES.has(policyRecord.mode), 'stage the regular repository policy file first');
  requireCondition(policyRecord.size <= 64 * 1024, 'repository policy exceeds 64 KiB');
  // Read only this identified policy blob. Never open tracked files, links or generated directories.
  const policy = JSON.parse(git(['cat-file', 'blob', policyRecord.oid], cwd));
  return { fileCount: records.length, violations: checkRecords(policy, records) };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    requireCondition(process.argv.length === 2, 'usage: node .github/scripts/check-repository-policy.mjs');
    const result = inspectRepository(REPOSITORY_ROOT);
    if (result.violations.length) {
      for (const violation of result.violations) console.error(violation);
      process.exitCode = 1;
    } else {
      console.log(`Repository policy passed: ${result.fileCount} indexed files (metadata only; staged policy).`);
    }
  } catch (error) {
    console.error(`Repository policy failed: ${error.message}`);
    process.exitCode = 1;
  }
}
