import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { chmod, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { promisify } from 'node:util';
import { npmEnvironment, validatePublicationAuth, validatePublicationReview } from '../scripts/publication-auth.mjs';

const exec = promisify(execFile);
const secret = 'isolated-test-token-not-a-real-credential';
const bootstrapEnvironment = { NPM_BOOTSTRAP: 'true', GITHUB_JOB: 'publish', NODE_AUTH_TOKEN: secret };
const oidcEnvironment = { ACTIONS_ID_TOKEN_REQUEST_URL: 'https://oidc.example.invalid/token', ACTIONS_ID_TOKEN_REQUEST_TOKEN: 'isolated-oidc-request' };
const aliases = ['NPM_TOKEN', 'NPM_AUTH_TOKEN', 'NPM_BOOTSTRAP_TOKEN'];

async function temporaryDirectory(t) {
  const directory = await mkdtemp(join(tmpdir(), 'eoka-publication-auth-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

function rejectsWithoutValue(action, variable, value = secret) {
  assert.throws(action, error => {
    assert.ok(error instanceof Error);
    assert.ok(error.message.includes(variable));
    assert.ok(!error.message.includes(value));
    return true;
  });
}

async function childTokens(environment, directory) {
  const { stdout } = await exec(process.execPath, ['-e', 'process.stdout.write(JSON.stringify(Object.fromEntries(Object.entries(process.env).filter(([name]) => /^(?:NODE_AUTH_TOKEN|NPM_TOKEN|NPM_AUTH_TOKEN|NPM_BOOTSTRAP_TOKEN)$/i.test(name)))))'], { env: environment, cwd: directory });
  return JSON.parse(stdout);
}

test('publication auth defaults to OIDC and accepts only exact bootstrap values', () => {
  assert.equal(validatePublicationAuth({}, 'publish'), false);
  for (const setting of ['', 'false']) assert.equal(validatePublicationAuth({ NPM_BOOTSTRAP: setting }, 'publish'), false);
  for (const command of ['gate', 'download', 'verify', 'smoke']) assert.equal(validatePublicationAuth({ NPM_BOOTSTRAP: 'true' }, command), true);
  assert.equal(validatePublicationAuth(bootstrapEnvironment, 'publish'), true);
  for (const setting of [secret, 'TRUE', '1', ' false ', true, false, null]) {
    rejectsWithoutValue(() => validatePublicationAuth({ NPM_BOOTSTRAP: setting }, 'publish'), 'NPM_BOOTSTRAP');
  }
});

test('NODE_AUTH_TOKEN is admitted only for explicit bootstrap publication in the publish job', () => {
  for (const NPM_BOOTSTRAP of [undefined, '', 'false']) {
    rejectsWithoutValue(() => validatePublicationAuth({ NPM_BOOTSTRAP, GITHUB_JOB: 'publish', NODE_AUTH_TOKEN: secret }, 'publish'), 'NODE_AUTH_TOKEN');
  }
  for (const command of ['gate', 'download', 'verify', 'smoke', undefined]) {
    rejectsWithoutValue(() => validatePublicationAuth(bootstrapEnvironment, command), 'NODE_AUTH_TOKEN');
  }
  for (const GITHUB_JOB of [undefined, '', 'gate', 'verify', 'smoke', 'Publish']) {
    rejectsWithoutValue(() => validatePublicationAuth({ ...bootstrapEnvironment, GITHUB_JOB }, 'publish'), 'NODE_AUTH_TOKEN');
  }
  for (const NODE_AUTH_TOKEN of [undefined, '', ' ', '\n\t']) {
    rejectsWithoutValue(() => validatePublicationAuth({ ...bootstrapEnvironment, NODE_AUTH_TOKEN }, 'publish'), 'NODE_AUTH_TOKEN');
  }
  rejectsWithoutValue(() => validatePublicationAuth({ NPM_BOOTSTRAP: 'true' }, 'publish'), 'GITHUB_JOB');
  rejectsWithoutValue(() => validatePublicationAuth({ NODE_AUTH_TOKEN: '' }, 'publish'), 'NODE_AUTH_TOKEN');
});

test('token aliases are forbidden in every command even with bootstrap enabled', () => {
  for (const name of [...aliases, 'npm_token', 'Node_Auth_Token']) {
    for (const command of ['gate', 'download', 'verify', 'publish', 'smoke']) {
      rejectsWithoutValue(() => validatePublicationAuth({ NPM_BOOTSTRAP: 'true', GITHUB_JOB: 'publish', [name]: secret }, command), name);
      rejectsWithoutValue(() => validatePublicationAuth({ [name]: secret }, command), name);
    }
    rejectsWithoutValue(() => validatePublicationAuth({ ...bootstrapEnvironment, [name]: '' }, 'publish'), name);
  }
});

test('source, build, Rust and npm authentication overrides remain forbidden', () => {
  const names = [
    'EOKA_CORE_REF', 'EOKA_CORE_PATH', 'EOKA_SERVER_BIN', 'EOKA_CHROME_PATH',
    'CARGO_PATCH_CRATES_IO_EOKA_PATH', 'CARGO_SOURCE_CRATES_IO_REPLACE_WITH',
    'CARGO_BUILD_RUSTFLAGS', 'RUSTFLAGS', 'npm_config__authToken',
    'NPM_CONFIG_//REGISTRY.NPMJS.ORG/:_AUTHTOKEN', 'NpM_cOnFiG__aUtH',
  ];
  for (const name of names) {
    rejectsWithoutValue(() => validatePublicationAuth({ [name]: secret }, 'gate'), name);
    rejectsWithoutValue(() => validatePublicationAuth({ ...bootstrapEnvironment, [name]: secret }, 'publish'), name);
  }
});

test('credential-free npm environments strip all inherited configs and token aliases', async t => {
  const directory = await temporaryDirectory(t);
  const environment = {
    ...bootstrapEnvironment,
    ...oidcEnvironment,
    NPM_TOKEN: secret,
    NPM_AUTH_TOKEN: secret,
    NPM_BOOTSTRAP_TOKEN: secret,
    node_auth_token: secret,
    npm_config__authToken: secret,
    NpM_cOnFiG_UsErCoNfIg: '/inherited/npmrc',
    NPM_CONFIG_GLOBALCONFIG: '/inherited/global-npmrc',
    npm_config_registry: 'https://registry.example.invalid/',
    npm_config_cache: '/inherited/cache',
    npm_config_fetch_retries: '10',
    UNRELATED: 'preserved',
  };
  const original = { ...environment };
  const child = await npmEnvironment(directory, { environment });
  assert.deepEqual(environment, original);
  assert.deepEqual(await childTokens(child, directory), {});
  assert.equal(child.NPM_CONFIG_USERCONFIG, join(directory, 'npmrc'));
  assert.equal(child.NPM_CONFIG_GLOBALCONFIG, join(directory, 'global-npmrc'));
  assert.equal(child.NPM_CONFIG_REGISTRY, 'https://registry.npmjs.org/');
  assert.equal(child.NPM_CONFIG_CACHE, join(directory, 'npm-cache'));
  assert.deepEqual(Object.keys(child).filter(key => /^npm_config_/i.test(key)).sort(), ['NPM_CONFIG_CACHE', 'NPM_CONFIG_FETCH_RETRIES', 'NPM_CONFIG_GLOBALCONFIG', 'NPM_CONFIG_REGISTRY', 'NPM_CONFIG_USERCONFIG']);
  assert.equal(await readFile(child.NPM_CONFIG_USERCONFIG, 'utf8'), '');
  assert.equal(await readFile(child.NPM_CONFIG_GLOBALCONFIG, 'utf8'), '');
});

test('default process environment cannot leak a bootstrap token into verification children', async t => {
  const directory = await temporaryDirectory(t);
  const injected = { ...bootstrapEnvironment, NPM_BOOTSTRAP_TOKEN: secret, npm_config__authToken: secret };
  const previous = Object.fromEntries(Object.keys(injected).map(key => [key, process.env[key]]));
  try {
    Object.assign(process.env, injected);
    const child = await npmEnvironment(directory);
    assert.deepEqual(await childTokens(child, directory), {});
    assert.equal(await readFile(child.NPM_CONFIG_USERCONFIG, 'utf8'), '');
    assert.equal(await readFile(child.NPM_CONFIG_GLOBALCONFIG, 'utf8'), '');
    assert.equal(process.env.NODE_AUTH_TOKEN, secret);
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
});

test('authenticated npm config keeps the token out of files and subsequent verification', async t => {
  const directory = await temporaryDirectory(t);
  const environment = { ...bootstrapEnvironment, ...oidcEnvironment, NPM_CONFIG_REGISTRY: 'https://registry.example.invalid/' };
  const child = await npmEnvironment(directory, { environment, authenticated: true });
  assert.equal(await readFile(child.NPM_CONFIG_USERCONFIG, 'utf8'), '//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}\n');
  assert.equal(await readFile(child.NPM_CONFIG_GLOBALCONFIG, 'utf8'), '');
  assert.equal(child.NPM_CONFIG_REGISTRY, 'https://registry.npmjs.org/');
  const verificationDirectory = join(directory, 'verification');
  await mkdir(verificationDirectory);
  const verification = await npmEnvironment(verificationDirectory, { environment: child });
  assert.deepEqual(await childTokens(verification, verificationDirectory), {});
  assert.equal(await readFile(verification.NPM_CONFIG_USERCONFIG, 'utf8'), '');
  assert.equal(await readFile(child.NPM_CONFIG_USERCONFIG, 'utf8'), '//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}\n');
});

test('authenticated npm environments reject missing opt-in, missing tokens, wrong jobs and aliases before writing files', async t => {
  const directory = await temporaryDirectory(t);
  const environments = [
    {},
    { GITHUB_JOB: 'publish' },
    { ...bootstrapEnvironment, NPM_BOOTSTRAP: 'false' },
    { ...bootstrapEnvironment, NPM_BOOTSTRAP: 'true-with-secret' },
    { ...bootstrapEnvironment, NODE_AUTH_TOKEN: undefined },
    { ...bootstrapEnvironment, NODE_AUTH_TOKEN: '' },
    { ...bootstrapEnvironment, GITHUB_JOB: 'verify' },
    ...aliases.map(name => ({ ...bootstrapEnvironment, [name]: secret })),
    { ...bootstrapEnvironment, npm_config__authToken: secret },
    { ...bootstrapEnvironment, RUSTFLAGS: secret },
  ];
  for (const environment of environments) {
    await assert.rejects(npmEnvironment(directory, { environment, authenticated: true }), error => {
      assert.ok(!error.message.includes(secret));
      assert.match(error.message, /NPM_BOOTSTRAP|NODE_AUTH_TOKEN|GITHUB_JOB|NPM_TOKEN|NPM_AUTH_TOKEN|npm_config__authToken|RUSTFLAGS/);
      return true;
    });
    assert.deepEqual(await readdir(directory), []);
  }
});

test('only boolean true can request an authenticated npm environment', async t => {
  const directory = await temporaryDirectory(t);
  for (const authenticated of [false, 'true', 'false', 1, null]) {
    const child = await npmEnvironment(directory, { environment: bootstrapEnvironment, authenticated });
    assert.deepEqual(await childTokens(child, directory), {});
    assert.equal(await readFile(child.NPM_CONFIG_USERCONFIG, 'utf8'), '');
  }
});

test('npm configuration files have restrictive POSIX modes even when replacing existing files', { skip: process.platform === 'win32' }, async t => {
  const directory = await temporaryDirectory(t);
  for (const authenticated of [false, true]) {
    for (const existing of [false, true]) {
      const isolated = join(directory, `${authenticated}-${existing}`);
      await mkdir(isolated);
      if (existing) {
        for (const name of ['npmrc', 'global-npmrc']) {
          await writeFile(join(isolated, name), secret);
          await chmod(join(isolated, name), 0o666);
        }
      }
      const child = await npmEnvironment(isolated, { environment: bootstrapEnvironment, authenticated });
      for (const path of [child.NPM_CONFIG_USERCONFIG, child.NPM_CONFIG_GLOBALCONFIG]) {
        assert.equal((await stat(path)).mode & 0o777, 0o600);
        assert.ok(!(await readFile(path, 'utf8')).includes(secret));
      }
    }
  }
});

test('review policy always requires approval and defaults to independent approval', () => {
  const reviewers = { type: 'required_reviewers', reviewers: [{ type: 'User', reviewer: { login: 'owner' } }], prevent_self_review: true };
  validatePublicationReview(reviewers);
  validatePublicationReview(reviewers, false);
  validatePublicationReview(reviewers, true);
  assert.throws(() => validatePublicationReview({ ...reviewers, prevent_self_review: false }), /prevent self-review/);
  assert.throws(() => validatePublicationReview({ ...reviewers, prevent_self_review: false }, false), /prevent self-review/);
  validatePublicationReview({ ...reviewers, prevent_self_review: false }, true);
  for (const bootstrap of ['true', 'false', 1, null]) {
    assert.throws(() => validatePublicationReview({ ...reviewers, prevent_self_review: false }, bootstrap), /NPM_BOOTSTRAP/);
  }
  for (const bootstrap of [false, true]) {
    for (const policy of [undefined, null, {}, { ...reviewers, type: 'wait_timer' }, { ...reviewers, reviewers: [] }, { ...reviewers, reviewers: undefined }, { ...reviewers, reviewers: 'owner' }]) {
      assert.throws(() => validatePublicationReview(policy, bootstrap), /require reviewer approval/);
    }
    for (const prevent_self_review of [undefined, null, '', 'false', 'true', 0]) {
      assert.throws(() => validatePublicationReview({ ...reviewers, prevent_self_review }, bootstrap), /prevent_self_review/);
    }
  }
});
