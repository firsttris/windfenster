// `bun run dev` (scripts/dev.ts) with stand-ins for `cargo run` and the web app's `bun run dev`:
// what is left running when the script itself is stopped.
import { afterEach, expect, test } from 'bun:test';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const DEV = resolve(import.meta.dir, 'dev.ts');

let dir = '';
let pids: number[] = [];

afterEach(() => {
  for (const pid of pids) {
    try {
      process.kill(pid, 'SIGKILL');
    } catch {}
  }
  pids = [];
  if (dir) rmSync(dir, { recursive: true, force: true });
});

/** A folder with `.env`, a web app whose dev script and a `cargo` that both sleep and note their pid */
function sandbox() {
  dir = mkdtempSync(join(tmpdir(), 'windfenster-dev-'));
  const bin = join(dir, 'bin');
  mkdirSync(bin);
  mkdirSync(join(dir, 'web'));
  writeFileSync(join(dir, '.env'), '');
  writeFileSync(join(bin, 'cargo'), `#!/bin/sh\necho $$ > "${dir}/server.pid"\nexec sleep 600\n`);
  chmodSync(join(bin, 'cargo'), 0o755);
  writeFileSync(
    join(dir, 'web', 'package.json'),
    JSON.stringify({ name: 'web', scripts: { dev: `echo $$ > "${dir}/web.pid" && exec sleep 600` } }),
  );
  return bin;
}

function alive(pid: number) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function until(check: () => boolean, ms = 10_000) {
  const end = Date.now() + ms;
  while (!check()) {
    if (Date.now() > end) throw new Error('timed out');
    await Bun.sleep(50);
  }
}

test('stopping `bun run dev` with SIGTERM stops the server and Vite too', async () => {
  const bin = sandbox();
  const dev = Bun.spawn([process.execPath, DEV], {
    cwd: dir,
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
    stdio: ['ignore', 'ignore', 'ignore'],
  });
  const files = ['server.pid', 'web.pid'].map((name) => join(dir, name));
  await until(() => files.every((file) => existsSync(file) && readFileSync(file, 'utf8').trim()));
  pids = files.map((file) => Number(readFileSync(file, 'utf8')));

  dev.kill('SIGTERM');
  await dev.exited;
  await Bun.sleep(500);

  expect(pids.filter(alive)).toEqual([]);
}, 20_000);
