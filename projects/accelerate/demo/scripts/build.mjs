import { copyFile, lstat, mkdir, open, readFile, rm, writeFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { demoPaths, demoRoot, ensureOwnedDirectory, repositoryRoot } from './paths.mjs';

const paths = demoPaths();
const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const wasmFiles = ['augment_chess_browser.js', 'augment_chess_browser_bg.wasm'];

async function wasmFileRecords(directory = paths.wasm) {
  return Promise.all(wasmFiles.map(async path => {
    const stat = await lstat(join(directory, path));
    if (!stat.isFile() || stat.isSymbolicLink()) throw new Error(`WASM compiled artifact는 일반 파일이어야 합니다: ${path}`);
    const data = await readFile(join(directory, path));
    return { path, bytes: data.length, sha256: createHash('sha256').update(data).digest('hex') };
  }));
}

function run(command, args, cwd = repositoryRoot, environment = process.env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env: environment, stdio: 'inherit', shell: process.platform === 'win32' && command.endsWith('.cmd') });
    child.once('error', reject);
    child.once('exit', (code, signal) => code === 0 ? resolve() : reject(new Error(`${command} ${args.join(' ')}: exit=${code}, signal=${signal}`)));
  });
}

async function cliVersion() {
  const manifest = await readFile(join(repositoryRoot, 'projects/augment-chess/browser/Cargo.toml'), 'utf8');
  const match = manifest.match(/^wasm-bindgen\s*=\s*"=(\d+\.\d+\.\d+)"/m);
  if (!match) throw new Error('browser/Cargo.toml에 wasm-bindgen 정확한 버전 pin이 필요합니다.');
  return match[1];
}

async function prepareAssets() {
  const { sourceFingerprint } = await import('./ci-evidence.mjs');
  const stamp = JSON.parse(await readFile(join(paths.wasm, 'wasm-inputs.json'), 'utf8'));
  if (stamp.schemaVersion !== 1 || JSON.stringify(stamp.features) !== '[]' || stamp.inputFingerprint !== await sourceFingerprint('wasm') || JSON.stringify(stamp.files) !== JSON.stringify(await wasmFileRecords())) throw new Error('WASM 소스 입력·배포 feature 또는 compiled artifact가 바뀌었습니다. npm run wasm으로 다시 빌드하세요.');
  ensureOwnedDirectory(paths.public);
  // This verified, checkout-owned slot contains only regenerated public assets.
  // Remove prior inputs so an old manifest or test binding cannot enter the site.
  await rm(paths.public, { recursive: true });
  ensureOwnedDirectory(paths.public);
  ensureOwnedDirectory(join(paths.public, 'wasm'));
  for (const name of wasmFiles) {
    await copyFile(join(paths.wasm, name), join(paths.public, 'wasm', name));
  }
  const contracts = join(repositoryRoot, 'projects/augment-chess/contracts/catalog');
  await copyFile(join(contracts, 'site-20260928.json'), join(paths.public, 'catalog.json'));
  await copyFile(join(contracts, 'execution-profile-20260928.json'), join(paths.public, 'execution-profile.json'));
  await copyFile(join(contracts, 'card-presentation-20260928.json'), join(paths.public, 'card-presentation.json'));
  await copyFile(join(contracts, 'card-definitions-20260928.json'), join(paths.public, 'card-definitions.json'));
}

async function main() {
  const command = process.argv[2];
  if (command === 'native') {
    ensureOwnedDirectory(paths.cargoTarget);
    ensureOwnedDirectory(paths.reports);
    const environment = { ...process.env, CARGO_TARGET_DIR: paths.cargoTarget, CARGO_BUILD_JOBS: '2' };
    await run('cargo', ['test', '-p', 'augment-chess-browser', '--features', 'browser-test-fixtures', '--locked'], repositoryRoot, environment);
    const fixture = await open(join(paths.reports, 'native-fixtures.json'), 'w');
    try {
      await new Promise((resolve, reject) => {
        const child = spawn('cargo', ['run', '-p', 'augment-chess-browser', '--features', 'browser-test-fixtures', '--bin', 'browser-fixtures', '--locked'], { cwd: repositoryRoot, env: environment, stdio: ['ignore', fixture.fd, 'inherit'] });
        child.once('error', reject);
        child.once('exit', (code, signal) => code === 0 ? resolve() : reject(new Error(`native-fixtures: exit=${code}, signal=${signal}`)));
      });
    } finally { await fixture.close(); }
  } else if (command === 'wasm' || command === 'wasm-test') {
    const features = command === 'wasm-test' ? ['browser-test-fixtures'] : [];
    const output = command === 'wasm-test' ? paths.wasmTest : paths.wasm;
    const { sourceFingerprint } = await import('./ci-evidence.mjs');
    const before = await sourceFingerprint('wasm');
    const expected = `wasm-bindgen ${await cliVersion()}`;
    const { execFile } = await import('node:child_process');
    const version = await new Promise((resolve, reject) => execFile('wasm-bindgen', ['--version'], (error, stdout, stderr) => {
      if (stderr) process.stderr.write(stderr);
      error ? reject(error) : resolve(stdout.trim());
    }));
    if (version !== expected) throw new Error(`wasm-bindgen CLI 버전 불일치: expected=${expected}, actual=${version}`);
    ensureOwnedDirectory(output);
    ensureOwnedDirectory(paths.cargoTarget);
    await mkdir(output, { recursive: true });
    const environment = { ...process.env, CARGO_TARGET_DIR: paths.cargoTarget, CARGO_BUILD_JOBS: '2' };
    await run('cargo', ['build', '-p', 'augment-chess-browser', '--target', 'wasm32-unknown-unknown', '--release', '--locked', '--no-default-features', ...(features.length ? ['--features', features.join(',')] : [])], repositoryRoot, environment);
    await run('wasm-bindgen', [join(paths.cargoTarget, 'wasm32-unknown-unknown/release/augment_chess_browser.wasm'), '--target', 'web', '--out-name', 'augment_chess_browser', '--out-dir', output]);
    const after = await sourceFingerprint('wasm');
    if (before !== after) throw new Error('WASM 빌드 중 엔진·공통 계약·바인딩 입력이 변경됐습니다.');
    await writeFile(join(output, 'wasm-inputs.json'), JSON.stringify({ schemaVersion: 1, inputFingerprint: after, features, wasmBindgen: expected, files: await wasmFileRecords(output) }, null, 2) + '\n');
  } else if (command === 'build' || command === 'dev') {
    ensureOwnedDirectory(paths.site);
    ensureOwnedDirectory(paths.viteCache);
    await prepareAssets();
    const { sourceFingerprint } = await import('./ci-evidence.mjs');
    const before = command === 'build' ? await sourceFingerprint() : null;
    await run(npm, ['exec', '--workspaces=false', '--', 'vite', ...(command === 'build' ? ['build'] : [])], demoRoot);
    if (command === 'build') {
      const after = await sourceFingerprint();
      if (before !== after) throw new Error('정적 빌드 중 소스 입력이 변경됐습니다.');
      const { filesAt } = await import('./delivery.mjs');
      ensureOwnedDirectory(paths.reports);
      await writeFile(join(paths.reports, 'build-inputs.json'), JSON.stringify({ schemaVersion: 1, fingerprint: after, files: await filesAt(paths.site) }, null, 2) + '\n');
    }
  } else if (command === 'preview') {
    await run(npm, ['exec', '--workspaces=false', '--', 'vite', 'preview'], demoRoot);
  } else throw new Error('사용법: node scripts/build.mjs native|wasm|wasm-test|build|dev|preview');
}

main().catch(error => { console.error(error); process.exitCode = 1; });
