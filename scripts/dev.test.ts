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

/** Kills what the shell `shellPid` still runs (dev.ts, stopped or not) and the shell itself */
function killShell(shellPid: number) {
  const children = Bun.spawnSync(['ps', '-o', 'pid=', '--ppid', String(shellPid)]).stdout.toString();
  for (const pid of [...children.trim().split(/\s+/).filter(Boolean).map(Number), shellPid]) {
    try {
      process.kill(pid, 'SIGKILL');
    } catch {}
  }
}

test.skipIf(process.platform !== 'linux' || !Bun.which('script'))(
  'after Ctrl+Z in a terminal, what is typed goes to the shell, not to Vite',
  async () => {
    const bin = sandbox();
    // Vite reads the keys of its shortcuts (h, r, o, q) line by line from stdin when it is a terminal
    writeFileSync(
      join(dir, 'web', 'vite.ts'),
      [
        `import { appendFileSync, writeFileSync } from 'node:fs';`,
        `import { createInterface } from 'node:readline';`,
        `createInterface({ input: process.stdin }).on('line', (line) => appendFileSync(${JSON.stringify(join(dir, 'typed'))}, line + '\\n'));`,
        `writeFileSync(${JSON.stringify(join(dir, 'web.pid'))}, String(process.pid));`,
        'setInterval(() => {}, 1000);',
      ].join('\n'),
    );
    writeFileSync(join(dir, 'web', 'package.json'), JSON.stringify({ name: 'web', scripts: { dev: 'bun vite.ts' } }));
    // an interactive shell in a pseudo-terminal, typed into through `script`
    const shell = Bun.spawn(['script', '-qfec', 'bash --norc --noprofile -i', '/dev/null'], {
      cwd: dir,
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
      stdin: 'pipe',
      stdout: 'pipe',
      stderr: 'ignore',
    });
    let output = '';
    (async () => {
      for await (const chunk of shell.stdout) output += new TextDecoder().decode(chunk);
    })();
    const type = (text: string) => {
      shell.stdin.write(text);
      shell.stdin.flush();
    };
    type(`echo $$ > shell.pid; ${process.execPath} ${DEV}\n`);
    const files = ['server.pid', 'web.pid', 'shell.pid'].map((name) => join(dir, name));
    await until(() => files.every((file) => existsSync(file) && readFileSync(file, 'utf8').trim()));
    const [server, web, shellPid] = files.map((file) => Number(readFileSync(file, 'utf8')));
    pids = [server, web];
    await Bun.sleep(500);

    try {
      type('\x1a');
      await Bun.sleep(500);
      // in canonical mode each line goes to one reader: whoever reads the terminal first
      const lines = [1, 2, 3, 4, 5].map((n) => `typed-${n * 42}`);
      for (const n of [1, 2, 3, 4, 5]) {
        type(`echo typed-$((${n} * 42))\n`);
        await Bun.sleep(300);
      }
      await until(() => lines.every((line) => output.includes(line)), 3_000).catch(() => {});

      expect(existsSync(join(dir, 'typed')) ? readFileSync(join(dir, 'typed'), 'utf8') : '').toBe('');
      expect(lines.filter((line) => !output.includes(line))).toEqual([]);
    } finally {
      killShell(shellPid);
      shell.kill('SIGKILL');
    }
  },
  20_000,
);
