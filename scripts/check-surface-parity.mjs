#!/usr/bin/env node
// Every sidecar method must be exposed by the Node SDK, the Go SDK and the
// operation catalog (CLI, MCP, Tack), unless scripts/surface-parity.json lists
// it as a known gap. New methods therefore land everywhere or fail CI, and a
// gap that gets closed must be removed from the list.
//
//   node scripts/check-surface-parity.mjs                  check
//   node scripts/check-surface-parity.mjs --write-baseline record current gaps
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = new URL('..', import.meta.url).pathname;
const configPath = join(root, 'scripts/surface-parity.json');
const read = path => readFileSync(join(root, path), 'utf8');
const sources = (dir, keep) => readdirSync(join(root, dir)).filter(keep).map(name => read(join(dir, name))).join('\n');

const methods = Object.keys(JSON.parse(read('clients/node/src/generated/schema-bundle.json')).methods).sort();
const node = sources('clients/node/src', name => name.endsWith('.ts'));
const go = sources('clients/go', name => name.endsWith('.go') && !name.endsWith('_test.go'));
const catalogCommands = new Set([...read('crates/eoka-protocol/src/catalog.rs').matchAll(/\bcmd:\s*"([^"]+)"/g)].map(match => match[1]));

const config = JSON.parse(readFileSync(configPath, 'utf8'));
const surfaces = {
  node: method => node.includes(`'${method}'`),
  go: method => go.includes(`"${method}"`),
  catalog: method => catalogCommands.has(config.catalog[method]),
};

const problems = [];
for (const [method, command] of Object.entries(config.catalog)) {
  if (!methods.includes(method)) problems.push(`catalog maps unknown sidecar method ${method}`);
  if (!catalogCommands.has(command)) problems.push(`catalog maps ${method} to unknown command ${command}`);
}

if (process.argv.includes('--write-baseline')) {
  config.gaps = Object.fromEntries(Object.entries(surfaces).map(([surface, covers]) => [surface, methods.filter(method => !covers(method))]));
  writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`);
  console.log(`Recorded gaps: ${Object.entries(config.gaps).map(([surface, gaps]) => `${surface} ${gaps.length}`).join(', ')}`);
  process.exit(0);
}

for (const [surface, covers] of Object.entries(surfaces)) {
  const gaps = new Set(config.gaps[surface] ?? []);
  for (const method of methods) {
    if (!covers(method) && !gaps.has(method)) problems.push(`${surface} does not expose ${method}`);
    if (covers(method) && gaps.has(method)) problems.push(`${surface} now exposes ${method}; remove it from gaps.${surface}`);
  }
  for (const method of gaps) {
    if (!methods.includes(method)) problems.push(`gaps.${surface} lists unknown sidecar method ${method}`);
  }
}

if (problems.length > 0) {
  console.error(`Surface parity failed (see ${configPath.slice(root.length)}):\n${problems.map(problem => `  - ${problem}`).join('\n')}`);
  process.exit(1);
}
console.log(`Surface parity: ${methods.length} sidecar methods checked across Node, Go and the operation catalog.`);
