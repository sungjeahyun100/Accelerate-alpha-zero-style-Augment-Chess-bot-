import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { lstat, readFile, writeFile } from 'node:fs/promises';
import { join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { demoPaths, ensureOwnedDirectory, repositoryRoot } from './paths.mjs';

const INPUTS = ['Cargo.toml', 'Cargo.lock', 'package.json', '.gitignore', 'NOTICE.md',
  '.github/repository-policy.json', '.github/workflows/static-demo.yml',
  'packages/adapter-contract', 'packages/adapter-runtime',
  'projects/augment-chess/engine', 'projects/augment-chess/contracts',
  'projects/augment-chess/browser', 'projects/accelerate/demo'];
const WASM_INPUTS = ['Cargo.toml', 'Cargo.lock', 'packages/adapter-contract', 'packages/adapter-runtime',
  'projects/augment-chess/engine', 'projects/augment-chess/contracts', 'projects/augment-chess/browser',
  'projects/accelerate/demo/scripts/build.mjs', 'projects/accelerate/demo/scripts/paths.mjs',
  'projects/accelerate/demo/scripts/ci-evidence.mjs'];
const SCOPES = new Set(['bindings', 'frontend', 'browser']);
const sha256 = data => createHash('sha256').update(data).digest('hex');
export const gitExecutable = process.env.ACCELERATE_GIT_EXECUTABLE || 'git';
const execute = (command, args) => execFileSync(command, args, { cwd: repositoryRoot, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }).trim();

export async function sourceFingerprint(scope = 'all') {
  if (scope !== 'all' && scope !== 'wasm') throw new Error(`잘못된 빌드 입력 범위: ${scope}`);
  const files = execute(gitExecutable, ['ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', ...(scope === 'wasm' ? WASM_INPUTS : INPUTS)]).split('\0').filter(Boolean).sort();
  const records = [];
  for (const path of files) {
    if (/(^|\/)(\.env(?:\..*)?|credentials?|node_modules|build|dist|coverage|\.cache)(\/|$)/i.test(path)) throw new Error(`CI 입력으로 금지된 경로: ${path}`);
    const stat = await lstat(join(repositoryRoot, path));
    if (!stat.isFile() || stat.isSymbolicLink()) throw new Error(`CI source 입력은 일반 추적 파일이어야 합니다: ${path}`);
    records.push(`${path}\0${sha256(await readFile(join(repositoryRoot, path)))}`);
  }
  const requiredInputs = ['Cargo.lock', 'projects/augment-chess/browser/Cargo.toml', 'projects/augment-chess/contracts/catalog/execution-profile-20260928.json'];
  if (scope === 'all') requiredInputs.push('.github/workflows/static-demo.yml', 'projects/accelerate/demo/package-lock.json');
  for (const required of requiredInputs) {
    if (!files.includes(required)) throw new Error(`CI 전이 입력 누락: ${required}`);
  }
  return sha256(records.join('\n'));
}

export async function validationIdentity(scope) {
  if (!SCOPES.has(scope)) throw new Error(`알 수 없는 CI 검증 범위: ${scope}`);
  const runnerImage = process.env.ImageVersion;
  if (!process.env.RUNNER_TEMP || !runnerImage) throw new Error('성공 근거 재사용은 RUNNER_TEMP·ImageVersion이 있는 CI에서만 지원합니다.');
  const node = process.version;
  if (!/^v22\./.test(node)) throw new Error(`CI Node22 계약 불일치: ${node}`);
  const rust = execute('rustc', ['--version', '--verbose']);
  const commands = {
    bindings: ['npm run test:native', 'npm run wasm', 'npm run wasm-test', 'npm run test:wasm'],
    frontend: ['npm run check', 'npm test'],
    browser: ['npm run build', 'npm run test:browser', 'npm run bundle', 'npm run verify-bundle']
  }[scope];
  const inputs = await sourceFingerprint();
  const environment = { platform: process.platform, arch: process.arch, runnerImage, node, npm: execute(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['--version']), rust, wasmBindgen: '0.2.126', rustFlags: process.env.RUSTFLAGS || '', encodedRustFlags: process.env.CARGO_ENCODED_RUSTFLAGS || '' };
  const fingerprint = sha256(JSON.stringify({ inputs, environment, commands }));
  return { schemaVersion: 1, scope, fingerprint, inputs, environment, commands, key: `accelerate-static-${scope}-verified-v1-${fingerprint}` };
}

async function fileRecord(path, paths) {
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('성공 근거는 link가 아닌 일반 파일이어야 합니다.');
  const bytes = await readFile(path);
  return { path: relative(paths.root, path).replaceAll('\\', '/'), bytes: bytes.length, sha256: sha256(bytes) };
}

async function main() {
  const [command, scope, ...evidenceNames] = process.argv.slice(2);
  const paths = demoPaths();
  ensureOwnedDirectory(paths.reports);
  if (command === 'configure') {
    const variables = {
      ACCELERATE_OUTPUT_ROOT: paths.root, ACCELERATE_DEMO_REPORTS: paths.reports,
      ACCELERATE_DEMO_WASM: paths.wasm, ACCELERATE_DEMO_SITE: paths.site,
      ACCELERATE_DEMO_WASM_TEST: paths.wasmTest,
      ACCELERATE_DEMO_BUNDLE: paths.bundle, CARGO_TARGET_DIR: paths.cargoTarget,
      CARGO_BUILD_JOBS: '2'
    };
    if (!process.env.GITHUB_ENV) throw new Error('CI configure에 GITHUB_ENV가 필요합니다.');
    await writeFile(process.env.GITHUB_ENV, Object.entries(variables).map(([key, value]) => `${key}=${value}`).join('\n') + '\n', { flag: 'a' });
    return;
  }
  const identity = await validationIdentity(scope);
  const marker = join(paths.reports, `${scope}-success.json`);
  if (command === 'key') {
    if (!process.env.GITHUB_OUTPUT) throw new Error('CI key에 GITHUB_OUTPUT이 필요합니다.');
    await writeFile(process.env.GITHUB_OUTPUT, `key=${identity.key}\n`, { flag: 'a' });
  } else if (command === 'record') {
    if (evidenceNames.length === 0) throw new Error('성공 marker에는 실제 검사를 마친 근거 파일이 필요합니다.');
    const evidence = [];
    for (const name of evidenceNames) {
      if (!/^[a-z0-9-]+\.(log|json)$/.test(name)) throw new Error(`잘못된 근거 파일 이름: ${name}`);
      evidence.push(await fileRecord(join(paths.reports, name), paths));
    }
    const artifacts = scope === 'bindings' ? await Promise.all([paths.wasm, paths.wasmTest].flatMap(directory => ['augment_chess_browser.js', 'augment_chess_browser_bg.wasm', 'wasm-inputs.json'].map(name => fileRecord(join(directory, name), paths)))) : [];
    await writeFile(marker, JSON.stringify({ identity, evidence, artifacts }, null, 2) + '\n');
  } else if (command === 'verify') {
    const saved = JSON.parse(await readFile(marker, 'utf8'));
    if (JSON.stringify(saved.identity) !== JSON.stringify(identity) || !Array.isArray(saved.evidence) || saved.evidence.length === 0 || !Array.isArray(saved.artifacts)) throw new Error(`${scope}의 보존된 성공 근거가 현재 입력과 다릅니다.`);
    for (const record of [...saved.evidence, ...saved.artifacts]) {
      if (typeof record.path !== 'string' || record.path.startsWith('/') || record.path.includes('\\') || record.path.split('/').some(part => part === '..' || part === '.')) throw new Error('성공 근거의 상대 경로가 유효하지 않습니다.');
      if (JSON.stringify(await fileRecord(join(paths.root, record.path), paths)) !== JSON.stringify(record)) throw new Error(`성공 근거 파일 SHA256 불일치: ${record.path}`);
    }
    if (scope === 'bindings' && saved.artifacts.length !== 6) throw new Error('배포용·시험용 WASM 성공 근거의 compiled artifact·입력 stamp가 누락됐습니다.');
    console.log(JSON.stringify({ stage: 'reused-success', scope, fingerprint: identity.fingerprint }));
  } else throw new Error('사용법: ci-evidence.mjs configure|key|record|verify [bindings|frontend|browser] [검사.log|자료.json...]');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch(error => { console.error(error); process.exitCode = 1; });
