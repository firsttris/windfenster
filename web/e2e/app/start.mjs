/**
 * Starts what the app tests run against: the stand-in for every outside source, then the real
 * server (`WF_BIN`, else the workspace's debug build) on a fresh data directory with the map's
 * textures written in, its station lists imported from the stand-in, and no background loops.
 * Playwright runs it as a web server and stops it after the tests.
 */
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { writeMapData } from './map-data.mjs';
import { startStandIn } from './stand-in.mjs';

const STAND_IN_PORT = 5298;
/** Where the data folder's path is written for the tests */
export const DATA_DIR_NOTE = join(tmpdir(), 'windfenster-e2e-data-dir');
const bin = process.env.WF_BIN || resolve(import.meta.dirname, '../../../target/debug/windfenster');
const data = mkdtempSync(join(tmpdir(), 'windfenster-e2e-'));
// The admin test makes its account an admin on the command line, as an operator does: it needs
// the data folder.
writeFileSync(DATA_DIR_NOTE, data);
const now = Math.floor(Date.now() / 1000);

const standIn = await startStandIn(STAND_IN_PORT, now);
writeMapData(join(data, 'map'), now);
const env = {
  ...process.env,
  WF_DATA_DIR: data,
  WF_SOURCES: `http://127.0.0.1:${STAND_IN_PORT}`,
  WF_SCHEDULER: 'false',
  WF_BASEMAP: 'off',
  WF_LISTEN: '127.0.0.1:5299',
  WF_PUBLIC_URL: 'http://localhost:5299',
  RUST_LOG: process.env.RUST_LOG ?? 'warn',
  // The stand-in is local, whatever proxy the machine has
  NO_PROXY: '127.0.0.1,localhost',
  no_proxy: '127.0.0.1,localhost',
};
// Not spawnSync: the stand-in answers from this same process.
const [code] = await once(spawn(bin, ['import-stations'], { env, stdio: 'inherit' }), 'exit');
if (code !== 0) throw new Error(`import-stations failed (${bin})`);
const server = spawn(bin, ['serve'], { env, stdio: 'inherit' });

function stop() {
  server.kill('SIGTERM');
  standIn.close();
  rmSync(data, { recursive: true, force: true });
  process.exit(0);
}
process.on('SIGTERM', stop);
process.on('SIGINT', stop);
server.on('exit', (code) => {
  standIn.close();
  rmSync(data, { recursive: true, force: true });
  process.exit(code ?? 1);
});
