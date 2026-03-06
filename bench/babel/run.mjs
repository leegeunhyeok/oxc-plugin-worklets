import { readFileSync } from 'node:fs';
import { transformSync } from '@babel/core';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixture = readFileSync(join(__dirname, '..', 'fixture.ts'), 'utf8');
const N = parseInt(process.argv[2] || '100', 10);

const fixturePath = join(__dirname, '..', 'fixture.ts');

const babelOptions = {
  filename: fixturePath,
  cwd: __dirname,
  plugins: [
    '@babel/plugin-transform-typescript',
    'react-native-worklets/plugin',
  ],
  configFile: false,
  babelrc: false,
};

// Warmup
transformSync(fixture, babelOptions);

const start = performance.now();
for (let i = 0; i < N; i++) {
  transformSync(fixture, babelOptions);
}
const elapsed = performance.now() - start;

console.log(`Babel: ${N} iterations`);
console.log(`  total: ${elapsed.toFixed(2)} ms`);
console.log(`  avg:   ${(elapsed / N).toFixed(2)} ms/transform`);
