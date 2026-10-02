import { createHash } from 'node:crypto';
import { lstatSync, mkdirSync, realpathSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const demoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const repositoryRoot = resolve(demoRoot, '../../..');

export function isWithin(parent, child) {
  const part = relative(parent, child);
  return part === '' || (!part.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`) && part !== '..' && !isAbsolute(part));
}

export function demoPaths(environment = process.env) {
  let root;
  if (environment.ACCELERATE_OUTPUT_ROOT) root = environment.ACCELERATE_OUTPUT_ROOT;
  else if (environment.RUNNER_TEMP) root = join(environment.RUNNER_TEMP, 'Accelerate');
  else if (process.platform === 'win32' && environment.APPDATA) root = join(environment.APPDATA, 'Accelerate');
  else if (process.platform !== 'win32') root = join(environment.XDG_CACHE_HOME || join(homedir(), '.cache'), 'accelerate');
  else throw new Error('Windows APPDATA 또는 ACCELERATE_OUTPUT_ROOT가 필요합니다.');
  if (!isAbsolute(root)) throw new Error('ACCELERATE_OUTPUT_ROOT는 절대 경로여야 합니다.');
  root = resolve(root);
  if (isWithin(repositoryRoot, root) || isWithin(root, repositoryRoot)) throw new Error('생성물 루트와 checkout 경로가 겹칩니다.');
  const checkout = createHash('sha256').update(realpathSync(repositoryRoot)).digest('hex').slice(0, 12);
  const slot = join('hf-static-demo', checkout, process.platform);
  const build = join(root, 'build', slot);
  return {
    root, checkout, build,
    wasm: join(build, 'wasm'), wasmTest: join(build, 'wasm-test'), public: join(build, 'public'), site: join(build, 'site'),
    bundle: join(build, 'hf-bundle'), cargoTarget: join(root, 'cache', slot, 'cargo-target'),
    viteCache: join(root, 'cache', slot, 'vite'),
    reports: join(root, 'reports', slot),
    playwright: join(root, 'reports', slot, 'playwright')
  };
}

export function ensureOwnedDirectory(path, paths = demoPaths()) {
  if (!isWithin(paths.root, path) || path === paths.root) throw new Error('생성물 하위의 소유 경로가 필요합니다.');
  let ancestor = paths.root;
  const missing = [];
  while (true) {
    try { ancestor = realpathSync(ancestor); break; }
    catch (error) {
      if (error.code !== 'ENOENT') throw error;
      missing.unshift(basename(ancestor));
      ancestor = dirname(ancestor);
    }
  }
  const canonicalRoot = join(ancestor, ...missing);
  const canonicalRepository = realpathSync(repositoryRoot);
  if (isWithin(canonicalRepository, canonicalRoot) || isWithin(canonicalRoot, canonicalRepository)) throw new Error('생성물 최종 루트와 checkout 경로가 겹칩니다.');
  mkdirSync(paths.root, { recursive: true });
  if (lstatSync(paths.root).isSymbolicLink()) throw new Error('생성물 루트의 link/junction은 지원하지 않습니다.');
  let current = paths.root;
  for (const segment of relative(paths.root, path).split(/[\\/]/)) {
    current = join(current, segment);
    try {
      if (lstatSync(current).isSymbolicLink()) throw new Error(`생성물 경로의 link/junction 거부: ${current}`);
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      mkdirSync(current);
    }
  }
  const resolved = realpathSync(path);
  if (!isWithin(realpathSync(paths.root), resolved)) throw new Error('생성물 최종 경로가 루트에서 벗어났습니다.');
  return path;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(demoPaths(), null, 2));
}
