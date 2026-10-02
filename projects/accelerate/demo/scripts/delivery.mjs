import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFile, lstat, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { demoPaths, ensureOwnedDirectory, repositoryRoot } from './paths.mjs';
import {gitExecutable} from './ci-evidence.mjs';
import { sourceFingerprint } from './ci-evidence.mjs';

const MAX_FILES = 512;
const MAX_FILE_BYTES = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES = 128 * 1024 * 1024;
const HF_OWNER = 'daejunnom';
const sha256 = data => createHash('sha256').update(data).digest('hex');

export function validateAssetName(name) {
  if (!name || !/^[A-Za-z0-9._/-]+$/.test(name) || name.startsWith('/') || !name.split('/').every(part => part && part !== '.' && part !== '..')) throw new Error(`잘못된 배포 상대 경로: ${name}`);
  if (/(^|\/)(\.env(?:\..*)?|node_modules|\.git|credentials?)(\/|$)/i.test(name)) throw new Error(`배포할 수 없는 경로: ${name}`);
  if (!/\.(?:html|js|css|wasm|json|svg|png|ico|webp|txt|md)$/.test(name)) throw new Error(`허용되지 않은 배포 파일 형식: ${name}`);
}

export async function filesAt(directory, prefix = '', result = []) {
  if (prefix.split('/').length > 12) throw new Error('배포 경로 깊이 한도 초과');
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const name = prefix + entry.name;
    const path = join(directory, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`배포 파일 link 거부: ${name}`);
    if (entry.isDirectory()) await filesAt(path, `${name}/`, result);
    else if (entry.isFile()) {
      validateAssetName(name);
      const stat = await lstat(path);
      if (stat.size > MAX_FILE_BYTES) throw new Error(`배포 파일 크기 한도 초과: ${name}`);
      result.push({ path: name, bytes: stat.size, sha256: sha256(await readFile(path)) });
      if (result.length > MAX_FILES || result.reduce((total, item) => total + item.bytes, 0) > MAX_TOTAL_BYTES) throw new Error('배포 묶음 파일 수 또는 총 크기 한도 초과');
    } else throw new Error(`일반 파일이 아닌 배포 입력: ${name}`);
  }
  return result.sort((left, right) => left.path.localeCompare(right.path, 'en'));
}

export async function verifyBundle(directory) {
  const manifest = JSON.parse(await readFile(join(directory, 'source-manifest.json'), 'utf8'));
  if (manifest.schemaVersion !== 1 || !/^[0-9a-f]{40}$/.test(manifest.sourceCommit) || !Array.isArray(manifest.files)) throw new Error('source-manifest의 버전·소스 commit·files가 유효하지 않습니다.');
  if (manifest.deploymentPlan?.provider !== 'hugging-face' || manifest.deploymentPlan?.sdk !== 'static'
      || manifest.deploymentPlan?.owner !== HF_OWNER || manifest.deploymentPlan?.spaceName !== null) {
    throw new Error('계획된 HF 배포 대상은 daejunnom 소유의 이름 미정 Static Space여야 합니다.');
  }
  const actual = (await filesAt(directory)).filter(file => file.path !== 'source-manifest.json');
  const recorded = manifest.files;
  for (const file of recorded) {
    validateAssetName(file.path);
    if (!Number.isSafeInteger(file.bytes) || file.bytes < 0 || !/^[0-9a-f]{64}$/.test(file.sha256)) throw new Error(`배포 manifest 항목 오류: ${file.path}`);
  }
  if (JSON.stringify(actual) !== JSON.stringify(recorded)) throw new Error('배포 파일 목록·크기·SHA256이 source-manifest와 다릅니다.');
  for (const required of ['index.html', 'README.md', 'NOTICE.md', 'wasm/augment_chess_browser.js', 'wasm/augment_chess_browser_bg.wasm', 'catalog.json', 'execution-profile.json', 'card-presentation.json', 'card-definitions.json']) {
    if (!recorded.some(file => file.path === required)) throw new Error(`배포 필수 파일 누락: ${required}`);
  }
  const readme = await readFile(join(directory, 'README.md'), 'utf8');
  if (!/^---\nsdk: static\napp_file: index.html\n---\n/.test(readme)) throw new Error('HF Static README 설정이 일치하지 않습니다.');
  return manifest;
}

async function bundle() {
  const paths = demoPaths();
  ensureOwnedDirectory(paths.site);
  ensureOwnedDirectory(paths.bundle);
  const assets = await filesAt(paths.site);
  if (!assets.some(file => file.path === 'index.html')) throw new Error('먼저 실제 정적 빌드를 실행해야 합니다.');
  const buildInputs = JSON.parse(await readFile(join(paths.reports, 'build-inputs.json'), 'utf8'));
  if (buildInputs.schemaVersion !== 1 || buildInputs.fingerprint !== await sourceFingerprint() || JSON.stringify(buildInputs.files) !== JSON.stringify(assets)) throw new Error('빌드 입력 또는 정적 파일이 바뀌었습니다. 현재 소스로 다시 빌드하세요.');
  const commit = execFileSync(gitExecutable, ['rev-parse', 'HEAD'], { cwd: repositoryRoot, encoding: 'utf8' }).trim();
  const dirty = execFileSync(gitExecutable, ['status', '--porcelain', '--untracked-files=normal'], { cwd: repositoryRoot, encoding: 'utf8' });
  if (dirty) throw new Error('배포 묶음은 커밋된 깨끗한 checkout에서 생성합니다. 변경 파일을 먼저 검토·커밋하세요.');
  // 소유한 고정 생성물 슬롯만 재생성한다. ensureOwnedDirectory가 link 경계를 검사한다.
  await rm(paths.bundle, { recursive: true });
  ensureOwnedDirectory(paths.bundle);
  for (const file of assets) {
    const target = join(paths.bundle, file.path);
    ensureOwnedDirectory(dirname(target));
    await copyFile(join(paths.site, file.path), target);
  }
  await writeFile(join(paths.bundle, 'README.md'), `---\nsdk: static\napp_file: index.html\n---\n\n# Augment Chess 엔진 시험 예제\n\n브라우저에서 Rust WASM 엔진을 실행하는 Svelte 예제입니다.\nAI 백엔드와 학습 모델의 준비 상태는 화면에서 별도로 표시합니다.\n공개 관측 기반 자료는 비공개 전체 상태의 완전 재현을 보장하지 않습니다.\n\n계획된 HF 소유자는 \`${HF_OWNER}\`이며 GitHub 저장소 소유자와 별개입니다.\nSpace 이름은 미정입니다. 이 묶음의 생성·검증은 Space 생성·업로드·게재를 실행하지 않습니다.\n\n원본 게임: https://augmentchess.org/\n소스와 파일 SHA256: [source-manifest.json](source-manifest.json)\n`);
  await copyFile(join(repositoryRoot, 'NOTICE.md'), join(paths.bundle, 'NOTICE.md'));
  const manifest = {
    schemaVersion: 1, sourceCommit: commit,
    sourceRepository: 'https://github.com/sungjeahyun100/Accelerate-alpha-zero-style-Augment-Chess-bot-',
    deploymentPlan: { provider: 'hugging-face', sdk: 'static', owner: HF_OWNER, spaceName: null },
    ai: { backend: 'backend-pending', model: 'model-missing' },
    files: await filesAt(paths.bundle)
  };
  await writeFile(join(paths.bundle, 'source-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  await verifyBundle(paths.bundle);
  console.log(JSON.stringify({ stage: 'verified-static-bundle', sourceCommit: commit, files: manifest.files.length }));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const command = process.argv[2];
  const action = command === 'bundle' ? bundle() : command === 'verify' ? verifyBundle(demoPaths().bundle).then(manifest => console.log(JSON.stringify({ sourceCommit: manifest.sourceCommit, verified: true }))) : Promise.reject(new Error('사용법: node scripts/delivery.mjs bundle|verify'));
  action.catch(error => { console.error(error); process.exitCode = 1; });
}
