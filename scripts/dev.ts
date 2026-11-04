// `bun run dev`: creates .env on the first run, then runs the Rust server (:8080) and the Vite
// dev server (:5173, proxies /api to :8080) side by side. When one of them exits, the other is
// stopped too (as `bun dev/dev.ts` in firsttris/kontext), so a crash leaves nothing half running.
// A signal to this script goes on to both, as concurrently did: stopped with SIGTERM (an IDE, kill),
// Bun would otherwise exit at once and leave the server on :8080 and Vite on :5173 running.
// Each runs in a process group of its own and signals go to the whole group (as scripts/dev.ts in
// firsttris/ccu-addon-mui), so they also reach what cargo and `bun run` start, even if those
// don't pass a signal on.
import { copyFileSync, existsSync } from 'node:fs';

if (!existsSync('.env')) {
  copyFileSync('.env.example', '.env');
  console.log('.env created from .env.example');
}

console.log('\n  Windfenster: http://localhost:5173\n');

const processes = [
  Bun.spawn(['cargo', 'run', '-p', 'windfenster'], { stdio: ['ignore', 'inherit', 'inherit'], detached: true }),
  Bun.spawn(['bun', 'run', 'dev'], { cwd: 'web', stdio: ['inherit', 'inherit', 'inherit'], detached: true }),
];

const signalAll = (signal: NodeJS.Signals) => {
  for (const child of processes) {
    try {
      process.kill(-child.pid, signal);
    } catch {
      // the group has already exited
    }
  }
};

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP'] as const) process.on(signal, () => signalAll(signal));

// Ctrl+Z (SIGTSTP) reaches only this script, the children's sessions are out of the terminal's reach:
// stop them too, or Vite keeps reading the terminal the shell has taken back and takes what is typed
// there. SIGSTOP, as the kernel drops SIGTSTP sent to an orphaned group (a session of its own is
// one); `fg` (SIGCONT) resumes them. As scripts/dev.ts in firsttris/haul.
process.on('SIGTSTP', () => {
  signalAll('SIGSTOP');
  process.kill(process.pid, 'SIGSTOP');
});
process.on('SIGCONT', () => signalAll('SIGCONT'));

// the exit code of the first to exit, 128 + n when a signal n stopped it
const code = await Promise.race(processes.map((child) => child.exited));
signalAll('SIGTERM');
await Promise.all(processes.map((child) => child.exited));
process.exit(code);
