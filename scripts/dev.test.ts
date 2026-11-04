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

/**
 * As `sandbox`, but like the real ones the stand-ins start what does the work as a child of their
 * own (cargo the `windfenster` binary, `bun run` Vite) and wait for it: a plain `sleep` that notes
 * its pid. The parents are Bun scripts, as `sh` would start a background `sleep` with SIGINT ignored.
 * The `cargo` stand-in exits with 3 once `crash` exists, as a server that fails.
 */
function sandboxWithGrandchildren() {
  dir = mkdtempSync(join(tmpdir(), 'windfenster-dev-'));
  const bin = join(dir, 'bin');
  mkdirSync(bin);
  mkdirSync(join(dir, 'web'));
  writeFileSync(join(dir, '.env'), '');
  const parent = (pidFile: string, crash: boolean) =>
    [
      `const child = Bun.spawn(['sleep', '600'], { stdio: ['ignore', 'ignore', 'ignore'] });`,
      `await Bun.write(${JSON.stringify(join(dir, pidFile))}, String(child.pid));`,
      crash
        ? `while (!(await Bun.file(${JSON.stringify(join(dir, 'crash'))}).exists())) await Bun.sleep(50);\nprocess.exit(3);`
        : 'await child.exited;',
      '',
    ].join('\n');
  writeFileSync(join(bin, 'cargo'), `#!${process.execPath}\n${parent('server.pid', true)}`);
  chmodSync(join(bin, 'cargo'), 0o755);
  writeFileSync(join(dir, 'web', 'vite.ts'), parent('web.pid', false));
  writeFileSync(join(dir, 'web', 'package.json'), JSON.stringify({ name: 'web', scripts: { dev: 'bun vite.ts' } }));
  return bin;
}

/** Whether `pid` still runs: a zombie (stopped, not yet reaped by an init that reaps late) does not */
function alive(pid: number) {
  try {
    process.kill(pid, 0);
  } catch {
    return false;
  }
  try {
    return readFileSync(`/proc/${pid}/stat`, 'utf8').split(') ').at(-1)?.[0] !== 'Z';
  } catch {
    return !existsSync('/proc/self/stat');
  }
}

async function until(check: () => boolean, ms = 10_000) {
  const end = Date.now() + ms;
  while (!check()) {
    if (Date.now() > end) throw new Error('timed out');
    await Bun.sleep(50);
  }
}

/** Runs dev.ts with the stand-ins in `bin` and waits until both have noted their pid */
async function start(bin: string) {
  const dev = Bun.spawn([process.execPath, DEV], {
    cwd: dir,
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
    stdio: ['ignore', 'ignore', 'ignore'],
  });
  const files = ['server.pid', 'web.pid'].map((name) => join(dir, name));
  await until(() => files.every((file) => existsSync(file) && readFileSync(file, 'utf8').trim()));
  pids = files.map((file) => Number(readFileSync(file, 'utf8')));
  return dev;
}

test('stopping `bun run dev` with SIGTERM stops the server and Vite too', async () => {
  const dev = await start(sandbox());

  dev.kill('SIGTERM');
  await dev.exited;
  await Bun.sleep(500);

  expect(pids.filter(alive)).toEqual([]);
}, 20_000);

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP'] as const) {
  test(`${signal} also stops what cargo and \`bun run\` started`, async () => {
    const dev = await start(sandboxWithGrandchildren());

    dev.kill(signal);
    await dev.exited;
    await Bun.sleep(500);

    expect(pids.filter(alive)).toEqual([]);
  }, 20_000);
}

test('when the server exits, Vite started by `bun run` is stopped and its exit code passed on', async () => {
  const dev = await start(sandboxWithGrandchildren());

  writeFileSync(join(dir, 'crash'), '');
  const code = await dev.exited;
  await Bun.sleep(500);

  expect(pids.filter(alive)).toEqual([]);
  expect(code).toBe(3);
}, 20_000);
