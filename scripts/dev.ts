// `bun run dev`: creates .env on the first run, then runs the Rust server (:8080) and the Vite
// dev server (:5173, proxies /api to :8080) side by side. When one of them exits, the other is
// stopped too (as `bun dev/dev.ts` in firsttris/kontext), so a crash leaves nothing half running.
import { copyFileSync, existsSync } from 'node:fs';

if (!existsSync('.env')) {
  copyFileSync('.env.example', '.env');
  console.log('.env created from .env.example');
}

console.log('\n  Windfenster: http://localhost:5173\n');

const processes = [
  Bun.spawn(['cargo', 'run', '-p', 'windfenster'], { stdio: ['ignore', 'inherit', 'inherit'] }),
  Bun.spawn(['bun', 'run', 'dev'], { cwd: 'web', stdio: ['inherit', 'inherit', 'inherit'] }),
];

const code = await Promise.race(processes.map((child) => child.exited));
for (const child of processes) child.kill();
await Promise.all(processes.map((child) => child.exited));
process.exit(code === 0 ? 0 : 1);
