import type {
  ActionReceipt,
  MethodName,
  MethodParams,
  MethodResults,
  OperationRecord,
  ProtocolRequest,
  ProtocolResponse,
} from "../../src/generated/protocol.js";

declare function send<M extends MethodName>(method: M, params: MethodParams[M]): Promise<MethodResults[M]>;

const listRequest: ProtocolRequest<"operations.list"> = {
  id: 1,
  method: "operations.list",
  params: { limit: 10, before_ms: null },
};
const evaluationRequest: ProtocolRequest<"eval"> = {
  id: 2,
  method: "eval",
  params: { pageId: "page-1", code: "()=>({answer: 42})", function: true },
};
const waitRequest: ProtocolRequest<"operations.wait"> = {
  id: 3,
  method: "operations.wait",
  params: { id: "operation-1", timeout_ms: 5000 },
};
const actionRequest: ProtocolRequest<"actions.perform"> = {
  id: 4,
  method: "actions.perform",
  params: { pageId: "page-1", operation_id: "operation-2", action: { kind: "reload" } },
};
const operationArray: MethodResults["operations.list"] = [];
const evaluationObject: MethodResults["eval"] = { answer: 42 };
const evaluationNull: MethodResults["eval"] = null;
const evaluationScalar: MethodResults["eval"] = 42;
const evaluationArray: MethodResults["eval"] = [1, "two", null];
const listResponse: ProtocolResponse<"operations.list"> = { id: 1, result: operationArray };
const evaluationResponse: ProtocolResponse<"eval"> = { id: 2, result: evaluationObject };

async function consume() {
  const operations: OperationRecord[] = await send("operations.list", listRequest.params);
  const performed: OperationRecord = await send("actions.perform", actionRequest.params);
  const dynamic: unknown = await send("eval", evaluationRequest.params);
  const receipts: Array<ActionReceipt | null | undefined> = operations.map((record) => record.receipt);
  const receiptOperation = await send("operations.wait", waitRequest.params);
  const receipt: ActionReceipt | null | undefined = receiptOperation.receipt;
  return { dynamic, receipts, receipt, performed };
}

async function managedSession() {
  const launched = await send("browser.launch", {
    headless: true,
    executablePath: "/opt/chrome/chrome",
    runtimeDir: "/tmp/private-runtime",
    sandbox: true,
    viewportWidth: 640,
    viewportHeight: 360,
    deviceScaleFactor: 2,
  });
  const attached = await send("browser.connect", { wsEndpoint: "ws://127.0.0.1:9222/devtools/browser/id" });
  const ownership: "owned" | "borrowed" = launched.ownership;
  const version: string = attached.browserVersion;
  await send("emulate", { pageId: "page-1", width: 640, height: 360, dpr: 2, desktop: true });
  await send("emulate", { pageId: "page-1", reset: true });
  return { ownership, version };
}

export {
  consume,
  managedSession,
  evaluationArray,
  evaluationNull,
  evaluationScalar,
  listResponse,
  evaluationResponse,
};
