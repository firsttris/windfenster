// `pnpm dev`: creates .env on the first run, then runs the Rust server (:8080) and the Vite
// dev server (:5173, proxies /api to :8080) side by side.
import { copyFileSync, existsSync } from 'node:fs';
import concurrently from 'concurrently';

if (!existsSync('.env')) {
  copyFileSync('.env.example', '.env');
  console.log('.env created from .env.example');
}

console.log('\n  Windfenster: http://localhost:5173\n');

const { result } = concurrently(
  [
    { name: 'server', command: 'cargo run -p windfenster', prefixColor: 'yellow' },
    { name: 'web', command: 'pnpm --filter @windfenster/web dev', prefixColor: 'cyan' },
  ],
  { killOthersOn: ['failure', 'success'] },
);
result.catch(() => process.exit(1));
