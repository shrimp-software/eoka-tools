import {
  Browser,
  ConfigurationError,
  EokaError,
  InstallationError,
  Operation,
  OperationError,
  Page,
  Recording,
  RecordingError,
  SidecarTransport,
  methodNames,
  protocolVersion,
  installBrowser,
  releaseManifest,
  selectPlatform,
} from "@cbxss/eoka";
import type {
  Action,
  ActionReceipt,
  CallOptions,
  ConnectOptions,
  ErrorDetails,
  Expectation,
  InvokeClient,
  BrowserInstallation,
  InstallBrowserOptions,
  InstallProgress,
  PlatformId,
  ReleaseManifest,
  LaunchOptions,
  LocatorSpec,
  MethodResults,
  OperationRecord,
  PerformOptions,
  WaitOptions,
  Artifact,
  ScreenshotOptions,
  ScreenshotFileOptions,
  RecordOptions,
  RecordingRecord,
  RecordingFailure,
  RecordingState,
} from "@cbxss/eoka";
import type { MethodName, MethodParams, ProtocolResponse, ServerError } from "@cbxss/eoka/protocol";
import { protocolVersion as wireVersion } from "@cbxss/eoka/protocol";

export const launchOptions: LaunchOptions = {
  serverPath: "/opt/eoka/eoka-server",
  executablePath: "/opt/chrome/chrome",
  sandbox: true,
  viewport: { width: 1024, height: 768, deviceScaleFactor: 1 },
};
export const managedLaunchOptions: LaunchOptions = {
  browserCacheDir: "/opt/eoka/cache",
  download: "never",
  runtimeDir: "/tmp/eoka",
  sandbox: true,
  onProgress: (progress: InstallProgress) => { void progress.downloadedBytes; },
};
export const installOptions: InstallBrowserOptions = {
  browserCacheDir: "/opt/eoka/cache",
  download: "never",
  timeoutMs: 300000,
  signal: new AbortController().signal,
};
export async function prepareBrowser(): Promise<BrowserInstallation> {
  return installBrowser(installOptions);
}
export const manifest: ReleaseManifest = releaseManifest;
export const platform: PlatformId = selectPlatform("linux", "x64", "2.35");
export const connectOptions: ConnectOptions = {
  serverPath: "/opt/eoka/eoka-server",
  browserWSEndpoint: "ws://127.0.0.1:9222/devtools/browser/example",
};
export const locator: LocatorSpec = { query: { kind: "css", selector: "#name" } };
export const expectation: Expectation = { kind: "value", target: locator, value: "Ada" };
export const action: Action = { kind: "fill", target: locator, value: "Ada" };
export const performOptions: PerformOptions = {
  operationId: "7f2e1450-f1bd-4a5e-b111-215a5d873e26",
  actionOptions: { expect: expectation, timeout_ms: 5000 },
  timeoutMs: 6000,
};
export const waitOptions: WaitOptions = { waitTimeoutMs: 1000, timeoutMs: 2000 };
export const callOptions: CallOptions = { signal: new AbortController().signal, timeoutMs: 2000 };
export const details: ErrorDetails = { operationId: performOptions.operationId! };
export const wireError: ServerError = { code: "InvalidParams", message: "Invalid request" };
export const response: ProtocolResponse<"page.evaluate"> = { id: 1, result: { result: { answer: 42 } } };

export async function useClient<M extends MethodName>(client: InvokeClient, method: M, params: MethodParams[M]): Promise<MethodResults[M]> {
  return client.invoke(method, params, callOptions);
}

export async function usePage(page: Page): Promise<{ receipt: ActionReceipt; record: OperationRecord; evaluation: MethodResults["page.evaluate"] }> {
  const receipt = await page.fill(locator, "Ada", performOptions);
  const operation: Operation = await page.perform(action, performOptions);
  const record = await operation.wait(waitOptions);
  await operation.get(callOptions);
  await page.operation(operation.id).get(callOptions);
  const evaluation = await page.invoke("page.evaluate", { pageId: page.id, js: "({ answer: 42 })" });
  const bounded = await page.fetchBounded("/text", { maxBytes: 8192 });
  const retainedBytes: number = bounded.bytesRead;
  const omission: string | null | undefined = bounded.bodyOmitted;
  void [retainedBytes, omission];
  return { receipt, record, evaluation };
}
export const screenshotOptions: ScreenshotOptions = { format: "jpeg", quality: 80, clip: { x: 0, y: 0, width: 640, height: 360 } };
export const screenshotFileOptions: ScreenshotFileOptions = { path: "./captures/page.webp", format: "webp", fullPage: true, ...callOptions };
export const recordOptions: RecordOptions = { path: "./captures/session.mp4", fps: 30, maxWidth: 1280, maxHeight: 720, ...callOptions };

export async function useCapture(browser: Browser, page: Page): Promise<Artifact> {
  const image: Buffer = await page.screenshot({ ...screenshotOptions, ...callOptions });
  const legacyImage: Buffer = await page.screenshot(callOptions);
  const screenshot: Artifact = await page.screenshotToFile(screenshotFileOptions);
  const recording: Recording = await page.record(recordOptions);
  const record: RecordingRecord = await recording.status(callOptions);
  const state: RecordingState = record.state;
  const failure: RecordingFailure | null | undefined = record.failure;
  const completed: Artifact = await recording.stop(callOptions);
  const recovered: Recording = browser.recording(recording.id);
  const terminal: RecordingRecord = await recovered.cancel(callOptions);
  const evidence = new RecordingError("RecordingFailed", "Retained evidence", recording.id, terminal);
  const partial: string | undefined = evidence.partialPath;
  void [image, legacyImage, screenshot, state, failure, partial];
  return completed;
}


export async function useBrowser(): Promise<void> {
  const browser = await Browser.launch(launchOptions);
  try {
    const page: Page = await browser.newPage("https://example.com/", { timeoutMs: 15000 });
    const receipt: ActionReceipt = await page.goto("https://example.com/", { timeoutMs: 15000 });
    const supported: boolean = browser.supports("page.evaluate");
    const ownership: "owned" | "borrowed" = browser.ownership;
    const records: OperationRecord[] = await browser.operations();
    void [receipt, supported, ownership, records];
  } finally {
    await browser.close();
  }
}

export function runConsumer() {
  if (protocolVersion.major !== wireVersion.major || !methodNames.includes("page.evaluate")) {
    throw new Error("Generated protocol exports disagree");
  }
  const error = new InstallationError("BrowserNotInstalled", "Offline cache is not prepared", details);
  if (!(error instanceof EokaError)) throw new Error("Public errors do not share EokaError");
  return {
    protocol: protocolVersion,
    runtimeExports: [Browser, Page, Operation, OperationError, Recording, RecordingError, SidecarTransport, ConfigurationError, InstallationError, installBrowser].map((value) => value.name),
    error: { code: error.code, operationId: error.operationId },
  };
}
