import { readFile } from "node:fs/promises";
import { decodeMethodResult } from "../scripts/schema-consumer.mjs";

const bundle = JSON.parse(await readFile(new URL("../src/generated/schema-bundle.json", import.meta.url), "utf8"));
const operations = decodeMethodResult(bundle, "operations.list", await readFile(new URL("../fixtures/wire/operations-list.json", import.meta.url), "utf8"));
const evaluation = decodeMethodResult(bundle, "page.evaluate", await readFile(new URL("../fixtures/wire/evaluation.json", import.meta.url), "utf8"));
console.log(JSON.stringify({ operations: operations.map((record) => record.id), evaluation: evaluation.result }, null, 2));
