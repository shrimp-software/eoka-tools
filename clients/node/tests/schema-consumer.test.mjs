import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import Ajv2020 from "ajv/dist/2020.js";
import { decodeMethodResult } from "../scripts/schema-consumer.mjs";

const bundle = JSON.parse(await readFile(new URL("../src/generated/schema-bundle.json", import.meta.url), "utf8"));
const operationsWire = await readFile(new URL("../fixtures/wire/operations-list.json", import.meta.url), "utf8");
const evaluationWire = await readFile(new URL("../fixtures/wire/evaluation.json", import.meta.url), "utf8");

test("canonical operations array decodes with its durable operation and action receipt", () => {
  const operations = decodeMethodResult(bundle, "operations.list", operationsWire);
  assert.ok(Array.isArray(operations));
  assert.equal(operations[0].id, "operation-1");
  assert.equal(operations[0].receipt.outcome, "verified_complete");
  assert.equal(operations[0].receipt.dispatch, "acknowledged");
  assert.throws(() => decodeMethodResult(bundle, "operations.list", JSON.stringify({ id: 1, result: {} })), /Invalid operations.list result/);
  const invalidReceipt = JSON.parse(operationsWire);
  invalidReceipt.result[0].receipt.outcome = "success_without_evidence";
  assert.throws(() => decodeMethodResult(bundle, "operations.list", JSON.stringify(invalidReceipt)), /Invalid operations.list result/);
  const unsafe = JSON.parse(operationsWire);
  unsafe.result[0].started_at_ms = Number.MAX_SAFE_INTEGER + 1;
  assert.throws(() => decodeMethodResult(bundle, "operations.list", JSON.stringify(unsafe)), /Invalid operations.list result/);
});

test("canonical evaluation preserves dynamic JSON without manufacturing an object result", () => {
  const evaluation = decodeMethodResult(bundle, "page.evaluate", evaluationWire);
  assert.deepEqual(evaluation.result, { answer: 42, items: [1, "two", null, { nested: true }] });
  for (const value of [null, true, 42, "text", [1, null], { nested: { arbitrary: true } }]) {
    const decoded = decodeMethodResult(bundle, "page.evaluate", JSON.stringify({ id: 2, result: { result: value } }));
    assert.deepEqual(decoded.result, value);
  }
  assert.throws(() => decodeMethodResult(bundle, "page.evaluate", JSON.stringify({ id: 2, result: 42 })), /Invalid page.evaluate result/);
});

test("the consumer refuses unsafe IDs, error/success ambiguity and unavailable methods", () => {
  assert.throws(() => decodeMethodResult(bundle, "operations.list", JSON.stringify({ id: Number.MAX_SAFE_INTEGER + 1, result: [] })), /safe positive request ID/);
  assert.throws(() => decodeMethodResult(bundle, "operations.list", JSON.stringify({ id: 1, result: [], error: { code: "Internal", message: "failed" } })), /successful sidecar response/);
  assert.throws(() => decodeMethodResult(bundle, "browser.not_a_method", "{}"), /Unknown sidecar method/);
});

test("canonical method schemas reject invalid arguments and preserve null optionals", () => {
  const evaluate = new Ajv2020({ strict: false, validateFormats: false }).compile(bundle.methods["page.evaluate"].params);
  assert.equal(evaluate({ pageId: "page-1", js: "42" }), true);
  assert.equal(evaluate({ pageId: "page-1" }), false);
  assert.equal(evaluate({ pageId: 7, js: "42" }), false);
  const list = new Ajv2020({ strict: false, validateFormats: false }).compile(bundle.methods["operations.list"].params);
  assert.equal(list({ limit: 10, before_ms: null }), true);
  assert.equal(list({ limit: -1 }), false);
  assert.equal(list({ before_ms: Number.MAX_SAFE_INTEGER + 1 }), false);
});

test("browser session metadata requires truthful ownership and version fields", () => {
  for (const method of ["browser.launch", "browser.connect"]) {
    for (const ownership of ["owned", "borrowed"]) {
      const result = { ownership, browserVersion: "Chrome/155.0.8059.39" };
      assert.deepEqual(decodeMethodResult(bundle, method, JSON.stringify({ id: 3, result })), result);
    }
    for (const result of [{}, { ownership: "owned" }, { ownership: "unknown", browserVersion: "Chrome/155" }]) {
      assert.throws(() => decodeMethodResult(bundle, method, JSON.stringify({ id: 3, result })), /Invalid browser/);
    }
  }
});

test("viewport schemas reject invalid CSS dimensions and device scale factors", () => {
  const viewport = new Ajv2020({ strict: false, validateFormats: false }).compile(bundle.methods["page.set_viewport"].params);
  const valid = { pageId: "page-1", width: 640, height: 360, deviceScaleFactor: 2 };
  assert.equal(viewport(valid), true);
  for (const invalid of [
    { ...valid, width: 0 },
    { ...valid, height: -1 },
    { ...valid, width: 2147483648 },
    { ...valid, deviceScaleFactor: 0 },
    { ...valid, deviceScaleFactor: -1 },
    { ...valid, deviceScaleFactor: "2" },
  ]) assert.equal(viewport(invalid), false);
});
