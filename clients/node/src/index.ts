export { Browser } from "./browser.js";
export type { BrowserOptions, LaunchOptions, ConnectOptions } from "./browser.js";
export { installBrowser } from "./browser-install.js";
export type { InstallBrowserOptions, InstallProgress, BrowserInstallation } from "./browser-install.js";
export { InstallationError } from "./install-errors.js";
export { releaseManifest } from "./release.js";
export type {
  ReleaseManifest,
  ChromeRelease,
  ChromePlatformRelease,
  ServerRelease,
  ServerPlatformRelease,
} from "./release.js";
export { detectPlatform, selectPlatform } from "./platform.js";
export type { PlatformId } from "./platform.js";
export { Page } from "./page.js";
export type {
  PerformOptions,
  LocatorInput,
  PageWaitOptions,
  AgentStepOptions,
  AgentObserveOptions,
  FetchOptions,
  BoundedFetchOptions,
  ViewportOptions,
  ScreenshotOptions,
  ScreenshotFileOptions,
  RecordOptions,
} from "./page.js";
export { Operation, OperationError } from "./operations.js";
export type { WaitOptions } from "./operations.js";
export { Recording, RecordingError } from "./recording.js";
export { SidecarTransport } from "./transport.js";
export type { SidecarStartOptions } from "./transport.js";
export {
  EokaError,
  ConfigurationError,
  TransportError,
  ProtocolError,
  ServerOperationError,
} from "./errors.js";
export type { ErrorDetails, RequestState } from "./errors.js";
export type { CallOptions, InvokeClient } from "./client.js";
export { protocolVersion, methodNames } from "./generated/protocol.js";
export type {
  Action,
  ActionOptions,
  ActionOutcome,
  ActionReceipt,
  AgentAction,
  AgentMode,
  AgentObservation,
  AgentStepResult,
  BrowserState,
  Artifact,
  ArtifactContainer,
  ScreenshotClip,
  ScreenshotFormat,
  RecordingRecord,
  RecordingState,
  RecordingFailure,
  RecordingFailureStage,
  DispatchState,
  ErrorCode,
  ErrorData,
  ErrorDetail,
  Expectation,
  HelloParams,
  HelloResult,
  LocatorQuery,
  LocatorSpec,
  MethodName,
  MethodParams,
  MethodResults,
  OperationRecord,
  OperationState,
  ProtocolRequest,
  ProtocolResponse,
  ProtocolVersion,
  ServerError,
  TransportLimits,
} from "./generated/protocol.js";
