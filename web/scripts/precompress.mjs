/**
 * Puts a Brotli (`.br`) and a gzip (`.gz`) copy beside every compressible file of the build, at
 * the highest levels, so the server sends them as they are instead of compressing the bundle
 * for every new visitor (crates/wf-api/src/spa.rs picks the copy the browser accepts). Brotli
 * for HTTPS, gzip for plain http, where browsers do not offer Brotli. Node's own zlib: no
 * dependency. Run after `vite build`.
 */
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

const DIST = resolve(import.meta.dirname, '../dist');
/** Text formats; images and fonts are compressed already */
const TEXT = /\.(js|mjs|css|json|html|svg|webmanifest|txt)$/;
/** Below this a copy saves next to nothing */
const MIN_BYTES = 1024;

function* files(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) yield* files(path);
    else yield path;
  }
}

let [count, before, after] = [0, 0, 0];
for (const path of files(DIST)) {
  if (!TEXT.test(path)) continue;
  const data = readFileSync(path);
  if (data.length < MIN_BYTES) continue;
  const br = brotliCompressSync(data, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY,
      [constants.BROTLI_PARAM_SIZE_HINT]: data.length,
    },
  });
  writeFileSync(`${path}.br`, br);
  writeFileSync(`${path}.gz`, gzipSync(data, { level: 9 }));
  [count, before, after] = [count + 1, before + data.length, after + br.length];
}
const mb = (n) => (n / 1e6).toFixed(1);
console.log(`precompressed ${count} files: ${mb(before)} MB → ${mb(after)} MB with Brotli`);
