import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Page } from '../dist/page.js';
import { Operation, OperationError } from '../dist/operations.js';
import { ProtocolError, ServerOperationError, TransportError } from '../dist/errors.js';

const id = 'bb4bb8d9-5e7b-4f21-8c3e-50d0722272b1';
const target = { query: { kind: 'css', selector: '#button' } };
const expect = { kind: 'visible', target: { query: { kind: 'css', selector: '#done' } } };

function receipt(overrides = {}) {
  return {
    action: 'click', already_satisfied: false, cleanup_pending: false,
    dispatch: 'acknowledged', elapsed_ms: 10, finished_at_ms: 20,
    outcome: 'verified_complete', preparation_may_have_executed: false,
    started_at_ms: 10, target_id: 'tab', verification: 'visible', ...overrides,
  };
}

function record(overrides = {}) {
  return {
    id, action: 'click', dispatch_intent: false, preparation_intent: false,
    runtime_id: 'runtime', started_at_ms: 10, state: 'running',
    outcome: 'not_dispatched', target_id: 'tab', ...overrides,
  };
}

function completed(overrides = {}) {
  return record({ state: 'finished', outcome: 'verified_complete', receipt: receipt(), ...overrides });
}

for (const [name, args] of [
  ['click', ['#button']], ['type', ['#input', 'text']], ['pressKey', ['Enter']],
  ['submit', ['#form']], ['drag', ['#item', 10, 20]],
]) {
  test(`${name} refuses missing expectations before any request`, async () => {
    let sent = 0;
    const page = new Page({ invoke: async () => { sent++; } }, 'tab');
    for (const actionOptions of [undefined, {}, { expect: null }]) {
      await assert.rejects(page[name](...args, { operationId: id, actionOptions }), (error) => {
        assert.ok(error instanceof OperationError);
        assert.equal(error.code, 'expectation_required');
        assert.equal(error.operationId, id);
        assert.equal(error.requestState, 'not_sent');
        assert.equal(error.receipt, undefined);
        return true;
      });
    }
    assert.equal(sent, 0);
  });
}

test('perform creates a caller-recoverable UUID before dispatch and preserves lost-response metadata', async () => {
  let dispatchedId;
  const latest = completed();
  const calls = [];
  const lost = new TransportError('EOF', 'lost admission response', { requestId: 7, requestState: 'may_have_been_sent', stderr: 'peer exited' });
  const page = new Page({ invoke: async (method, params, options) => {
    calls.push(method);
    if (method === 'actions.perform') {
      dispatchedId = params.operation_id;
      assert.match(dispatchedId, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
      assert.equal(options.operationId, dispatchedId);
      throw lost;
    }
    assert.equal(params.id, dispatchedId);
    return { ...latest, id: dispatchedId };
  } }, 'tab');
  let failure;
  await assert.rejects(page.perform({ kind: 'fill', target, value: 'value' }), (error) => {
    failure = error;
    assert.equal(error.operationId, dispatchedId);
    assert.equal(error.code, 'EOF');
    assert.equal(error.requestId, 7);
    assert.equal(error.requestState, 'may_have_been_sent');
    assert.equal(error.stderr, 'peer exited');
    assert.equal(error.cause, lost);
    return true;
  });
  const recovered = await page.operation(failure.operationId).get();
  assert.equal(recovered.outcome, 'verified_complete');
  assert.deepEqual(calls, ['actions.perform', 'operations.get']);
});

test('perform returns admission only and Operation.wait exposes the latest running record without polling', async () => {
  const admitted = record();
  const stillRunning = record({ dispatch_intent: true, outcome: 'outcome_unknown' });
  const calls = [];
  const page = new Page({ invoke: async (method, params, options) => {
    calls.push({ method, params, options });
    return method === 'actions.perform' ? admitted : stillRunning;
  } }, 'tab');
  const operation = await page.perform({ kind: 'click', target }, { operationId: id, actionOptions: { expect } });
  assert.ok(operation instanceof Operation);
  assert.equal(operation.record, admitted);
  assert.equal(await operation.wait({ waitTimeoutMs: 4, timeoutMs: 50 }), stillRunning);
  assert.equal(operation.record, stillRunning);
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[1].params, { id, timeout_ms: 4 });
  assert.equal(calls[1].options.timeoutMs, 50);
});

test('a convenience cannot resolve merely because the action was admitted', { timeout: 1_000 }, async () => {
  const waiter = Promise.withResolvers();
  const waitStarted = Promise.withResolvers();
  let settled = false;
  const terminal = completed();
  const calls = [];
  const page = new Page({ invoke: async (method) => {
    calls.push(method);
    if (method === 'actions.perform') return record();
    waitStarted.resolve();
    return waiter.promise;
  } }, 'tab');
  const action = page.click('#button', { operationId: id, actionOptions: { expect } });
  action.then(() => { settled = true; });
  await waitStarted.promise;
  assert.equal(settled, false);
  waiter.resolve(terminal);
  assert.equal(await action, terminal.receipt);
  assert.deepEqual(calls, ['actions.perform', 'operations.wait']);
});

for (const latest of [
  record({ state: 'running', outcome: 'outcome_unknown' }),
  record({ state: 'finished', outcome: 'outcome_unknown', error_code: 'verification_timeout', receipt: receipt({ outcome: 'outcome_unknown' }) }),
  record({ state: 'interrupted', outcome: 'outcome_unknown', receipt: receipt({ outcome: 'outcome_unknown' }) }),
  record({ state: 'expired', outcome: 'not_dispatched' }),
  completed({ receipt: undefined }),
  completed({ receipt: receipt({ outcome: 'not_dispatched', dispatch: 'not_sent' }) }),
  completed({ error_code: 'cleanup_failed' }),
]) {
  test(`strict action rejects ${latest.state}/${latest.outcome}/${latest.receipt?.outcome ?? 'no receipt'}/${latest.error_code ?? 'no error'} without replay`, async () => {
    const calls = [];
    const page = new Page({ invoke: async (method) => {
      calls.push(method);
      return method === 'actions.perform' ? record() : latest;
    } }, 'tab');
    await assert.rejects(page.click('#button', { operationId: id, actionOptions: { expect } }), (error) => {
      assert.ok(error instanceof OperationError);
      assert.equal(error.operationId, id);
      assert.equal(error.record, latest);
      assert.equal(error.receipt, latest.receipt);
      assert.equal(error.requestState, 'acknowledged');
      return true;
    });
    assert.deepEqual(calls, ['actions.perform', 'operations.wait']);
  });
}

test('verified admission may resolve directly, but needs the receipt and finished record together', async () => {
  const latest = completed();
  let requests = 0;
  const page = new Page({ invoke: async () => { requests++; return latest; } }, 'tab');
  assert.equal(await page.goto('https://example.test', { operationId: id }), latest.receipt);
  assert.equal(requests, 1);
});

test('wire errors retain canonical code, data, receipt, and their original error', async () => {
  const unknownReceipt = receipt({ outcome: 'outcome_unknown' });
  const data = { operation_id: id, receipt: unknownReceipt, detail: { code: 'expectation_timeout', message: 'not observed' } };
  const source = new ServerOperationError({ code: 'Action', message: 'verification failed', data }, { requestId: 9, requestState: 'acknowledged' });
  const page = new Page({ invoke: async () => { throw source; } }, 'tab');
  await assert.rejects(page.click('#button', { operationId: id, actionOptions: { expect } }), (error) => {
    assert.equal(error.code, 'Action');
    assert.equal(error.message, 'verification failed');
    assert.equal(error.data, data);
    assert.equal(error.receipt, unknownReceipt);
    assert.equal(error.operationId, id);
    assert.equal(error.requestId, 9);
    assert.equal(error.requestState, 'acknowledged');
    assert.equal(error.cause, source);
    return true;
  });
});

for (const requestState of ['not_sent', 'may_have_been_sent']) {
  test(`local abort preserves ${requestState} and recovers by ID without performing again`, async () => {
    const controller = new AbortController();
    const dispatched = Promise.withResolvers();
    const calls = [];
    const page = new Page({ invoke: (method, params, options) => {
      calls.push(method);
      if (method === 'operations.get') return Promise.resolve(completed());
      assert.equal(params.operation_id, id);
      assert.equal(options.signal, controller.signal);
      dispatched.resolve();
      return new Promise((resolve, reject) => {
        options.signal.addEventListener('abort', () => reject(new TransportError('Aborted', 'local caller aborted', { requestState })), { once: true });
      });
    } }, 'tab');
    const action = page.click('#button', { operationId: id, signal: controller.signal, actionOptions: { expect } });
    await dispatched.promise;
    controller.abort();
    await assert.rejects(action, (error) => {
      assert.equal(error.operationId, id);
      assert.equal(error.requestState, requestState);
      assert.equal(error.receipt, undefined);
      return true;
    });
    const recovered = await page.operation(id).get();
    assert.equal(recovered.id, id);
    assert.deepEqual(calls, ['actions.perform', 'operations.get']);
  });
}

test('a lost wait response preserves the admission record and supports later recovery', async () => {
  const admitted = record({ dispatch_intent: true, outcome: 'outcome_unknown' });
  let waits = 0;
  const operation = new Operation({ invoke: async (method) => {
    if (method === 'operations.wait') {
      waits++;
      throw new TransportError('Timeout', 'local wait timed out', { requestState: 'may_have_been_sent', requestId: 3 });
    }
    return completed();
  } }, id, admitted);
  await assert.rejects(operation.wait({ waitTimeoutMs: 100, timeoutMs: 5 }), (error) => {
    assert.equal(error.operationId, id);
    assert.equal(error.record, admitted);
    assert.equal(error.requestId, 3);
    return true;
  });
  assert.equal(operation.record, admitted);
  assert.equal((await operation.get()).outcome, 'verified_complete');
  assert.equal(waits, 1);
});

test('strict actions share one deadline across admission and wait', async (t) => {
  let now = 100;
  t.mock.method(performance, 'now', () => now);
  const calls = [];
  const page = new Page({ invoke: async (method, params, options) => {
    calls.push({ method, params, options });
    if (method === 'actions.perform') {
      assert.equal(options.timeoutMs, 100);
      now = 160;
      return record();
    }
    assert.equal(options.timeoutMs, 40);
    assert.equal(params.timeout_ms, 40);
    now = 180;
    return completed();
  } }, 'tab');
  assert.equal((await page.click('#button', { operationId: id, timeoutMs: 100, actionOptions: { expect } })).outcome, 'verified_complete');
  assert.equal(calls.length, 2);
});

test('an exhausted admission deadline does not start a fresh wait deadline', async (t) => {
  let now = 100;
  t.mock.method(performance, 'now', () => now);
  let calls = 0;
  const admitted = record();
  const page = new Page({ invoke: async () => { calls++; now = 201; return admitted; } }, 'tab');
  await assert.rejects(page.fill('#input', 'value', { operationId: id, timeoutMs: 100 }), (error) => {
    assert.equal(error.code, 'DeadlineExceeded');
    assert.equal(error.operationId, id);
    assert.equal(error.record, admitted);
    assert.equal(error.requestState, 'acknowledged');
    return true;
  });
  assert.equal(calls, 1);
});

test('a verified record arriving past the local deadline is recoverable, not local success', async (t) => {
  let now = 100;
  t.mock.method(performance, 'now', () => now);
  const latest = completed();
  const page = new Page({ invoke: async (method) => {
    if (method === 'actions.perform') return record();
    now = 201;
    return latest;
  } }, 'tab');
  await assert.rejects(page.fill('#input', 'value', { operationId: id, timeoutMs: 100 }), (error) => {
    assert.equal(error.code, 'DeadlineExceeded');
    assert.equal(error.operationId, id);
    assert.equal(error.record, latest);
    assert.equal(error.receipt, latest.receipt);
    return true;
  });
});

test('canonical locator identity and intrinsic conveniences remain unchanged', async () => {
  const locator = { query: { kind: 'role', role: 'button' }, frame_id: 'frame', index: 2 };
  const actions = [];
  const page = new Page({ invoke: async (method, params) => {
    assert.equal(method, 'actions.perform');
    actions.push(params.action);
    return completed({ id: params.operation_id });
  } }, 'tab');
  await page.fill(locator, 'text');
  await page.select('#select', ['a', 'b']);
  await page.hover(locator);
  assert.equal(actions[0].target, locator);
  assert.deepEqual(actions[1], { kind: 'select', target: { query: { kind: 'css', selector: '#select' } }, values: ['a', 'b'] });
  assert.equal(actions[2].target, locator);
});

test('cancel returns its actual record, which can still be running and unknown', async () => {
  const latest = record({ dispatch_intent: true, outcome: 'outcome_unknown' });
  const operation = new Operation({ invoke: async (method, params, options) => {
    assert.equal(method, 'operations.cancel');
    assert.deepEqual(params, { id });
    assert.equal(options.operationId, id);
    return latest;
  } }, id);
  assert.equal(await operation.cancel(), latest);
  assert.equal(operation.record, latest);
});

test('agent step fixes a durable operation ID before dispatch and never replays lost responses', async () => {
  let operationId;
  const calls = [];
  const page = new Page({ invoke: async (method, params, options) => {
    calls.push(method);
    if (method === 'agent.step') {
      operationId = params.operation_id;
      assert.equal(options.operationId, operationId);
      assert.equal(params.mode, 'explore');
      throw new TransportError('EOF', 'lost agent response', { requestState: 'may_have_been_sent' });
    }
    return { ...completed(), id: params.id };
  } }, 'tab');
  await assert.rejects(page.step({ kind: 'click', target: '@control' }, { mode: 'explore' }), (error) => {
    assert.equal(error.operationId, operationId);
    assert.equal(error.code, 'EOF');
    return true;
  });
  assert.equal((await page.operation(operationId).get()).id, operationId);
  assert.deepEqual(calls, ['agent.step', 'operations.get']);
});

test('evaluation preserves dynamic JSON and propagates canonical non-JSON errors', async () => {
  let value;
  const unsupported = new ProtocolError('result is not JSON');
  const page = new Page({ invoke: async (method, params) => {
    assert.equal(method, 'page.evaluate');
    assert.equal(params.pageId, 'tab');
    if (params.js === 'unsupported') throw unsupported;
    return { result: value };
  } }, 'tab');
  for (value of [null, false, 42, 'text', [1, null], { nested: { value: true } }]) {
    assert.equal(await page.evaluate('expression'), value);
  }
  await assert.rejects(page.evaluate('unsupported'), (error) => error === unsupported);
});

test('read wrappers decode canonical state and PNG bytes while raw invoke remains typed escape hatch', async () => {
  const state = { url: 'https://example.test', userAgent: 'test', cookies: [], localStorage: { key: 'value' } };
  const png = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const page = new Page({ supports: () => true, hasCapability: name => name === 'screenshotOptions', invoke: async (method) => {
    if (method === 'page.capture_state') return { state };
    if (method === 'page.screenshot') return { dataBase64: png.toString('base64') };
    if (method === 'page.get_attribute') return { value: null };
    if (method === 'operations.list') return [completed()];
    throw new Error(`unexpected method ${method}`);
  } }, 'tab');
  assert.equal(await page.captureState(), state);
  assert.deepEqual(await page.screenshot(), png);
  assert.equal(await page.getAttribute('#missing', 'attribute'), null);
  assert.equal((await page.invoke('operations.list', {}))[0].id, id);
});

test('page waits keep the server wait timeout distinct from local cancellation and deadlines', async () => {
  const controller = new AbortController();
  const observed = [];
  const page = new Page({ invoke: async (method, params, options) => {
    observed.push({ method, params, options });
    return {};
  } }, 'tab');
  for (const [method, value] of [['waitFor', '#target'], ['waitForVisible', '#target'], ['waitForText', 'ready']]) {
    await page[method](value, { waitTimeoutMs: 25, timeoutMs: 10, signal: controller.signal });
  }
  for (const call of observed) {
    assert.equal(call.params.timeoutMs, 25);
    assert.equal(call.options.timeoutMs, 10);
    assert.equal(call.options.signal, controller.signal);
    assert.equal(Object.hasOwn(call.params, 'signal'), false);
    assert.equal(Object.hasOwn(call.options, 'waitTimeoutMs'), false);
  }
});

for (const method of ['get', 'wait', 'cancel']) {
  for (const received of [
    completed({ id: 'f46c92d0-87dd-4516-85ee-235c19cfe1ef' }),
    completed({ target_id: 'other-tab', receipt: receipt({ target_id: 'other-tab' }) }),
    completed({ receipt: receipt({ target_id: 'other-tab' }) }),
  ]) {
    test(`${method} rejects wrong identity ${received.id}/${received.target_id}/${received.receipt.target_id} and retains the received evidence`, async () => {
      const admitted = record();
      const operation = new Operation({ invoke: async () => received }, id, admitted);
      await assert.rejects(operation[method](), (error) => {
        assert.ok(error instanceof OperationError);
        assert.equal(error.code, 'Protocol');
        assert.equal(error.operationId, id);
        assert.equal(error.record, received);
        assert.equal(error.receipt, received.receipt);
        assert.equal(error.requestState, 'acknowledged');
        return true;
      });
      assert.equal(operation.id, id);
      assert.equal(operation.record, admitted);
    });
  }
}

test('Operation construction refuses an initial record belonging to another operation', () => {
  const received = completed({ id: 'f46c92d0-87dd-4516-85ee-235c19cfe1ef' });
  assert.throws(() => new Operation({ invoke: async () => {} }, id, received), (error) => {
    assert.equal(error.code, 'Protocol');
    assert.equal(error.operationId, id);
    assert.equal(error.record, received);
    assert.equal(error.receipt, received.receipt);
    assert.equal(error.requestState, 'acknowledged');
    return true;
  });
});

for (const stage of ['admission', 'wait']) {
  for (const received of [
    completed({ id: 'f46c92d0-87dd-4516-85ee-235c19cfe1ef' }),
    completed({ target_id: 'other-tab', receipt: receipt({ target_id: 'other-tab' }) }),
    completed({ receipt: receipt({ target_id: 'other-tab' }) }),
  ]) {
    test(`strict action rejects ${stage} record with wrong identity ${received.id}/${received.target_id}/${received.receipt.target_id}`, async () => {
      const calls = [];
      const page = new Page({ invoke: async (method) => {
        calls.push(method);
        return stage === 'admission' || method === 'operations.wait' ? received : record();
      } }, 'tab');
      await assert.rejects(page.click('#button', { operationId: id, actionOptions: { expect } }), (error) => {
        assert.equal(error.code, 'Protocol');
        assert.equal(error.operationId, id);
        assert.equal(error.record, received);
        assert.equal(error.receipt, received.receipt);
        assert.equal(error.requestState, 'acknowledged');
        return true;
      });
      assert.deepEqual(calls, stage === 'admission' ? ['actions.perform'] : ['actions.perform', 'operations.wait']);
    });
  }
}

test('legitimate already-satisfied verified receipts need no dispatched mutation', async () => {
  const latest = completed({ receipt: receipt({ already_satisfied: true, dispatch: 'not_sent' }) });
  const page = new Page({ invoke: async () => latest }, 'tab');
  assert.equal(await page.fill('#input', 'value', { operationId: id }), latest.receipt);
  assert.equal((await new Operation({ invoke: async () => latest }, id).get()).receipt.dispatch, 'not_sent');
});

for (const received of [
  { operation_id: 'f46c92d0-87dd-4516-85ee-235c19cfe1ef', operation: completed() },
  { operation_id: id, operation: completed({ id: 'f46c92d0-87dd-4516-85ee-235c19cfe1ef' }) },
  { operation_id: id, operation: completed({ target_id: 'other-tab', receipt: receipt({ target_id: 'other-tab' }) }) },
]) {
  test(`agent step rejects foreign operation identity ${received.operation_id}/${received.operation.id}/${received.operation.target_id}`, async () => {
    const page = new Page({ invoke: async () => received }, 'tab');
    await assert.rejects(page.step({ kind: 'click', target: '@control' }, { operationId: id, mode: 'explore' }), (error) => {
      assert.equal(error.code, 'Protocol');
      assert.equal(error.operationId, id);
      assert.equal(error.record, received.operation);
      assert.equal(error.receipt, received.operation.receipt);
      assert.equal(error.requestState, 'acknowledged');
      return true;
    });
  });
}
