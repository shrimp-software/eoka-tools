import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
import { generateProtocol, schemaDeclarations } from "../scripts/schema-to-typescript.mjs";

function diagnostics(source) {
  const filename = "/eoka-schema-consumer.ts";
  const options = {
    strict: true,
    exactOptionalPropertyTypes: true,
    noEmit: true,
    types: [],
    lib: ["lib.es2024.d.ts"],
    target: ts.ScriptTarget.ES2024,
    module: ts.ModuleKind.ESNext,
  };
  const host = ts.createCompilerHost(options);
  const getSourceFile = host.getSourceFile.bind(host);
  host.getSourceFile = (name, ...args) => name === filename
    ? ts.createSourceFile(filename, source, options.target, true)
    : getSourceFile(name, ...args);
  const program = ts.createProgram([filename], options, host);
  return ts.getPreEmitDiagnostics(program).map((diagnostic) => ({
    code: diagnostic.code,
    message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
    file: diagnostic.file?.fileName,
    start: diagnostic.start,
  }));
}

const inputSchema = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  type: "object",
  additionalProperties: false,
  required: ["name", "values", "state", "metadata", "dynamic", "node"],
  properties: {
    name: { type: "string" },
    optional: { type: ["string", "null"] },
    values: { type: "array", items: { anyOf: [{ type: "integer" }, { type: "null" }] } },
    state: { oneOf: [{ const: "running" }, { const: "finished" }] },
    metadata: { type: "object", additionalProperties: { type: "boolean" } },
    dynamic: true,
    node: { $ref: "#/$defs/Node" },
  },
  $defs: {
    Node: {
      type: "object",
      required: ["value"],
      additionalProperties: false,
      properties: {
        value: { type: "integer" },
        child: { anyOf: [{ $ref: "#/$defs/Node" }, { type: "null" }] },
      },
    },
  },
};
const declaration = schemaDeclarations(inputSchema, "Input").join("\n");
const valid = `const input: Input = {
  name: "example", optional: null, values: [1, null], state: "running",
  metadata: { enabled: true }, dynamic: { arbitrary: [1, null] },
  node: { value: 1, child: { value: 2, child: null } }
};`;

test("conversion preserves required, optional, nullable, unions, records and recursive references", () => {
  assert.deepEqual(diagnostics(`${declaration}\n${valid}`), []);
  const rejected = [
    "const bad: Input = {};",
    "const bad: Input['name'] = null;",
    "const bad: Input['optional'] = 1;",
    "const bad: Input['values'] = ['invalid'];",
    "const bad: Input['state'] = 'pending';",
    "const bad: Input['metadata'] = { enabled: 'yes' };",
    "const bad: Input['node'] = { value: 'one' };",
    "declare const input: Input; const bad: string = input.dynamic;",
  ];
  for (const source of rejected) {
    const errors = diagnostics(`${declaration}\n${source}`);
    assert.ok(errors.some((error) => [2322, 2739, 2740, 2741].includes(error.code)), source);
  }
});

test("boolean schemas, intersections, escaped references and tuple items retain their types", () => {
  const declarations = [
    ...schemaDeclarations(false, "Impossible"),
    ...schemaDeclarations({ allOf: [
      { type: "object", properties: { a: { type: "string" } }, required: ["a"] },
      { type: "object", properties: { b: { type: "number" } }, required: ["b"] },
    ] }, "Intersection"),
    ...schemaDeclarations({ type: "array", prefixItems: [{ type: "string" }, { type: "integer" }], minItems: 1, items: false }, "Tuple"),
    ...schemaDeclarations({ $ref: "#/$defs/slash~1tilde~0", $defs: { "slash/tilde~": { type: "boolean" } } }, "Escaped"),
  ].join("\n");
  assert.deepEqual(diagnostics(`${declarations}\nconst a: Intersection = {a: 'x', b: 1}; const b: Tuple = ['x']; const c: Tuple = ['x', 2]; const d: Escaped = true;`), []);
  assert.ok(diagnostics(`${declarations}\nconst a: Tuple = ['x', 'y'];`).some((error) => error.code === 2322));
  assert.ok(diagnostics(`${declarations}\nconst a: Impossible = null;`).some((error) => error.code === 2322));
});

test("unsupported semantics and unresolved references fail explicitly", () => {
  for (const schema of [
    { type: "object", patternProperties: { "^a": { type: "string" } } },
    { not: { type: "null" } },
    { if: { const: 1 }, then: { const: 2 } },
    { $ref: "https://example.invalid/schema" },
    { $ref: "#/$defs/Missing" },
    { type: "object", properties: { named: { type: "string" } }, additionalProperties: { type: "integer" } },
  ]) assert.throws(() => schemaDeclarations(schema, "Invalid"), /unsupported|unresolved|cannot be represented/i);
});

test("method maps correlate requests and concrete array and dynamic evaluation results", () => {
  const bundle = {
    protocolVersion: { major: 1, minor: 0 },
    types: { ServerError: { type: "object", required: ["message"], properties: { message: { type: "string" } } } },
    methods: {
      "operations.list": { params: { type: "object", properties: { limit: { type: "integer" } } }, result: { type: "array", items: { type: "object", required: ["id"], properties: { id: { type: "string" } } } } },
      "page.evaluate": { params: { type: "object", required: ["js"], properties: { js: { type: "string" } } }, result: { type: "object", required: ["result"], properties: { result: true } } },
    },
  };
  const generated = generateProtocol(bundle);
  const reordered = { ...bundle, methods: Object.fromEntries(Object.entries(bundle.methods).reverse()) };
  assert.equal(generateProtocol(reordered), generated);
  assert.deepEqual(diagnostics(`${generated}\nconst request: ProtocolRequest = {id: 1, method: 'page.evaluate', params: {js: '42'}}; const response: ProtocolResponse<'operations.list'> = {id: 1, result: [{id: 'op'}]}; const result: MethodResults['page.evaluate'] = {result: null};`), []);
  for (const source of [
    "const bad: ProtocolRequest = {id: 1, method: 'page.evaluate', params: {js: 42}};",
    "const bad: MethodResults['operations.list'] = {};",
    "const bad: ProtocolResponse<'operations.list'> = {id: 1, result: [], error: {message: 'no'}};",
    "declare const result: MethodResults['page.evaluate']; const bad: string = result.result;",
  ]) assert.ok(diagnostics(`${generated}\n${source}`).length > 0, source);
});
