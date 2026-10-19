import assert from 'node:assert/strict';
import { access, chmod, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { Browser, ConfigurationError, OperationError, ProtocolError } from '../dist/index.js';
import { SidecarTransport } from '../dist/transport.js';

const supported = process.platform !== 'win32';
const endpoint = 'ws://127.0.0.1:9222/devtools/browser/lifecycle-fixture';

async function fixture(t, mode = 'normal') {
  const directory = await mkdtemp(join(tmpdir(), 'eoka-node-browser-'));
  const serverPath = join(directory, 'sidecar');
  const logPath = join(directory, 'events.jsonl');
  await writeFile(logPath, '');
  await writeFile(serverPath, `#!${process.execPath}\nimport ${JSON.stringify(new URL('./fixtures/browser-sidecar.mjs', import.meta.url).href)};\n`);
  await chmod(serverPath, 0o700);
  const browsers = [];
  t.after(async () => {
    try {
      for (const browser of browsers) await browser.close();
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });
  const options = {
    serverPath,
    executablePath: process.execPath,
    shutdownTimeoutMs: 2000,
    runtimeDir: directory,
    serverEnv: { EOKA_BROWSER_FIXTURE_LOG: logPath, EOKA_BROWSER_FIXTURE_MODE: mode },
  };
  const events = async () => (await readFile(logPath, 'utf8')).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
  const waitFor = async predicate => {
    const deadline = performance.now() + 4000;
    while (performance.now() < deadline) {
      const current = await events();
      const found = current.find(predicate);
      if (found) return found;
      await delay(10);
    }
    assert.fail(`Fixture transition was not observed: ${JSON.stringify(await events())}`);
  };
  return { options, directory, events, waitFor, logPath, track: browser => browsers.push(browser) };
}

function connectOptions(options, extra = {}) {
  const { executablePath, runtimeDir, ...shared } = options;
  return { ...shared, browserWSEndpoint: endpoint, ...extra };
}

test('invalid launch and connect inputs fail before sidecar spawn', { skip: !supported }, async t => {
  const f = await fixture(t);
  for (const extra of [
    { viewport: { width: 0, height: 300 } },
    { viewport: { width: 640, height: 300, deviceScaleFactor: Infinity } },
    { sandbox: 'false' },
    { timeoutMs: 0 },
    { shutdownTimeoutMs: -1 },
    { executablePath: '' },
    { executablePath: join(tmpdir(), 'eoka-nonexistent-chrome-lifecycle') },
    { proxy: { server: 'socks5://user:private@localhost:1080' } },
    { browserCacheDir: '' },
    { download: 'sometimes' },
    { onProgress: true },
  ]) {
    await assert.rejects(Browser.launch({ ...f.options, ...extra }), ConfigurationError);
  }
  for (const browserWSEndpoint of [
    'http://127.0.0.1/json/version',
    'wss://127.0.0.1/devtools/browser/id',
    'ws://127.0.0.1/devtools/page/id',
    'ws://127.0.0.1/devtools/browser/',
    'ws://127.0.0.1/devtools/browser/id/extra',
    'ws://user:private@127.0.0.1/devtools/browser/id',
    'ws://127.0.0.1/devtools/browser/id?',
    'ws://127.0.0.1/devtools/browser/id#',
  ]) {
    await assert.rejects(Browser.connect(connectOptions(f.options, { browserWSEndpoint })), ConfigurationError);
  }
  await assert.rejects(Browser.connect(connectOptions(f.options, { sandbox: false })), ConfigurationError);
  for (const option of ['runtimeDir', 'browserCacheDir', 'download', 'onProgress']) {
    await assert.rejects(Browser.connect(connectOptions(f.options, { [option]: 'unused' })), ConfigurationError);
  }
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(Browser.launch({ ...f.options, signal: controller.signal }));
  assert.deepEqual(await f.events(), []);
});

test('autoSolveCloudflare is sent to the server and needs its capability', { skip: !supported }, async t => {
  const f = await fixture(t);
  const browser = await Browser.launch({ ...f.options, autoSolveCloudflare: { timeoutMs: 5000 } });
  f.track(browser);
  const launch = await f.waitFor(event => event.event === 'request' && event.method === 'browser.launch');
  assert.deepEqual(launch.params.autoSolveCloudflare, { intervalMs: 500, timeoutMs: 5000 });
  const plain = await Browser.launch({ ...f.options, autoSolveCloudflare: false });
  f.track(plain);
  const launches = (await f.events()).filter(event => event.event === 'request' && event.method === 'browser.launch');
  assert.equal(Object.hasOwn(launches.at(-1).params, 'autoSolveCloudflare'), false);

  const old = await fixture(t, 'no-cloudflare');
  await assert.rejects(Browser.launch({ ...old.options, autoSolveCloudflare: true }), /cloudflareAutoSolve capability/);
  for (const value of [{ intervalMs: 99 }, { timeoutMs: 0 }, { onSolved: () => {} }]) {
    await assert.rejects(Browser.launch({ ...f.options, autoSolveCloudflare: value }), ConfigurationError);
  }
});

test('explicit paths override environment paths and launch defaults keep sandbox enabled', { skip: !supported }, async t => {
  const f = await fixture(t);
  const browser = await Browser.launch({
    ...f.options,
    serverPath: relative(process.cwd(), f.options.serverPath),
    executablePath: relative(process.cwd(), process.execPath),
    serverEnv: { ...f.options.serverEnv, EOKA_SERVER_BIN: '/missing/server', EOKA_CHROME_PATH: '/missing/chrome' },
  });
  f.track(browser);
  const launched = (await f.events()).find(event => event.method === 'browser.launch');
  assert.equal(launched.params.executablePath, process.execPath);
  assert.equal(launched.params.sandbox, true);
  assert.equal(launched.params.deviceScaleFactor, null);
  assert.equal(browser.ownership, 'owned');
  assert.equal(browser.browserVersion, 'Chrome/fixture');
});

test('environment explicit paths bypass managed installation while handshake support gates dispatch', { skip: !supported }, async t => {
  const f = await fixture(t);
  const browser = await Browser.launch({
    serverEnv: { ...f.options.serverEnv, EOKA_SERVER_BIN: f.options.serverPath, EOKA_CHROME_PATH: process.execPath },
    shutdownTimeoutMs: 2000,
    download: 'never',
    browserCacheDir: join(f.directory, 'unused-cache'),
  });
  f.track(browser);
  assert.equal(browser.hasCapability('lifecycleFixture'), true);
  assert.equal(browser.hasCapability('installation'), false);
  assert.equal(browser.supports('page.execute'), false);
  await assert.rejects(browser.invoke('page.execute', { pageId: 'existing-tab', js: '1' }));
  assert.equal((await f.events()).some(event => event.method === 'page.execute'), false);
  await assert.rejects(access(join(f.directory, 'unused-cache')), { code: 'ENOENT' });
});

test('invalid session evidence and failed startup await resource cleanup before rejecting', { skip: !supported }, async t => {
  for (const mode of ['bad-session', 'owned-connect', 'failed-session', 'no-connect']) {
    await t.test(mode, async child => {
      const f = await fixture(child, mode);
      const promise = mode === 'owned-connect' || mode === 'no-connect'
        ? Browser.connect(connectOptions(f.options))
        : Browser.launch(f.options);
      await assert.rejects(promise, error => mode === 'bad-session' || mode === 'owned-connect' ? error instanceof ProtocolError : true);
      const events = await f.events();
      assert.equal(events.filter(event => event.method === 'server.shutdown').length, 1);
      assert.equal(events.some(event => event.event === 'cleanup'), true);
      if (mode === 'no-connect') assert.equal(events.some(event => event.method === 'browser.connect'), false);
    });
  }
});

test('startup abort does not cancel bounded cleanup for either ownership path', { skip: !supported }, async t => {
  for (const connect of [false, true]) {
    await t.test(connect ? 'borrowed connect' : 'owned launch', async child => {
      const f = await fixture(child, 'pending-session');
      const controller = new AbortController();
      const promise = connect
        ? Browser.connect(connectOptions(f.options, { signal: controller.signal }))
        : Browser.launch({ ...f.options, signal: controller.signal });
      const rejected = assert.rejects(promise);
      await f.waitFor(event => event.method === (connect ? 'browser.connect' : 'browser.launch'));
      controller.abort(new Error('caller left'));
      await rejected;
      assert.deepEqual((await f.events()).filter(event => event.event === 'cleanup').map(event => event.ownership), [connect ? 'borrowed' : 'owned']);
    });
  }
});

test('startup preserves both primary failure and cleanup failure', { skip: !supported }, async t => {
  const f = await fixture(t, 'failed-session-cleanup');
  await assert.rejects(Browser.launch(f.options), error => {
    assert.ok(error instanceof AggregateError);
    assert.match(error.errors[0].message, /session initialization failed/);
    assert.match(error.errors[1].message, /shutdown cleanup failed/);
    assert.equal(error.cause, error.errors[0]);
    return true;
  });
});

test('one startup deadline spans hello and launch while timeout still awaits cleanup', { skip: !supported }, async t => {
  const f = await fixture(t, 'shared-deadline');
  await assert.rejects(Browser.launch({ ...f.options, timeoutMs: 1000 }), error => {
    assert.equal(error.code, 'DeadlineExceeded');
    return true;
  });
  const events = await f.events();
  assert.equal(events.some(event => event.method === 'browser.launch'), true);
  assert.equal(events.some(event => event.event === 'cleanup'), true);
});

test('launch isolates child temporary state and close preserves caller runtime root, cache and profile', { skip: !supported }, async t => {
  const f = await fixture(t);
  const cache = join(f.directory, 'cache');
  const profile = join(f.directory, 'profile');
  await mkdir(cache);
  await mkdir(profile);
  await writeFile(join(cache, 'keep'), 'cache');
  await writeFile(join(profile, 'keep'), 'profile');
  const before = { TMPDIR: process.env.TMPDIR, TMP: process.env.TMP, TEMP: process.env.TEMP };
  const browser = await Browser.launch({
    ...f.options,
    browserCacheDir: cache,
    download: 'never',
    userDataDir: relative(process.cwd(), profile),
    serverEnv: { ...f.options.serverEnv, TMPDIR: '/caller/tmpdir', TMP: '/caller/tmp', TEMP: '/caller/temp', EOKA_RUNTIME_DIR: '/ignored/runtime' },
  });
  f.track(browser);
  const events = await f.events();
  const launched = events.find(event => event.method === 'browser.launch');
  const owned = launched.params.runtimeDir;
  assert.equal(dirname(owned), f.directory);
  assert.notEqual(owned, f.directory);
  assert.ok(owned.split('/').at(-1).startsWith('e-'));
  assert.equal((await stat(owned)).mode & 0o777, 0o700);
  assert.equal(launched.params.userDataDir, profile);
  assert.deepEqual(events.find(event => event.event === 'spawn').temp, { TMPDIR: owned, TMP: owned, TEMP: owned });
  assert.deepEqual({ TMPDIR: process.env.TMPDIR, TMP: process.env.TMP, TEMP: process.env.TEMP }, before);
  await browser.close();
  assert.equal((await f.events()).find(event => event.event === 'exit').runtimeExists, true);
  await assert.rejects(access(owned), { code: 'ENOENT' });
  assert.equal(await readFile(join(cache, 'keep'), 'utf8'), 'cache');
  assert.equal(await readFile(join(profile, 'keep'), 'utf8'), 'profile');
  assert.ok((await stat(f.directory)).isDirectory());
});

test('runtime environment root creates distinct owned children for concurrent launches', { skip: !supported }, async t => {
  const f = await fixture(t);
  const root = join(f.directory, 'runtime-root');
  const { runtimeDir, ...options } = f.options;
  const launches = await Promise.all([0, 1].map(() => Browser.launch({ ...options, serverEnv: { ...options.serverEnv, EOKA_RUNTIME_DIR: root } })));
  launches.forEach(f.track);
  const children = (await f.events()).filter(event => event.method === 'browser.launch').map(event => event.params.runtimeDir);
  assert.equal(new Set(children).size, 2);
  for (const child of children) assert.equal(dirname(child), root);
  await launches[0].close();
  assert.equal((await readdir(root)).length, 1);
  await launches[1].close();
  assert.deepEqual(await readdir(root), []);
});

test('startup failure and cancellation remove only their own runtime after sidecar exit', { skip: !supported }, async t => {
  for (const mode of ['failed-session', 'pending-session']) {
    await t.test(mode, async child => {
      const f = await fixture(child, mode);
      const controller = new AbortController();
      const rejected = assert.rejects(Browser.launch({ ...f.options, signal: controller.signal }));
      const launched = await f.waitFor(event => event.method === 'browser.launch');
      if (mode === 'pending-session') controller.abort();
      await rejected;
      assert.equal((await f.events()).find(event => event.event === 'exit').runtimeExists, true);
      await assert.rejects(access(launched.params.runtimeDir), { code: 'ENOENT' });
      assert.ok((await stat(f.directory)).isDirectory());
    });
  }
});

test('connect ignores managed environment settings and does not allocate runtime or cache', { skip: !supported }, async t => {
  const f = await fixture(t);
  const untouched = join(f.directory, 'untouched');
  const browser = await Browser.connect(connectOptions(f.options, { serverEnv: { ...f.options.serverEnv, EOKA_BROWSER_CACHE_DIR: untouched, EOKA_RUNTIME_DIR: untouched, EOKA_CHROME_PATH: '/missing/chrome' } }));
  f.track(browser);
  assert.equal(browser.ownership, 'borrowed');
  assert.equal((await f.events()).some(event => event.method === 'browser.launch'), false);
  await browser.close();
  await assert.rejects(access(untouched), { code: 'ENOENT' });
});

test('raw terminal calls and spontaneous process exit release runtime without a separate close call', { skip: !supported }, async t => {
  for (const mode of ['raw-shutdown', 'raw-browser-close', 'spontaneous-exit']) {
    await t.test(mode, async child => {
      const f = await fixture(child, mode);
      const browser = await Browser.launch(f.options);
      f.track(browser);
      const owned = (await f.events()).find(event => event.method === 'browser.launch').params.runtimeDir;
      if (mode !== 'spontaneous-exit') await browser.invoke(mode === 'raw-shutdown' ? 'server.shutdown' : 'browser.close', {});
      await f.waitFor(event => event.event === 'exit');
      const deadline = performance.now() + 2000;
      while ((await readdir(f.directory)).includes(owned.split('/').at(-1)) && performance.now() < deadline) await delay(10);
      await assert.rejects(access(owned), { code: 'ENOENT' });
      await browser.close();
    });
  }
});

test('newPage stays pending after admission until the same goto path has verified navigation', { skip: !supported }, async t => {
  const f = await fixture(t);
  const browser = await Browser.launch(f.options);
  f.track(browser);
  let settled = false;
  const promise = browser.newPage('http://example.test/verified').then(page => { settled = true; return page; });
  await f.waitFor(event => event.method === 'operations.wait');
  assert.equal(settled, false);
  const page = await promise;
  assert.equal(page.id, 'created-tab');
  const events = await f.events();
  assert.deepEqual(events.find(event => event.method === 'browser.new_page').params, {});
  assert.equal(events.find(event => event.event === 'verified').outcome, 'verified_complete');
  const records = await browser.operations();
  assert.equal((await browser.operation(records[0].id).get()).receipt.outcome, 'verified_complete');
  assert.equal((await browser.pages())[0].id, 'existing-tab');
});

test('unknown navigation closes only the created tab and preserves the operation evidence', { skip: !supported }, async t => {
  const f = await fixture(t, 'unknown-navigation');
  const browser = await Browser.connect(connectOptions(f.options));
  f.track(browser);
  await assert.rejects(browser.newPage('http://example.test/unknown'), error => {
    assert.ok(error instanceof OperationError);
    assert.equal(error.receipt.outcome, 'outcome_unknown');
    assert.ok(error.operationId);
    return true;
  });
  const events = await f.events();
  assert.deepEqual(events.filter(event => event.event === 'tab-closed').map(event => event.pageId), ['created-tab']);
  assert.equal(events.some(event => event.method === 'server.shutdown'), false);
  assert.equal((await browser.tabs())[0].id, 'existing-tab');
});

test('navigation abort awaits uncanceled created-tab cleanup', { skip: !supported }, async t => {
  const f = await fixture(t, 'abort-navigation');
  const browser = await Browser.launch(f.options);
  f.track(browser);
  const controller = new AbortController();
  const rejected = assert.rejects(browser.newPage('http://example.test/abort', { signal: controller.signal }));
  await f.waitFor(event => event.method === 'operations.wait');
  controller.abort();
  await rejected;
  assert.equal((await f.events()).some(event => event.event === 'tab-closed'), true);
});

test('navigation and input cleanup failures preserve both pieces of evidence', { skip: !supported }, async t => {
  const f = await fixture(t, 'navigation-cleanup-failure');
  const browser = await Browser.launch(f.options);
  f.track(browser);
  await assert.rejects(browser.newPage('http://example.test/unknown'), error => {
    assert.ok(error instanceof AggregateError);
    assert.ok(error.errors[0] instanceof OperationError);
    assert.equal(error.errors[0].receipt.outcome, 'outcome_unknown');
    assert.equal(error.errors[1].data.cleanupError, 'input release failed');
    assert.equal(error.cause, error.errors[0]);
    return true;
  });
});

test('close and async disposal share teardown and settle active waits before cleanup completes', { skip: !supported }, async t => {
  for (const mode of ['pending-wait', 'borrowed-launch']) {
    await t.test(mode, async child => {
      const f = await fixture(child, mode);
      const browser = await Browser.launch(f.options);
      f.track(browser);
      const waiting = browser.invoke('operations.wait', { id: '00000000-0000-4000-8000-000000000001', timeout_ms: 60000 });
      const rejected = assert.rejects(waiting);
      await f.waitFor(event => event.method === 'operations.wait');
      const close = browser.close();
      assert.equal(browser.close(), close);
      assert.equal(browser[Symbol.asyncDispose](), close);
      await rejected;
      assert.equal((await f.events()).some(event => event.event === 'cleanup'), false);
      await close;
      const events = await f.events();
      assert.equal(events.filter(event => event.method === 'server.shutdown').length, 1);
      assert.deepEqual(events.filter(event => event.event === 'cleanup').map(event => event.ownership), [mode === 'borrowed-launch' ? 'borrowed' : 'owned']);
    });
  }
});

test('aborted and timed-out page creation owns the late page ID without closing borrowed existing tabs', { skip: !supported }, async t => {
  for (const cancellation of ['abort', 'timeout']) {
    await t.test(cancellation, async child => {
      const f = await fixture(child, 'delayed-creation');
      const browser = await Browser.connect(connectOptions(f.options));
      f.track(browser);
      const controller = new AbortController();
      const options = cancellation === 'abort' ? { signal: controller.signal } : { timeoutMs: 80 };
      const rejected = assert.rejects(browser.newPage(undefined, options), error => {
        assert.equal(error.code, cancellation === 'abort' ? 'Aborted' : 'DeadlineExceeded');
        assert.equal(error.requestState, 'may_have_been_sent');
        return true;
      });
      await f.waitFor(event => event.method === 'browser.new_page');
      if (cancellation === 'abort') controller.abort(new Error('page caller left'));
      await rejected;
      const events = await f.events();
      assert.deepEqual(events.filter(event => event.event === 'tab-closed').map(event => event.pageId), ['created-tab']);
      assert.equal(events.some(event => event.method === 'server.shutdown'), false);
      assert.equal((await browser.tabs())[0].id, 'existing-tab');
      assert.equal(browser.ownership, 'borrowed');
    });
  }
});

test('disposal rejects local creation but closes its late exact tab before borrowed disconnect', { skip: !supported }, async t => {
  const f = await fixture(t, 'delayed-creation');
  const browser = await Browser.connect(connectOptions(f.options));
  f.track(browser);
  const rejected = assert.rejects(browser.newPage());
  await f.waitFor(event => event.method === 'browser.new_page');
  const closed = browser.close();
  assert.equal(browser[Symbol.asyncDispose](), closed);
  await rejected;
  await closed;
  const events = await f.events();
  const tabClosed = events.findIndex(event => event.event === 'tab-closed');
  const shutdown = events.findIndex(event => event.method === 'server.shutdown');
  assert.ok(tabClosed >= 0 && shutdown > tabClosed);
  assert.deepEqual(events.filter(event => event.event === 'tab-closed').map(event => event.pageId), ['created-tab']);
  assert.equal(events.find(event => event.event === 'cleanup').ownership, 'borrowed');
});

test('missing native creation completion is bounded and disposal reports unknown-tab uncertainty', { skip: !supported }, async t => {
  const f = await fixture(t, 'pending-creation');
  const browser = await Browser.connect(connectOptions(f.options, { shutdownTimeoutMs: 350 }));
  const controller = new AbortController();
  const rejected = assert.rejects(browser.newPage(undefined, { signal: controller.signal }), error => {
    assert.ok(error instanceof AggregateError);
    assert.equal(error.cause.code, 'Aborted');
    assert.equal(error.errors[1].code, 'CleanupUncertain');
    return true;
  });
  await f.waitFor(event => event.method === 'browser.new_page');
  controller.abort();
  await rejected;
  await assert.rejects(browser.close(), AggregateError);
  const events = await f.events();
  assert.equal(events.some(event => event.event === 'cleanup'), true);
  assert.equal(events.some(event => event.method === 'browser.close_tab'), false);
});

test('raw shutdown and browser close share disposal without duplicate shutdown or expected-EOF failure', { skip: !supported }, async t => {
  for (const method of ['server.shutdown', 'browser.close']) {
    for (const borrowed of [false, true]) {
      await t.test(`${method}: ${borrowed ? 'borrowed' : 'owned'}`, async child => {
        const f = await fixture(child, 'reader-closed-first');
        const browser = borrowed ? await Browser.connect(connectOptions(f.options)) : await Browser.launch(f.options);
        f.track(browser);
        const native = browser.invoke(method, {});
        const closed = browser.close();
        assert.equal(browser[Symbol.asyncDispose](), closed);
        assert.deepEqual(await native, {});
        let disposed = false;
        const observedClose = closed.then(() => { disposed = true; });
        await f.waitFor(event => event.event === 'reader-closed');
        assert.equal(disposed, false);
        await observedClose;
        assert.equal(browser.close(), closed);
        await assert.rejects(browser.tabs());
        const events = await f.events();
        assert.deepEqual(events.filter(event => event.method === 'server.shutdown' || event.method === 'browser.close').map(event => event.method), [method]);
        assert.deepEqual(events.filter(event => event.event === 'cleanup').map(event => event.ownership), [borrowed ? 'borrowed' : 'owned']);
        assert.equal(events.some(event => event.event === 'exit-after-reader-close'), true);
        assert.equal(events.some(event => event.method === 'browser.tabs'), false);
      });
    }
  }
});

test('unconfirmed process termination retains owned runtime and reports its path on launch failure and close', { skip: !supported }, async t => {
  for (const mode of ['failed-session-stubborn', 'stubborn-cleanup']) {
    await t.test(mode, async child => {
      const f = await fixture(child, mode);
      child.mock.method(SidecarTransport.prototype, 'signalOwnedTree', async () => {
        throw Object.assign(new Error('simulated process termination permission denial'), { code: 'EPERM' });
      });
      let owned;
      let pid;
      const failure = error => {
        assert.ok(error instanceof AggregateError);
        const uncertainty = error.errors.find(item => item.code === 'RuntimeCleanupUncertain');
        assert.ok(uncertainty);
        assert.equal(uncertainty.data.runtimeDir, owned);
        assert.ok(uncertainty.message.includes(owned));
        if (mode === 'failed-session-stubborn') assert.match(error.errors[0].cause.message, /session initialization failed/);
        return true;
      };
      try {
        const launched = Browser.launch({ ...f.options, shutdownTimeoutMs: 15 });
        const rejection = mode === 'failed-session-stubborn' ? assert.rejects(launched, failure) : undefined;
        pid = (await f.waitFor(event => event.event === 'spawn')).pid;
        owned = (await f.waitFor(event => event.method === 'browser.launch')).params.runtimeDir;
        if (rejection) await rejection;
        else await assert.rejects((await launched).close(), failure);
        assert.equal(await readFile(join(owned, 'core-owned', 'scratch'), 'utf8'), 'owned scratch');
        assert.doesNotThrow(() => process.kill(pid, 0));
      } finally {
        if (pid === undefined) pid = (await f.events()).find(event => event.event === 'spawn')?.pid;
        if (pid !== undefined) {
          try { process.kill(-pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
          const deadline = performance.now() + 2000;
          while (performance.now() < deadline) {
            try { process.kill(pid, 0); } catch (error) { if (error.code === 'ESRCH') break; throw error; }
            await delay(10);
          }
        }
      }
    });
  }
});
