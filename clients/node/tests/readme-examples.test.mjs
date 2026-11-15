import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import * as sdk from '../dist/index.js';

const packageRoot = fileURLToPath(new URL('../', import.meta.url));
const readme = await readFile(new URL('../README.md', import.meta.url), 'utf8');
const javascript = new Set(['js', 'javascript', 'mjs']);

function fencedBlocks(markdown) {
  const blocks = [];
  let heading = '';
  let open;
  markdown.split('\n').forEach((line, index) => {
    const fence = /^\s*(`{3,}|~{3,})\s*([\w+-]*)([^`]*)$/.exec(line);
    if (open === undefined) {
      if (fence) open = { marker: fence[1], language: fence[2].toLowerCase(), line: index + 2, heading, lines: [] };
      else if (/^#{1,6}\s/.test(line)) heading = line.replace(/^#+\s*/, '').trim();
      return;
    }
    if (fence && fence[1][0] === open.marker[0] && fence[1].length >= open.marker.length && `${fence[2]}${fence[3]}`.trim() === '') {
      blocks.push({ language: open.language, line: open.line, heading: open.heading, code: `${open.lines.join('\n')}\n` });
      open = undefined;
      return;
    }
    open.lines.push(line);
  });
  assert.equal(open, undefined, 'README.md ends inside an unclosed code fence');
  return blocks;
}

const examples = fencedBlocks(readme).filter(block => javascript.has(block.language));
const label = block => `README.md line ${block.line} under "${block.heading}"`;

async function workspace(t, prefix) {
  const directory = await mkdtemp(join(tmpdir(), prefix));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await mkdir(join(directory, 'node_modules', '@cbxss'), { recursive: true });
  await symlink(packageRoot, join(directory, 'node_modules', '@cbxss', 'eoka'), 'dir');
  return directory;
}

function execute(file, args, options) {
  return new Promise(resolve => {
    execFile(file, args, options, (error, stdout, stderr) => {
      resolve({ code: error === null ? 0 : error.code ?? error.signal ?? 'failed', stdout, stderr });
    });
  });
}

test('README has JavaScript examples and opens with a complete program', () => {
  assert.ok(examples.length > 0, 'README.md has no fenced js block');
  const [first] = examples;
  assert.match(first.code, /^import\s[^;]*["']@cbxss\/eoka["']/m, `${label(first)} does not import @cbxss/eoka`);
  assert.match(first.code, /Browser\.launch\(/, `${label(first)} does not launch a browser`);
});

test('every README js block parses as an ES module', { timeout: 60000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), 'eoka-readme-parse-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  for (const [index, block] of examples.entries()) {
    await t.test(label(block), async () => {
      const file = join(directory, `example-${index + 1}.mjs`);
      await writeFile(file, block.code);
      const checked = await execute(process.execPath, ['--check', file], { timeout: 30000 });
      assert.equal(checked.code, 0, `${label(block)} does not parse:\n${checked.stderr}`);
    });
  }
});

test('every README js block type-checks against the built declarations', { timeout: 120000 }, async t => {
  const directory = await workspace(t, 'eoka-readme-types-');
  const files = [];
  for (const [index, block] of examples.entries()) {
    const file = join(directory, `example-${index + 1}.mjs`);
    await writeFile(file, block.code);
    files.push(file);
  }
  const ambient = join(directory, 'ambient.d.ts');
  const options = {
    allowJs: true,
    checkJs: true,
    noEmit: true,
    strict: true,
    skipLibCheck: true,
    target: ts.ScriptTarget.ES2024,
    module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
    moduleDetection: ts.ModuleDetectionKind.Force,
    lib: ['lib.es2024.d.ts', 'lib.dom.d.ts'],
    types: ['node'],
    typeRoots: [join(packageRoot, 'node_modules', '@types')],
  };
  const assumed = new Set();
  const unknownName = 2304;
  let diagnostics = [];
  for (let round = 0; round < 3; round++) {
    await writeFile(ambient, [
      "import type * as sdk from '@cbxss/eoka';",
      'declare global {',
      '  const page: sdk.Page;',
      '  const browser: sdk.Browser;',
      ...Object.keys(sdk).map(name => `  const ${name}: typeof sdk.${name};`),
      ...[...assumed].map(name => `  const ${name}: any;`),
      '}',
      '',
    ].join('\n'));
    diagnostics = ts.getPreEmitDiagnostics(ts.createProgram([...files, ambient], options));
    const carried = diagnostics
      .filter(diagnostic => diagnostic.code === unknownName)
      .map(diagnostic => /'([^']+)'/.exec(ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'))?.[1])
      .filter(name => name !== undefined && /^[A-Za-z_$][\w$]*$/.test(name) && !assumed.has(name));
    if (carried.length === 0) break;
    for (const name of carried) assumed.add(name);
  }
  if (assumed.size) t.diagnostic(`names carried over from earlier examples and assumed to exist: ${[...assumed].join(', ')}`);
  const problems = diagnostics.map(diagnostic => {
    const index = files.indexOf(diagnostic.file?.fileName);
    const block = examples[index];
    const position = diagnostic.file && diagnostic.start !== undefined ? diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start).line : 0;
    const where = block ? `README.md line ${block.line + position}` : diagnostic.file?.fileName ?? 'compiler options';
    return `${where}: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')}`;
  });
  assert.deepEqual(problems, []);
});

test('K13: the Quick start block runs twice in the same directory against a local fixture', {
  skip: !process.env.EOKA_RELIABILITY_SERVER,
  timeout: 180000,
}, async t => {
  assert.ok(process.env.EOKA_RELIABILITY_CHROME);
  assert.ok(examples.length > 0, 'README.md has no fenced js block');
  const [quickStart] = examples;
  let documents = 0;
  const server = createServer((request, response) => {
    if (new URL(request.url, 'http://fixture.invalid').pathname !== '/favicon.ico') documents++;
    response.writeHead(200, { 'content-type': 'text/html' }).end('<!doctype html><title>quick start fixture</title><h1>Quick start</h1>');
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(async () => {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  });
  const origin = `http://127.0.0.1:${server.address().port}`;
  const directory = await workspace(t, 'eoka-readme-run-');
  const script = join(directory, 'quick-start.mjs');
  await writeFile(script, quickStart.code.replace(/https?:\/\/[A-Za-z0-9.-]+(?::\d+)?/g, origin));
  const env = { ...process.env, EOKA_SERVER_BIN: process.env.EOKA_RELIABILITY_SERVER, EOKA_CHROME_PATH: process.env.EOKA_RELIABILITY_CHROME };
  const runs = [];
  for (const attempt of ['first', 'second']) {
    const before = documents;
    const started = performance.now();
    const ran = await execute(process.execPath, [script], { cwd: directory, env, timeout: 80000 });
    const failure = ran.stderr.split('\n').find(line => /^\s*\w*Error\b.*:/.test(line))?.trim() ?? ran.stderr.trim().slice(0, 300);
    t.diagnostic(`${attempt} run: exit ${ran.code} in ${Math.round(performance.now() - started)} ms${ran.code === 0 ? '' : `: ${failure}`}`);
    runs.push({ attempt, ran, loaded: documents > before });
  }
  for (const { attempt, ran, loaded } of runs) {
    assert.equal(ran.code, 0, `the ${attempt} run of ${label(quickStart)} failed:\n${ran.stderr}\n${ran.stdout}`);
    assert.ok(loaded, `the ${attempt} run of ${label(quickStart)} never loaded the fixture`);
  }
});
