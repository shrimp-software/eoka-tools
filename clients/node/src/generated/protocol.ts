export type Action = ({ "delta": number; "kind": ("history" & string); [key: string]: unknown; } | { "kind": ("select_option" & string); "target": Action__LocatorSpec; "value": string; [key: string]: unknown; } | { "headers": Record<string, string>; "kind": ("navigate_with_headers" & string); "url": string; [key: string]: unknown; } | { "dx": number; "dy": number; "kind": ("drag" & string); "target": Action__LocatorSpec; [key: string]: unknown; } | { "kind": ("insert_text" & string); "text": string; [key: string]: unknown; } | { "kind": ("select_text" & string); "target": Action__LocatorSpec; "text": string; [key: string]: unknown; } | { "double"?: boolean; "human"?: boolean; "kind": ("click" & string); "target": Action__LocatorSpec; [key: string]: unknown; } | { "input_mode"?: Action__InputMode; "kind": ("fill" & string); "target": Action__LocatorSpec; "value": string; [key: string]: unknown; } | { "input_mode"?: Action__InputMode; "kind": ("type" & string); "target": Action__LocatorSpec; "text": string; [key: string]: unknown; } | { "kind": ("select" & string); "target": Action__LocatorSpec; "values": Array<string>; [key: string]: unknown; } | { "files": Array<string>; "kind": ("upload" & string); "target": Action__LocatorSpec; [key: string]: unknown; } | { "key": string; "kind": ("press_key" & string); [key: string]: unknown; } | { "kind": ("submit" & string); "target": Action__LocatorSpec; [key: string]: unknown; } | { "human"?: boolean; "kind": ("hover" & string); "target": Action__LocatorSpec; [key: string]: unknown; } | { "kind": ("focus" & string); "target": Action__LocatorSpec; [key: string]: unknown; } | { "kind": ("scroll_into_view" & string); "target": Action__LocatorSpec; [key: string]: unknown; } | { "kind": ("navigate" & string); "referrer"?: (string | null); "url": string; [key: string]: unknown; } | { "kind": ("reload" & string); [key: string]: unknown; } | { "kind": ("back" & string); [key: string]: unknown; } | { "kind": ("forward" & string); [key: string]: unknown; });
export type Action__InputMode = (("sequential" & string) | ("bulk" & string));
export type Action__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type Action__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": Action__LocatorQuery; [key: string]: unknown; };

export type ActionOptions = { "destination"?: ActionOptions__Destination; "expect"?: (ActionOptions__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type ActionOptions__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type ActionOptions__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<ActionOptions__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": ActionOptions__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": ActionOptions__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": ActionOptions__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": ActionOptions__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": ActionOptions__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": ActionOptions__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": ActionOptions__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": ActionOptions__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type ActionOptions__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type ActionOptions__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": ActionOptions__LocatorQuery; [key: string]: unknown; };

export type ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));

export type ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": ActionReceipt__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": ActionReceipt__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type ActionReceipt__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type ActionReceipt__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));

export type AgentAction = (({ "kind": ("click" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "input_mode"?: AgentAction__InputMode; "kind": ("fill" & string); "target": string; "text": string; } & { "value"?: never; "key"?: never; "url"?: never; }) | ({ "input_mode"?: AgentAction__InputMode; "kind": ("type" & string); "target": string; "text": string; } & { "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("focus" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("select" & string); "target": string; "value": string; } & { "input_mode"?: never; "text"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("hover" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("scroll_into_view" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "key": string; "kind": ("press_key" & string); } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "url"?: never; }) | ({ "kind": ("navigate" & string); "url": string; } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; }) | ({ "kind": ("back" & string); } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("forward" & string); } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }));
export type AgentAction__InputMode = (("sequential" & string) | ("bulk" & string));

export type AgentMode = (("explore" | "verify") & string);

export type AgentObservation = { "controls": Array<AgentObservation__AgentControl>; "next_cursor"?: (string | null); "observation_id": string; "page"?: (AgentObservation__PageContext | null); "tab_id": string; "tabs": Array<AgentObservation__AgentTab>; "total_controls": number; "truncated": boolean; "warnings": Array<string>; [key: string]: unknown; };
export type AgentObservation__AgentControl = { "checked"?: (boolean | null); "disabled": boolean; "editable": boolean; "expanded"?: (boolean | null); "focused": boolean; "invalid": boolean; "name": string; "readonly": boolean; "required": boolean; "role": string; "target": string; "validation_message"?: (string | null); [key: string]: unknown; };
export type AgentObservation__AgentTab = { "id": string; "selected": boolean; "title": string; "url": string; [key: string]: unknown; };
export type AgentObservation__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type AgentStepResult = { "observation"?: (AgentStepResult__AgentObservation | null); "observation_error"?: (string | null); "operation"?: (AgentStepResult__OperationRecord | null); "operation_error"?: (AgentStepResult__ErrorDetail | null); "operation_id": string; "status": AgentStepResult__OperationStatus; [key: string]: unknown; };
export type AgentStepResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type AgentStepResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": AgentStepResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": AgentStepResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type AgentStepResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type AgentStepResult__AgentControl = { "checked"?: (boolean | null); "disabled": boolean; "editable": boolean; "expanded"?: (boolean | null); "focused": boolean; "invalid": boolean; "name": string; "readonly": boolean; "required": boolean; "role": string; "target": string; "validation_message"?: (string | null); [key: string]: unknown; };
export type AgentStepResult__AgentObservation = { "controls": Array<AgentStepResult__AgentControl>; "next_cursor"?: (string | null); "observation_id": string; "page"?: (AgentStepResult__PageContext | null); "tab_id": string; "tabs": Array<AgentStepResult__AgentTab>; "total_controls": number; "truncated": boolean; "warnings": Array<string>; [key: string]: unknown; };
export type AgentStepResult__AgentTab = { "id": string; "selected": boolean; "title": string; "url": string; [key: string]: unknown; };
export type AgentStepResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type AgentStepResult__ErrorContext = { "action_status"?: (AgentStepResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (AgentStepResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type AgentStepResult__ErrorDetail = { "code": string; "context"?: (AgentStepResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type AgentStepResult__OperationRecord = { "action": string; "dispatch_intent": boolean; "error"?: (AgentStepResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": AgentStepResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (AgentStepResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": AgentStepResult__OperationState; "target_id": string; [key: string]: unknown; };
export type AgentStepResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type AgentStepResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type AgentStepResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type Artifact = { "bytes": number; "container"?: (Artifact__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type Artifact__ArtifactContainer = ("mp4" & string);

export type ArtifactContainer = ("mp4" & string);

export type BrowserState = { "cookies": Array<BrowserState__SessionCookie>; "localStorage"?: Record<string, string>; "sessionStorage"?: Record<string, string>; "url": string; "userAgent": string; [key: string]: unknown; };
export type BrowserState__SessionCookie = { "domain": string; "expires"?: (number | null); "http_only": boolean; "name": string; "path": string; "same_site"?: (string | null); "secure": boolean; "value": string; [key: string]: unknown; };

export type CdpErrorDetail = { "code"?: (number | null); "message": string; "method"?: (string | null); [key: string]: unknown; };

export type CloudflareAutoSolve = { "intervalMs"?: number; "timeoutMs"?: number; [key: string]: unknown; };

export type Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));

export type DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));

export type ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);

export type ErrorData = { "blocker"?: (ErrorData__PageBlocker | null); "cdp"?: (ErrorData__CdpErrorDetail | null); "detail"?: (ErrorData__ErrorDetail | null); "operation_id"?: (string | null); "partialPath"?: (string | null); "path"?: (string | null); "receipt"?: (ErrorData__ActionReceipt | null); "recording"?: (ErrorData__RecordingRecord | null); "recordingId"?: (string | null); "result"?: unknown; "source"?: (ErrorData__ServerError | null); [key: string]: unknown; };
export type ErrorData__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type ErrorData__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": ErrorData__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": ErrorData__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type ErrorData__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type ErrorData__Artifact = { "bytes": number; "container"?: (ErrorData__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type ErrorData__ArtifactContainer = ("mp4" & string);
export type ErrorData__CdpErrorDetail = { "code"?: (number | null); "message": string; "method"?: (string | null); [key: string]: unknown; };
export type ErrorData__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type ErrorData__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type ErrorData__ErrorContext = { "action_status"?: (ErrorData__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (ErrorData__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type ErrorData__ErrorDetail = { "code": string; "context"?: (ErrorData__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type ErrorData__PageBlocker = ({ "dialogType": string; "elapsedMs": number; "kind": ("dialog" & string); "message": string; "url": string; [key: string]: unknown; } | { "elapsedMs": number; "kind": ("navigation" & string); "url"?: (string | null); [key: string]: unknown; });
export type ErrorData__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };
export type ErrorData__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": ErrorData__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": ErrorData__RecordingFailureStage; [key: string]: unknown; };
export type ErrorData__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type ErrorData__RecordingRecord = { "artifact"?: (ErrorData__Artifact | null); "durationMs"?: (number | null); "failure"?: (ErrorData__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": ErrorData__RecordingState; [key: string]: unknown; };
export type ErrorData__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);
export type ErrorData__ServerError = { "code": ErrorData__ErrorCode; "data"?: (ErrorData | null); "message": string; [key: string]: unknown; };

export type ErrorDetail = { "code": string; "context"?: (ErrorDetail__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type ErrorDetail__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type ErrorDetail__ErrorContext = { "action_status"?: (ErrorDetail__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (ErrorDetail__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type ErrorDetail__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": Expectation__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": Expectation__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": Expectation__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": Expectation__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": Expectation__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": Expectation__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": Expectation__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": Expectation__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type Expectation__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type Expectation__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": Expectation__LocatorQuery; [key: string]: unknown; };

export type HelloParams = { "protocol": HelloParams__ProtocolVersion; [key: string]: unknown; };
export type HelloParams__ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };

export type HelloResult = { "capabilities": Array<string>; "coreVersion": string; "limits": HelloResult__TransportLimits; "methods": Array<string>; "protocol": HelloResult__ProtocolVersion; "serverVersion": string; [key: string]: unknown; };
export type HelloResult__ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };
export type HelloResult__TransportLimits = { "maxInFlight": number; "maxMessageBytes": number; [key: string]: unknown; };

export type InputMode = (("sequential" & string) | ("bulk" & string));

export type LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });

export type LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": LocatorSpec__LocatorQuery; [key: string]: unknown; };
export type LocatorSpec__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });

export type OperationRecord = { "action": string; "dispatch_intent": boolean; "error"?: (OperationRecord__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": OperationRecord__ActionOutcome; "preparation_intent": boolean; "receipt"?: (OperationRecord__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": OperationRecord__OperationState; "target_id": string; [key: string]: unknown; };
export type OperationRecord__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type OperationRecord__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": OperationRecord__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": OperationRecord__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type OperationRecord__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type OperationRecord__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type OperationRecord__ErrorContext = { "action_status"?: (OperationRecord__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (OperationRecord__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type OperationRecord__ErrorDetail = { "code": string; "context"?: (OperationRecord__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type OperationRecord__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type OperationRecord__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type OperationState = (("running" | "finished" | "interrupted" | "expired") & string);

export type PageBlocker = ({ "dialogType": string; "elapsedMs": number; "kind": ("dialog" & string); "message": string; "url": string; [key: string]: unknown; } | { "elapsedMs": number; "kind": ("navigation" & string); "url"?: (string | null); [key: string]: unknown; });

export type ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };

export type RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": RecordingFailure__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": RecordingFailure__RecordingFailureStage; [key: string]: unknown; };
export type RecordingFailure__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type RecordingFailure__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);

export type RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);

export type RecordingIdParams = { "recordingId": string; [key: string]: unknown; };

export type RecordingRecord = { "artifact"?: (RecordingRecord__Artifact | null); "durationMs"?: (number | null); "failure"?: (RecordingRecord__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": RecordingRecord__RecordingState; [key: string]: unknown; };
export type RecordingRecord__Artifact = { "bytes": number; "container"?: (RecordingRecord__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type RecordingRecord__ArtifactContainer = ("mp4" & string);
export type RecordingRecord__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type RecordingRecord__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": RecordingRecord__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": RecordingRecord__RecordingFailureStage; [key: string]: unknown; };
export type RecordingRecord__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type RecordingRecord__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type RecordingStartParams = { "fps"?: (number | null); "maxHeight"?: (number | null); "maxWidth"?: (number | null); "pageId": string; "path": string; "recordingId": string; };

export type RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type Request = { "id": number; "method": string; "params"?: unknown; [key: string]: unknown; };

export type Response = (({ "id": (number | null); "result": unknown; } & { "error"?: never; }) | ({ "error": Response__ServerError; "id": (number | null); } & { "result"?: never; }));
export type Response__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type Response__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": Response__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": Response__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type Response__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type Response__Artifact = { "bytes": number; "container"?: (Response__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type Response__ArtifactContainer = ("mp4" & string);
export type Response__CdpErrorDetail = { "code"?: (number | null); "message": string; "method"?: (string | null); [key: string]: unknown; };
export type Response__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type Response__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type Response__ErrorContext = { "action_status"?: (Response__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (Response__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type Response__ErrorData = { "blocker"?: (Response__PageBlocker | null); "cdp"?: (Response__CdpErrorDetail | null); "detail"?: (Response__ErrorDetail | null); "operation_id"?: (string | null); "partialPath"?: (string | null); "path"?: (string | null); "receipt"?: (Response__ActionReceipt | null); "recording"?: (Response__RecordingRecord | null); "recordingId"?: (string | null); "result"?: unknown; "source"?: (Response__ServerError | null); [key: string]: unknown; };
export type Response__ErrorDetail = { "code": string; "context"?: (Response__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type Response__PageBlocker = ({ "dialogType": string; "elapsedMs": number; "kind": ("dialog" & string); "message": string; "url": string; [key: string]: unknown; } | { "elapsedMs": number; "kind": ("navigation" & string); "url"?: (string | null); [key: string]: unknown; });
export type Response__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };
export type Response__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": Response__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": Response__RecordingFailureStage; [key: string]: unknown; };
export type Response__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type Response__RecordingRecord = { "artifact"?: (Response__Artifact | null); "durationMs"?: (number | null); "failure"?: (Response__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": Response__RecordingState; [key: string]: unknown; };
export type Response__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);
export type Response__ServerError = { "code": Response__ErrorCode; "data"?: (Response__ErrorData | null); "message": string; [key: string]: unknown; };

export type ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };

export type ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type ServerError = { "code": ServerError__ErrorCode; "data"?: (ServerError__ErrorData | null); "message": string; [key: string]: unknown; };
export type ServerError__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type ServerError__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": ServerError__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": ServerError__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type ServerError__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type ServerError__Artifact = { "bytes": number; "container"?: (ServerError__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type ServerError__ArtifactContainer = ("mp4" & string);
export type ServerError__CdpErrorDetail = { "code"?: (number | null); "message": string; "method"?: (string | null); [key: string]: unknown; };
export type ServerError__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type ServerError__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type ServerError__ErrorContext = { "action_status"?: (ServerError__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (ServerError__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type ServerError__ErrorData = { "blocker"?: (ServerError__PageBlocker | null); "cdp"?: (ServerError__CdpErrorDetail | null); "detail"?: (ServerError__ErrorDetail | null); "operation_id"?: (string | null); "partialPath"?: (string | null); "path"?: (string | null); "receipt"?: (ServerError__ActionReceipt | null); "recording"?: (ServerError__RecordingRecord | null); "recordingId"?: (string | null); "result"?: unknown; "source"?: (ServerError | null); [key: string]: unknown; };
export type ServerError__ErrorDetail = { "code": string; "context"?: (ServerError__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type ServerError__PageBlocker = ({ "dialogType": string; "elapsedMs": number; "kind": ("dialog" & string); "message": string; "url": string; [key: string]: unknown; } | { "elapsedMs": number; "kind": ("navigation" & string); "url"?: (string | null); [key: string]: unknown; });
export type ServerError__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };
export type ServerError__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": ServerError__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": ServerError__RecordingFailureStage; [key: string]: unknown; };
export type ServerError__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type ServerError__RecordingRecord = { "artifact"?: (ServerError__Artifact | null); "durationMs"?: (number | null); "failure"?: (ServerError__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": ServerError__RecordingState; [key: string]: unknown; };
export type ServerError__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type TransportLimits = { "maxInFlight": number; "maxMessageBytes": number; [key: string]: unknown; };

export type MethodActionsPerformParams = { "action": MethodActionsPerformParams__Action; "operation_id": string; "options"?: MethodActionsPerformParams__ActionOptions; "pageId": string; [key: string]: unknown; };
export type MethodActionsPerformParams__Action = ({ "delta": number; "kind": ("history" & string); [key: string]: unknown; } | { "kind": ("select_option" & string); "target": MethodActionsPerformParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "headers": Record<string, string>; "kind": ("navigate_with_headers" & string); "url": string; [key: string]: unknown; } | { "dx": number; "dy": number; "kind": ("drag" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("insert_text" & string); "text": string; [key: string]: unknown; } | { "kind": ("select_text" & string); "target": MethodActionsPerformParams__LocatorSpec; "text": string; [key: string]: unknown; } | { "double"?: boolean; "human"?: boolean; "kind": ("click" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "input_mode"?: MethodActionsPerformParams__InputMode; "kind": ("fill" & string); "target": MethodActionsPerformParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "input_mode"?: MethodActionsPerformParams__InputMode; "kind": ("type" & string); "target": MethodActionsPerformParams__LocatorSpec; "text": string; [key: string]: unknown; } | { "kind": ("select" & string); "target": MethodActionsPerformParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; } | { "files": Array<string>; "kind": ("upload" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "key": string; "kind": ("press_key" & string); [key: string]: unknown; } | { "kind": ("submit" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "human"?: boolean; "kind": ("hover" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("focus" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("scroll_into_view" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("navigate" & string); "referrer"?: (string | null); "url": string; [key: string]: unknown; } | { "kind": ("reload" & string); [key: string]: unknown; } | { "kind": ("back" & string); [key: string]: unknown; } | { "kind": ("forward" & string); [key: string]: unknown; });
export type MethodActionsPerformParams__ActionOptions = { "destination"?: MethodActionsPerformParams__Destination; "expect"?: (MethodActionsPerformParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodActionsPerformParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodActionsPerformParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodActionsPerformParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodActionsPerformParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodActionsPerformParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodActionsPerformParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodActionsPerformParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodActionsPerformParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodActionsPerformParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodActionsPerformParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodActionsPerformParams__LocatorQuery; [key: string]: unknown; };

export type MethodActionsPerformResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodActionsPerformResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodActionsPerformResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodActionsPerformResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodActionsPerformResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodActionsPerformResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodActionsPerformResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodActionsPerformResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodActionsPerformResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodActionsPerformResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodActionsPerformResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodActionsPerformResult__ErrorContext = { "action_status"?: (MethodActionsPerformResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodActionsPerformResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodActionsPerformResult__ErrorDetail = { "code": string; "context"?: (MethodActionsPerformResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodActionsPerformResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodActionsPerformResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodAgentObserveParams = { "cursor"?: (string | null); "max_controls"?: number; "name"?: (string | null); "pageId": string; "role"?: (string | null); "timeout_ms"?: number; [key: string]: unknown; };

export type MethodAgentObserveResult = { "controls": Array<MethodAgentObserveResult__AgentControl>; "next_cursor"?: (string | null); "observation_id": string; "page"?: (MethodAgentObserveResult__PageContext | null); "tab_id": string; "tabs": Array<MethodAgentObserveResult__AgentTab>; "total_controls": number; "truncated": boolean; "warnings": Array<string>; [key: string]: unknown; };
export type MethodAgentObserveResult__AgentControl = { "checked"?: (boolean | null); "disabled": boolean; "editable": boolean; "expanded"?: (boolean | null); "focused": boolean; "invalid": boolean; "name": string; "readonly": boolean; "required": boolean; "role": string; "target": string; "validation_message"?: (string | null); [key: string]: unknown; };
export type MethodAgentObserveResult__AgentTab = { "id": string; "selected": boolean; "title": string; "url": string; [key: string]: unknown; };
export type MethodAgentObserveResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodAgentStepParams = { "action": MethodAgentStepParams__AgentAction; "action_timeout_ms"?: number; "destination"?: MethodAgentStepParams__Destination; "expect"?: (MethodAgentStepParams__Expectation | null); "max_controls"?: number; "mode"?: MethodAgentStepParams__AgentMode; "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: number; [key: string]: unknown; };
export type MethodAgentStepParams__AgentAction = (({ "kind": ("click" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "input_mode"?: MethodAgentStepParams__InputMode; "kind": ("fill" & string); "target": string; "text": string; } & { "value"?: never; "key"?: never; "url"?: never; }) | ({ "input_mode"?: MethodAgentStepParams__InputMode; "kind": ("type" & string); "target": string; "text": string; } & { "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("focus" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("select" & string); "target": string; "value": string; } & { "input_mode"?: never; "text"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("hover" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("scroll_into_view" & string); "target": string; } & { "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "key": string; "kind": ("press_key" & string); } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "url"?: never; }) | ({ "kind": ("navigate" & string); "url": string; } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; }) | ({ "kind": ("back" & string); } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }) | ({ "kind": ("forward" & string); } & { "target"?: never; "input_mode"?: never; "text"?: never; "value"?: never; "key"?: never; "url"?: never; }));
export type MethodAgentStepParams__AgentMode = (("explore" | "verify") & string);
export type MethodAgentStepParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodAgentStepParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodAgentStepParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodAgentStepParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodAgentStepParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodAgentStepParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodAgentStepParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodAgentStepParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodAgentStepParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodAgentStepParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodAgentStepParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodAgentStepParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodAgentStepParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodAgentStepParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodAgentStepParams__LocatorQuery; [key: string]: unknown; };

export type MethodAgentStepResult = { "observation"?: (MethodAgentStepResult__AgentObservation | null); "observation_error"?: (string | null); "operation"?: (MethodAgentStepResult__OperationRecord | null); "operation_error"?: (MethodAgentStepResult__ErrorDetail | null); "operation_id": string; "status": MethodAgentStepResult__OperationStatus; [key: string]: unknown; };
export type MethodAgentStepResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodAgentStepResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodAgentStepResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodAgentStepResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodAgentStepResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodAgentStepResult__AgentControl = { "checked"?: (boolean | null); "disabled": boolean; "editable": boolean; "expanded"?: (boolean | null); "focused": boolean; "invalid": boolean; "name": string; "readonly": boolean; "required": boolean; "role": string; "target": string; "validation_message"?: (string | null); [key: string]: unknown; };
export type MethodAgentStepResult__AgentObservation = { "controls": Array<MethodAgentStepResult__AgentControl>; "next_cursor"?: (string | null); "observation_id": string; "page"?: (MethodAgentStepResult__PageContext | null); "tab_id": string; "tabs": Array<MethodAgentStepResult__AgentTab>; "total_controls": number; "truncated": boolean; "warnings": Array<string>; [key: string]: unknown; };
export type MethodAgentStepResult__AgentTab = { "id": string; "selected": boolean; "title": string; "url": string; [key: string]: unknown; };
export type MethodAgentStepResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodAgentStepResult__ErrorContext = { "action_status"?: (MethodAgentStepResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodAgentStepResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodAgentStepResult__ErrorDetail = { "code": string; "context"?: (MethodAgentStepResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodAgentStepResult__OperationRecord = { "action": string; "dispatch_intent": boolean; "error"?: (MethodAgentStepResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodAgentStepResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodAgentStepResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodAgentStepResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodAgentStepResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodAgentStepResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodAgentStepResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodBackParams = { "expect"?: (MethodBackParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodBackParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodBackParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodBackParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodBackParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodBackParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodBackParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodBackParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodBackParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodBackParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodBackParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodBackParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodBackParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodBackParams__LocatorQuery; [key: string]: unknown; };

export type MethodBackResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodBackResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodBackResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodBackResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodBackResult__OperationState; "status": MethodBackResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodBackResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodBackResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodBackResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodBackResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodBackResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodBackResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodBackResult__ErrorContext = { "action_status"?: (MethodBackResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodBackResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodBackResult__ErrorDetail = { "code": string; "context"?: (MethodBackResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodBackResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodBackResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodBackResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodBrowserCloseParams = Record<string, never>;

export type MethodBrowserCloseResult = Record<string, never>;

export type MethodBrowserCloseTabParams = { "pageId": string; [key: string]: unknown; };

export type MethodBrowserCloseTabResult = { "cleanupError"?: (string | null); [key: string]: unknown; };

export type MethodBrowserConnectParams = { "autoSolveCloudflare"?: (MethodBrowserConnectParams__CloudflareAutoSolve | null); "wsEndpoint": string; };
export type MethodBrowserConnectParams__CloudflareAutoSolve = { "intervalMs"?: number; "timeoutMs"?: number; [key: string]: unknown; };

export type MethodBrowserConnectResult = { "browserVersion": string; "ownership": MethodBrowserConnectResult__BrowserOwnership; [key: string]: unknown; };
export type MethodBrowserConnectResult__BrowserOwnership = (("owned" | "borrowed") & string);

export type MethodBrowserLaunchParams = { "aggressiveCdpEvasion"?: boolean; "autoSolveCloudflare"?: (MethodBrowserLaunchParams__CloudflareAutoSolve | null); "deviceScaleFactor"?: (number | null); "executablePath"?: (string | null); "extraArgs"?: Array<string>; "geoAlign"?: boolean; "headless"?: boolean; "proxy"?: (MethodBrowserLaunchParams__ProxyParams | null); "runtimeDir"?: (string | null); "sandbox"?: boolean; "timezone"?: (string | null); "userAgent"?: (string | null); "userDataDir"?: (string | null); "viewportHeight"?: (number | null); "viewportWidth"?: (number | null); [key: string]: unknown; };
export type MethodBrowserLaunchParams__CloudflareAutoSolve = { "intervalMs"?: number; "timeoutMs"?: number; [key: string]: unknown; };
export type MethodBrowserLaunchParams__ProxyParams = { "password"?: (string | null); "server": string; "username"?: (string | null); [key: string]: unknown; };

export type MethodBrowserLaunchResult = { "browserVersion": string; "ownership": MethodBrowserLaunchResult__BrowserOwnership; [key: string]: unknown; };
export type MethodBrowserLaunchResult__BrowserOwnership = (("owned" | "borrowed") & string);

export type MethodBrowserNewPageParams = { "url"?: (string | null); [key: string]: unknown; };

export type MethodBrowserNewPageResult = { "pageId": string; [key: string]: unknown; };

export type MethodBrowserTabsParams = Record<string, never>;

export type MethodBrowserTabsResult = { "tabs": Array<MethodBrowserTabsResult__Tab>; [key: string]: unknown; };
export type MethodBrowserTabsResult__Tab = { "id": string; "title": string; "url": string; [key: string]: unknown; };

export type MethodCaptchaDatadomeParams = { "max_attempts"?: number; "pageId": string; "timeout_ms"?: number; };

export type MethodCaptchaDatadomeResult = { "elapsed_ms": number; "outcome": MethodCaptchaDatadomeResult__DataDomeOutcome; "tab_id": string; [key: string]: unknown; };
export type MethodCaptchaDatadomeResult__DataDomeOutcome = (("challenge_cleared" | "not_present" | "blocked" | "ip_banned" | "failed") & string);

export type MethodCaptchaDetectParams = { "pageId": string; };

export type MethodCaptchaDetectResult = { "detected": boolean; "sitekey"?: (string | null); "type"?: (string | null); [key: string]: unknown; };

export type MethodCaptchaInjectParams = { "callback"?: (string | null); "captcha_type"?: string; "click_after"?: (string | null); "pageId": string; "token": string; [key: string]: unknown; };

export type MethodCaptchaInjectResult = { "callbacks": Array<string>; "click_after"?: (MethodCaptchaInjectResult__ClickAfter | null); "created": Array<string>; "errors": Array<string>; "kind": string; "next_action"?: (string | null); "solverUserAgent"?: (string | null); "updated_count": number; [key: string]: unknown; };
export type MethodCaptchaInjectResult__ClickAfter = { "clicked": string; "target": string; [key: string]: unknown; };

export type MethodCaptchaTurnstileParams = { "pageId": string; "timeout_ms"?: number; };

export type MethodCaptchaTurnstileResult = { "clicks": number; "elapsed_ms": number; "tab_id": string; [key: string]: unknown; };

export type MethodClearCookiesParams = { "pageId": string; };

export type MethodClearCookiesResult = string;

export type MethodClickParams = { "expect"?: (MethodClickParams__Expectation | null); "intercept_nav"?: (MethodClickParams__NavIntercept | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodClickParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodClickParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodClickParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodClickParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodClickParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodClickParams__LocatorQuery; [key: string]: unknown; };
export type MethodClickParams__NavIntercept = (("block" | "allow") & string);

export type MethodClickResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodClickResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodClickResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodClickResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodClickResult__OperationState; "status": MethodClickResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodClickResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodClickResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodClickResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodClickResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodClickResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodClickResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodClickResult__ErrorContext = { "action_status"?: (MethodClickResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodClickResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodClickResult__ErrorDetail = { "code": string; "context"?: (MethodClickResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodClickResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodClickResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodClickResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodClickShadowParams = { "frame_id"?: (string | null); "pageId": string; "selector": string; "timeout_ms"?: number; [key: string]: unknown; };

export type MethodClickShadowResult = { "frame_id": string; "selector": string; "x": number; "y": number; [key: string]: unknown; };

export type MethodCloneFromParams = { "pageId": string; "source": string; "to"?: (string | null); [key: string]: unknown; };

export type MethodCloneFromResult = string;

export type MethodConsoleParams = { "clear"?: boolean; "level"?: (string | null); "pageId": string; [key: string]: unknown; };

export type MethodConsoleResult = Array<MethodConsoleResult__ConsoleEntry>;
export type MethodConsoleResult__ConsoleEntry = { "level": MethodConsoleResult__ConsoleLevel; "text": string; "ts": number; [key: string]: unknown; };
export type MethodConsoleResult__ConsoleLevel = (("log" | "warn" | "error" | "info" | "debug") & string);

export type MethodContentParams = { "pageId": string; };

export type MethodContentResult = string;

export type MethodCookiesParams = { "pageId": string; };

export type MethodCookiesResult = Array<MethodCookiesResult__Cookie>;
export type MethodCookiesResult__Cookie = { "domain": string; "expires"?: (number | null); "httpOnly": boolean; "name": string; "path": string; "sameSite"?: (string | null); "secure": boolean; "value": string; [key: string]: unknown; };

export type MethodDeleteCookieParams = { "domain"?: (string | null); "name": string; "pageId": string; [key: string]: unknown; };

export type MethodDeleteCookieResult = string;

export type MethodDialogAcceptParams = { "pageId": string; "prompt_text"?: (string | null); [key: string]: unknown; };

export type MethodDialogAcceptResult = string;

export type MethodDialogDismissParams = { "pageId": string; };

export type MethodDialogDismissResult = string;

export type MethodDoubleClickParams = { "expect"?: (MethodDoubleClickParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodDoubleClickParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodDoubleClickParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodDoubleClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodDoubleClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodDoubleClickParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodDoubleClickParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodDoubleClickParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodDoubleClickParams__LocatorQuery; [key: string]: unknown; };

export type MethodDoubleClickResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodDoubleClickResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodDoubleClickResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodDoubleClickResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodDoubleClickResult__OperationState; "status": MethodDoubleClickResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodDoubleClickResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodDoubleClickResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodDoubleClickResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodDoubleClickResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodDoubleClickResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodDoubleClickResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodDoubleClickResult__ErrorContext = { "action_status"?: (MethodDoubleClickResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodDoubleClickResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodDoubleClickResult__ErrorDetail = { "code": string; "context"?: (MethodDoubleClickResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodDoubleClickResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodDoubleClickResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodDoubleClickResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodDownloadCancelParams = { "id": string; "pageId": string; [key: string]: unknown; };

export type MethodDownloadCancelResult = { "completion"?: (string | null); "created_at_ms": number; "error"?: (MethodDownloadCancelResult__ErrorDetail | null); "id": string; "native_complete"?: boolean; "native_guid"?: (string | null); "output": string; "published_path"?: (string | null); "received_bytes": number; "sha256"?: (string | null); "source_url"?: (string | null); "staging_path"?: (string | null); "state": MethodDownloadCancelResult__DownloadState; "suggested_filename"?: (string | null); "tab_id": string; "total_bytes"?: (number | null); "updated_at_ms": number; [key: string]: unknown; };
export type MethodDownloadCancelResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodDownloadCancelResult__DownloadState = (("pending" | "downloading" | "verifying" | "completed" | "canceled" | "failed") & string);
export type MethodDownloadCancelResult__ErrorContext = { "action_status"?: (MethodDownloadCancelResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodDownloadCancelResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodDownloadCancelResult__ErrorDetail = { "code": string; "context"?: (MethodDownloadCancelResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodDownloadCancelResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodDownloadGetParams = { "id": string; "pageId": string; [key: string]: unknown; };

export type MethodDownloadGetResult = { "completion"?: (string | null); "created_at_ms": number; "error"?: (MethodDownloadGetResult__ErrorDetail | null); "id": string; "native_complete"?: boolean; "native_guid"?: (string | null); "output": string; "published_path"?: (string | null); "received_bytes": number; "sha256"?: (string | null); "source_url"?: (string | null); "staging_path"?: (string | null); "state": MethodDownloadGetResult__DownloadState; "suggested_filename"?: (string | null); "tab_id": string; "total_bytes"?: (number | null); "updated_at_ms": number; [key: string]: unknown; };
export type MethodDownloadGetResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodDownloadGetResult__DownloadState = (("pending" | "downloading" | "verifying" | "completed" | "canceled" | "failed") & string);
export type MethodDownloadGetResult__ErrorContext = { "action_status"?: (MethodDownloadGetResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodDownloadGetResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodDownloadGetResult__ErrorDetail = { "code": string; "context"?: (MethodDownloadGetResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodDownloadGetResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodDownloadListParams = { "pageId": string; };

export type MethodDownloadListResult = Array<MethodDownloadListResult__DownloadRecord>;
export type MethodDownloadListResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodDownloadListResult__DownloadRecord = { "created_at_ms": number; "error"?: (MethodDownloadListResult__ErrorDetail | null); "id": string; "native_complete"?: boolean; "native_guid"?: (string | null); "output": string; "published_path"?: (string | null); "received_bytes": number; "sha256"?: (string | null); "source_url"?: (string | null); "staging_path"?: (string | null); "state": MethodDownloadListResult__DownloadState; "suggested_filename"?: (string | null); "tab_id": string; "total_bytes"?: (number | null); "updated_at_ms": number; [key: string]: unknown; };
export type MethodDownloadListResult__DownloadState = (("pending" | "downloading" | "verifying" | "completed" | "canceled" | "failed") & string);
export type MethodDownloadListResult__ErrorContext = { "action_status"?: (MethodDownloadListResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodDownloadListResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodDownloadListResult__ErrorDetail = { "code": string; "context"?: (MethodDownloadListResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodDownloadListResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodDownloadStartParams = { "click"?: (string | null); "expect_bytes"?: (number | null); "operation_id"?: (string | null); "output": string; "pageId": string; "start_timeout_ms"?: number; "url"?: (string | null); [key: string]: unknown; };

export type MethodDownloadStartResult = { "completion"?: (string | null); "created_at_ms": number; "error"?: (MethodDownloadStartResult__ErrorDetail | null); "id": string; "native_complete"?: boolean; "native_guid"?: (string | null); "output": string; "published_path"?: (string | null); "received_bytes": number; "sha256"?: (string | null); "source_url"?: (string | null); "staging_path"?: (string | null); "state": MethodDownloadStartResult__DownloadState; "suggested_filename"?: (string | null); "tab_id": string; "total_bytes"?: (number | null); "updated_at_ms": number; [key: string]: unknown; };
export type MethodDownloadStartResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodDownloadStartResult__DownloadState = (("pending" | "downloading" | "verifying" | "completed" | "canceled" | "failed") & string);
export type MethodDownloadStartResult__ErrorContext = { "action_status"?: (MethodDownloadStartResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodDownloadStartResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodDownloadStartResult__ErrorDetail = { "code": string; "context"?: (MethodDownloadStartResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodDownloadStartResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodDownloadWaitParams = { "id": string; "pageId": string; "timeout_ms"?: number; [key: string]: unknown; };

export type MethodDownloadWaitResult = { "completion"?: (string | null); "created_at_ms": number; "error"?: (MethodDownloadWaitResult__ErrorDetail | null); "id": string; "native_complete"?: boolean; "native_guid"?: (string | null); "output": string; "published_path"?: (string | null); "received_bytes": number; "sha256"?: (string | null); "source_url"?: (string | null); "staging_path"?: (string | null); "state": MethodDownloadWaitResult__DownloadState; "suggested_filename"?: (string | null); "tab_id": string; "total_bytes"?: (number | null); "updated_at_ms": number; [key: string]: unknown; };
export type MethodDownloadWaitResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodDownloadWaitResult__DownloadState = (("pending" | "downloading" | "verifying" | "completed" | "canceled" | "failed") & string);
export type MethodDownloadWaitResult__ErrorContext = { "action_status"?: (MethodDownloadWaitResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodDownloadWaitResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodDownloadWaitResult__ErrorDetail = { "code": string; "context"?: (MethodDownloadWaitResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodDownloadWaitResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodDumpStorageParams = { "pageId": string; };

export type MethodDumpStorageResult = { "localStorage": Record<string, string>; "sessionStorage": Record<string, string>; [key: string]: unknown; };

export type MethodEmulateParams = { "desktop"?: boolean; "dpr"?: number; "height"?: number; "pageId": string; "reset"?: boolean; "width"?: number; [key: string]: unknown; };

export type MethodEmulateResult = (MethodEmulateResult__Viewport | string);
export type MethodEmulateResult__Viewport = { "dpr": number; "height": number; "mobile": boolean; "width": number; [key: string]: unknown; };

export type MethodErrorsParams = { "clear"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodErrorsResult = Array<MethodErrorsResult__PageError>;
export type MethodErrorsResult__PageError = { "col": number; "line": number; "message": string; "source": string; "stack": string; "ts": number; [key: string]: unknown; };

export type MethodEvalParams = { "arguments"?: Array<unknown>; "code"?: (string | null); "file"?: (string | null); "function"?: boolean; "json"?: boolean; "max_size"?: (number | null); "no_await"?: boolean; "pageId": string; "raw"?: boolean; [key: string]: unknown; };

export type MethodEvalResult = unknown;

export type MethodExecParams = { "arguments"?: Array<unknown>; "code"?: (string | null); "file"?: (string | null); "function"?: boolean; "json"?: boolean; "max_size"?: (number | null); "no_await"?: boolean; "pageId": string; "raw"?: boolean; [key: string]: unknown; };

export type MethodExecResult = string;

export type MethodExistsParams = { "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodExistsResult = { "exists": boolean; [key: string]: unknown; };

export type MethodFakeCameraParams = { "file": string; "loop_video"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodFakeCameraResult = string;

export type MethodFetchParams = { "body"?: (string | null); "body_only"?: boolean; "headers"?: unknown; "max_body"?: (number | null); "method"?: (string | null); "pageId": string; "redirect"?: (string | null); "url": string; [key: string]: unknown; };

export type MethodFetchResult = (string | MethodFetchResult__FetchedResponse);
export type MethodFetchResult__FetchedResponse = { "body": string; "bodyOmitted"?: (string | null); "bytesRead": number; "headers": Record<string, string>; "redirected": boolean; "status": number; "statusText": string; "truncated": boolean; "type": string; "url": string; [key: string]: unknown; };

export type MethodFillParams = { "expect"?: (MethodFillParams__Expectation | null); "input_mode"?: MethodFillParams__InputMode; "operation_id"?: (string | null); "pageId": string; "target": string; "text": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodFillParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodFillParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodFillParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodFillParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodFillParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodFillParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodFillParams__LocatorQuery; [key: string]: unknown; };

export type MethodFillResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodFillResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodFillResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodFillResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodFillResult__OperationState; "status": MethodFillResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodFillResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodFillResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodFillResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodFillResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodFillResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodFillResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodFillResult__ErrorContext = { "action_status"?: (MethodFillResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodFillResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodFillResult__ErrorDetail = { "code": string; "context"?: (MethodFillResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodFillResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodFillResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodFillResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodFindParams = { "pageId": string; "text": string; [key: string]: unknown; };

export type MethodFindResult = string;

export type MethodForwardParams = { "expect"?: (MethodForwardParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodForwardParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodForwardParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodForwardParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodForwardParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodForwardParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodForwardParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodForwardParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodForwardParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodForwardParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodForwardParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodForwardParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodForwardParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodForwardParams__LocatorQuery; [key: string]: unknown; };

export type MethodForwardResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodForwardResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodForwardResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodForwardResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodForwardResult__OperationState; "status": MethodForwardResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodForwardResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodForwardResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodForwardResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodForwardResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodForwardResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodForwardResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodForwardResult__ErrorContext = { "action_status"?: (MethodForwardResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodForwardResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodForwardResult__ErrorDetail = { "code": string; "context"?: (MethodForwardResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodForwardResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodForwardResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodForwardResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodFrameEvalParams = { "code"?: (string | null); "file"?: (string | null); "frame": string; "pageId": string; [key: string]: unknown; };

export type MethodFrameEvalResult = string;

export type MethodFramesParams = { "pageId": string; };

export type MethodFramesResult = Array<MethodFramesResult__FrameSummary>;
export type MethodFramesResult__FrameSummary = { "id": string; "name"?: (string | null); "url": string; [key: string]: unknown; };

export type MethodGetAttributeParams = { "name": string; "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodGetAttributeResult = { "value"?: (string | null); [key: string]: unknown; };

export type MethodGetTextParams = { "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodGetTextResult = string;

export type MethodHeadersParams = { "headers_json": string; "pageId": string; [key: string]: unknown; };

export type MethodHeadersResult = string;

export type MethodHoverParams = { "expect"?: (MethodHoverParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodHoverParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodHoverParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodHoverParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodHoverParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodHoverParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodHoverParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodHoverParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodHoverParams__LocatorQuery; [key: string]: unknown; };

export type MethodHoverResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodHoverResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodHoverResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodHoverResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodHoverResult__OperationState; "status": MethodHoverResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodHoverResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodHoverResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodHoverResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodHoverResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodHoverResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodHoverResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodHoverResult__ErrorContext = { "action_status"?: (MethodHoverResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodHoverResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodHoverResult__ErrorDetail = { "code": string; "context"?: (MethodHoverResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodHoverResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodHoverResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodHoverResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodInfoParams = { "pageId": string; };

export type MethodInfoResult = { "title": string; "url": string; [key: string]: unknown; };

export type MethodInterceptAddParams = { "capture"?: (string | null); "pageId": string; "respond"?: (string | null); "status"?: number; "url_pattern": string; [key: string]: unknown; };

export type MethodInterceptAddResult = string;

export type MethodInterceptListParams = { "pageId": string; };

export type MethodInterceptListResult = Array<MethodInterceptListResult__InterceptRuleSummary>;
export type MethodInterceptListResult__InterceptRuleSummary = { "capture"?: (string | null); "id": number; "pattern": string; "respond"?: (string | null); "status": number; [key: string]: unknown; };

export type MethodInterceptLogParams = { "clear"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodInterceptLogResult = Array<MethodInterceptLogResult__InterceptLogRecord>;
export type MethodInterceptLogResult__InterceptLogRecord = { "action": string; "has_body": boolean; "method": string; "network_entry_id"?: (number | null); "network_id"?: (string | null); "rule_id": number; "session_id"?: (string | null); "url": string; [key: string]: unknown; };

export type MethodInterceptRemoveParams = { "id": string; "pageId": string; [key: string]: unknown; };

export type MethodInterceptRemoveResult = string;

export type MethodJsAllowParams = { "domain": string; "pageId": string; [key: string]: unknown; };

export type MethodJsAllowResult = { "allow": Array<string>; "block": Array<string>; "mode": MethodJsAllowResult__ScriptMode; [key: string]: unknown; };
export type MethodJsAllowResult__ScriptMode = (("allow-all" | "block-all") & string);

export type MethodJsBlockParams = { "domain": string; "pageId": string; [key: string]: unknown; };

export type MethodJsBlockResult = { "allow": Array<string>; "block": Array<string>; "mode": MethodJsBlockResult__ScriptMode; [key: string]: unknown; };
export type MethodJsBlockResult__ScriptMode = (("allow-all" | "block-all") & string);

export type MethodJsListParams = { "pageId": string; };

export type MethodJsListResult = { "allow": Array<string>; "block": Array<string>; "mode": MethodJsListResult__ScriptMode; [key: string]: unknown; };
export type MethodJsListResult__ScriptMode = (("allow-all" | "block-all") & string);

export type MethodJsModeParams = { "mode": string; "pageId": string; [key: string]: unknown; };

export type MethodJsModeResult = { "allow": Array<string>; "block": Array<string>; "mode": MethodJsModeResult__ScriptMode; [key: string]: unknown; };
export type MethodJsModeResult__ScriptMode = (("allow-all" | "block-all") & string);

export type MethodJsRemoveParams = { "domain": string; "pageId": string; [key: string]: unknown; };

export type MethodJsRemoveResult = { "allow": Array<string>; "block": Array<string>; "mode": MethodJsRemoveResult__ScriptMode; [key: string]: unknown; };
export type MethodJsRemoveResult__ScriptMode = (("allow-all" | "block-all") & string);

export type MethodKeyParams = { "expect"?: (MethodKeyParams__Expectation | null); "key": string; "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodKeyParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodKeyParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodKeyParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodKeyParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodKeyParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodKeyParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodKeyParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodKeyParams__LocatorQuery; [key: string]: unknown; };

export type MethodKeyResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodKeyResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodKeyResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodKeyResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodKeyResult__OperationState; "status": MethodKeyResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodKeyResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodKeyResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodKeyResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodKeyResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodKeyResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodKeyResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodKeyResult__ErrorContext = { "action_status"?: (MethodKeyResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodKeyResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodKeyResult__ErrorDetail = { "code": string; "context"?: (MethodKeyResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodKeyResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodKeyResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodKeyResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodKeyDownParams = { "key": string; "pageId": string; [key: string]: unknown; };

export type MethodKeyDownResult = string;

export type MethodKeyUpParams = { "key": string; "pageId": string; [key: string]: unknown; };

export type MethodKeyUpResult = string;

export type MethodLoadStateParams = { "no_navigate"?: boolean; "pageId": string; "path": string; [key: string]: unknown; };

export type MethodLoadStateResult = string;

export type MethodMouseDownParams = { "button": MethodMouseDownParams__MouseButton; "pageId": string; "x": number; "y": number; [key: string]: unknown; };
export type MethodMouseDownParams__MouseButton = (("left" | "middle" | "right" | "back" | "forward") & string);

export type MethodMouseDownResult = string;

export type MethodMouseMoveParams = { "pageId": string; "x": number; "y": number; [key: string]: unknown; };

export type MethodMouseMoveResult = string;

export type MethodMouseUpParams = { "button": MethodMouseUpParams__MouseButton; "pageId": string; "x": number; "y": number; [key: string]: unknown; };
export type MethodMouseUpParams__MouseButton = (("left" | "middle" | "right" | "back" | "forward") & string);

export type MethodMouseUpResult = string;

export type MethodNetworkClearParams = { "pageId": string; };

export type MethodNetworkClearResult = { "cleared": boolean; "meta": MethodNetworkClearResult__NetworkMeta; [key: string]: unknown; };
export type MethodNetworkClearResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };

export type MethodNetworkExportParams = { "format"?: string; "pageId": string; "path": string; "settle_ms"?: (number | null); [key: string]: unknown; };

export type MethodNetworkExportResult = { "body_omitted": number; "body_pending": number; "entries": number; "exported": number; "format": string; "meta": MethodNetworkExportResult__NetworkMeta; "path": string; "skipped_in_flight": number; "warnings": Array<string>; [key: string]: unknown; };
export type MethodNetworkExportResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };

export type MethodNetworkLogParams = { "compact"?: boolean; "limit"?: (number | null); "method"?: (string | null); "pageId": string; "pattern"?: (string | null); "since"?: (number | null); "status"?: (number | null); [key: string]: unknown; };

export type MethodNetworkLogResult = { "count": number; "entries": Array<MethodNetworkLogResult__NetworkLogRow>; "filters": MethodNetworkLogResult__NetworkLogFilters; "meta": MethodNetworkLogResult__NetworkMeta; [key: string]: unknown; };
export type MethodNetworkLogResult__NetworkBodySummary = { "base64": boolean; "bytes": number; "omitted"?: (string | null); };
export type MethodNetworkLogResult__NetworkCompactEntry = { "complete": boolean; "id": number; "method": string; "status"?: (number | null); "url": string; [key: string]: unknown; };
export type MethodNetworkLogResult__NetworkLogEntry = { "complete": boolean; "duration_ms": number; "failed"?: (string | null); "id": number; "method": string; "request_body"?: (MethodNetworkLogResult__NetworkBodySummary | null); "resource_type"?: (string | null); "response_body"?: (MethodNetworkLogResult__NetworkBodySummary | null); "status"?: (number | null); "url": string; [key: string]: unknown; };
export type MethodNetworkLogResult__NetworkLogFilters = { "compact": boolean; "limit": number; "method"?: (string | null); "pattern"?: (string | null); "since"?: (number | null); "status"?: (number | null); [key: string]: unknown; };
export type MethodNetworkLogResult__NetworkLogRow = (MethodNetworkLogResult__NetworkLogEntry | MethodNetworkLogResult__NetworkCompactEntry);
export type MethodNetworkLogResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };

export type MethodNetworkRecordStartParams = { "clear"?: boolean; "max_body_bytes"?: number; "no_bodies"?: boolean; "pageId": string; "patterns"?: Array<string>; [key: string]: unknown; };

export type MethodNetworkRecordStartResult = { "meta": MethodNetworkRecordStartResult__NetworkMeta; "status": MethodNetworkRecordStartResult__NetworkStatus; [key: string]: unknown; };
export type MethodNetworkRecordStartResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };
export type MethodNetworkRecordStartResult__NetworkStatus = { "active": boolean; "body_failed": number; "body_limit_bytes": number; "body_pending": number; "captured_body_bytes": number; "dropped_events": number; "entry_count": number; "in_flight": number; "lagged_events": number; "last_id": number; "next_since": number; "patterns": Array<string>; "per_body_limit_bytes": number; "session_id"?: (string | null); "started_at"?: (number | null); "target_id"?: (string | null); [key: string]: unknown; };

export type MethodNetworkRecordStatusParams = { "pageId": string; };

export type MethodNetworkRecordStatusResult = { "meta": MethodNetworkRecordStatusResult__NetworkMeta; "status": MethodNetworkRecordStatusResult__NetworkStatus; [key: string]: unknown; };
export type MethodNetworkRecordStatusResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };
export type MethodNetworkRecordStatusResult__NetworkStatus = { "active": boolean; "body_failed": number; "body_limit_bytes": number; "body_pending": number; "captured_body_bytes": number; "dropped_events": number; "entry_count": number; "in_flight": number; "lagged_events": number; "last_id": number; "next_since": number; "patterns": Array<string>; "per_body_limit_bytes": number; "session_id"?: (string | null); "started_at"?: (number | null); "target_id"?: (string | null); [key: string]: unknown; };

export type MethodNetworkRecordStopParams = { "pageId": string; };

export type MethodNetworkRecordStopResult = { "meta": MethodNetworkRecordStopResult__NetworkMeta; "status": MethodNetworkRecordStopResult__NetworkStatus; [key: string]: unknown; };
export type MethodNetworkRecordStopResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };
export type MethodNetworkRecordStopResult__NetworkStatus = { "active": boolean; "body_failed": number; "body_limit_bytes": number; "body_pending": number; "captured_body_bytes": number; "dropped_events": number; "entry_count": number; "in_flight": number; "lagged_events": number; "last_id": number; "next_since": number; "patterns": Array<string>; "per_body_limit_bytes": number; "session_id"?: (string | null); "started_at"?: (number | null); "target_id"?: (string | null); [key: string]: unknown; };

export type MethodNetworkShowParams = { "body"?: boolean; "id": number; "max_body"?: (number | null); "pageId": string; [key: string]: unknown; };

export type MethodNetworkShowResult = { "entry": MethodNetworkShowResult__NetworkShowEntry; "meta": MethodNetworkShowResult__NetworkMeta; [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkBody = { "base64": boolean; "bytes": number; "mime_type"?: (string | null); "omitted"?: (string | null); "text"?: (string | null); "truncated"?: (boolean | null); [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkBodySummary = { "base64": boolean; "bytes": number; "omitted"?: (string | null); };
export type MethodNetworkShowResult__NetworkEntryDetail = { "canceled": boolean; "complete": boolean; "duration_ms": number; "failure"?: (string | null); "hop": number; "id": number; "method": string; "request": MethodNetworkShowResult__NetworkRequestDetail; "request_id": string; "resource_type"?: (string | null); "response": MethodNetworkShowResult__NetworkResponseDetail; "session_id": string; "started_at"?: (number | null); "target_id": string; "url": string; [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkEntryDetail2 = { "canceled": boolean; "complete": boolean; "duration_ms": number; "failure"?: (string | null); "hop": number; "id": number; "method": string; "request": MethodNetworkShowResult__NetworkRequestDetail2; "request_id": string; "resource_type"?: (string | null); "response": MethodNetworkShowResult__NetworkResponseDetail2; "session_id": string; "started_at"?: (number | null); "target_id": string; "url": string; [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkRequestDetail = { "headers": Record<string, string>; "headers_text"?: (string | null); "post_data"?: (MethodNetworkShowResult__NetworkBodySummary | null); [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkRequestDetail2 = { "headers": Record<string, string>; "headers_text"?: (string | null); "post_data"?: (MethodNetworkShowResult__NetworkBody | null); [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkResponseDetail = { "body"?: (MethodNetworkShowResult__NetworkBodySummary | null); "encoded_data_length"?: (number | null); "headers": Record<string, string>; "headers_text"?: (string | null); "mime_type"?: (string | null); "protocol"?: (string | null); "redirect_url"?: (string | null); "remote_ip"?: (string | null); "remote_port"?: (number | null); "status"?: (number | null); "status_text"?: (string | null); "timing"?: (Record<string, number> | null); [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkResponseDetail2 = { "body"?: (MethodNetworkShowResult__NetworkBody | null); "encoded_data_length"?: (number | null); "headers": Record<string, string>; "headers_text"?: (string | null); "mime_type"?: (string | null); "protocol"?: (string | null); "redirect_url"?: (string | null); "remote_ip"?: (string | null); "remote_port"?: (number | null); "status"?: (number | null); "status_text"?: (string | null); "timing"?: (Record<string, number> | null); [key: string]: unknown; };
export type MethodNetworkShowResult__NetworkShowEntry = (MethodNetworkShowResult__NetworkEntryDetail | MethodNetworkShowResult__NetworkEntryDetail2);

export type MethodNetworkWaitParams = { "include_existing"?: boolean; "method"?: (string | null); "pageId": string; "pattern"?: (string | null); "since"?: (number | null); "status"?: (number | null); "timeout"?: (number | null); [key: string]: unknown; };

export type MethodNetworkWaitResult = { "entry": MethodNetworkWaitResult__NetworkLogEntry; "matched": boolean; "meta": MethodNetworkWaitResult__NetworkMeta; [key: string]: unknown; };
export type MethodNetworkWaitResult__NetworkBodySummary = { "base64": boolean; "bytes": number; "omitted"?: (string | null); };
export type MethodNetworkWaitResult__NetworkLogEntry = { "complete": boolean; "duration_ms": number; "failed"?: (string | null); "id": number; "method": string; "request_body"?: (MethodNetworkWaitResult__NetworkBodySummary | null); "resource_type"?: (string | null); "response_body"?: (MethodNetworkWaitResult__NetworkBodySummary | null); "status"?: (number | null); "url": string; [key: string]: unknown; };
export type MethodNetworkWaitResult__NetworkMeta = { "active": boolean; "body_failed": number; "body_pending": number; "entry_count": number; "in_flight": number; "last_id": number; "namespace": string; "next_since": number; "suggested_commands": Array<string>; "warnings": Array<string>; [key: string]: unknown; };

export type MethodObserveParams = { "filter"?: (string | null); "max"?: (number | null); "pageId": string; "structured"?: boolean; [key: string]: unknown; };

export type MethodObserveResult = (string | MethodObserveResult__StructuredObservation);
export type MethodObserveResult__ElementBox = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };
export type MethodObserveResult__ObservedElement = { "bbox": MethodObserveResult__ElementBox; "checked": boolean; "disabled": boolean; "index": number; "input_type"?: (string | null); "placeholder"?: (string | null); "role"?: (string | null); "selector": string; "tag": string; "text": string; "value"?: (string | null); "visible": boolean; [key: string]: unknown; };
export type MethodObserveResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };
export type MethodObserveResult__StructuredObservation = { "elements": Array<MethodObserveResult__ObservedElement>; "page"?: (MethodObserveResult__PageContext | null); "text": string; [key: string]: unknown; };

export type MethodOpenParams = { "bypass_csp"?: boolean; "expect"?: (MethodOpenParams__Expectation | null); "headers"?: unknown; "inject_js"?: (string | null); "load_state"?: (string | null); "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); "url": string; "user_agent"?: (string | null); [key: string]: unknown; };
export type MethodOpenParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodOpenParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodOpenParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodOpenParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodOpenParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodOpenParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodOpenParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodOpenParams__LocatorQuery; [key: string]: unknown; };

export type MethodOpenResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodOpenResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodOpenResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodOpenResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodOpenResult__OperationState; "status": MethodOpenResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodOpenResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodOpenResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodOpenResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodOpenResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodOpenResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodOpenResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodOpenResult__ErrorContext = { "action_status"?: (MethodOpenResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodOpenResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodOpenResult__ErrorDetail = { "code": string; "context"?: (MethodOpenResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodOpenResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodOpenResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodOpenResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodOperationsCancelParams = { "id": string; [key: string]: unknown; };

export type MethodOperationsCancelResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodOperationsCancelResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodOperationsCancelResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodOperationsCancelResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodOperationsCancelResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodOperationsCancelResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodOperationsCancelResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodOperationsCancelResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodOperationsCancelResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodOperationsCancelResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodOperationsCancelResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodOperationsCancelResult__ErrorContext = { "action_status"?: (MethodOperationsCancelResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodOperationsCancelResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodOperationsCancelResult__ErrorDetail = { "code": string; "context"?: (MethodOperationsCancelResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodOperationsCancelResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodOperationsCancelResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodOperationsGetParams = { "id": string; [key: string]: unknown; };

export type MethodOperationsGetResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodOperationsGetResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodOperationsGetResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodOperationsGetResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodOperationsGetResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodOperationsGetResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodOperationsGetResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodOperationsGetResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodOperationsGetResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodOperationsGetResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodOperationsGetResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodOperationsGetResult__ErrorContext = { "action_status"?: (MethodOperationsGetResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodOperationsGetResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodOperationsGetResult__ErrorDetail = { "code": string; "context"?: (MethodOperationsGetResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodOperationsGetResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodOperationsGetResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodOperationsListParams = { "before_ms"?: (number | null); "limit"?: (number | null); [key: string]: unknown; };

export type MethodOperationsListResult = Array<MethodOperationsListResult__OperationRecord>;
export type MethodOperationsListResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodOperationsListResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodOperationsListResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodOperationsListResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodOperationsListResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodOperationsListResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodOperationsListResult__ErrorContext = { "action_status"?: (MethodOperationsListResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodOperationsListResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodOperationsListResult__ErrorDetail = { "code": string; "context"?: (MethodOperationsListResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodOperationsListResult__OperationRecord = { "action": string; "dispatch_intent": boolean; "error"?: (MethodOperationsListResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodOperationsListResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodOperationsListResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodOperationsListResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodOperationsListResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodOperationsListResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodOperationsWaitParams = { "id": string; "timeout_ms"?: number; [key: string]: unknown; };

export type MethodOperationsWaitResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodOperationsWaitResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodOperationsWaitResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodOperationsWaitResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodOperationsWaitResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodOperationsWaitResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodOperationsWaitResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodOperationsWaitResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodOperationsWaitResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodOperationsWaitResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodOperationsWaitResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodOperationsWaitResult__ErrorContext = { "action_status"?: (MethodOperationsWaitResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodOperationsWaitResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodOperationsWaitResult__ErrorDetail = { "code": string; "context"?: (MethodOperationsWaitResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodOperationsWaitResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodOperationsWaitResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageCaptureStateParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageCaptureStateResult = { "state": MethodPageCaptureStateResult__BrowserState; [key: string]: unknown; };
export type MethodPageCaptureStateResult__BrowserState = { "cookies": Array<MethodPageCaptureStateResult__SessionCookie>; "localStorage"?: Record<string, string>; "sessionStorage"?: Record<string, string>; "url": string; "userAgent": string; [key: string]: unknown; };
export type MethodPageCaptureStateResult__SessionCookie = { "domain": string; "expires"?: (number | null); "http_only": boolean; "name": string; "path": string; "same_site"?: (string | null); "secure": boolean; "value": string; [key: string]: unknown; };

export type MethodPageRestoreStateParams = { "pageId": string; "state": MethodPageRestoreStateParams__BrowserState; [key: string]: unknown; };
export type MethodPageRestoreStateParams__BrowserState = { "cookies": Array<MethodPageRestoreStateParams__SessionCookie>; "localStorage"?: Record<string, string>; "sessionStorage"?: Record<string, string>; "url": string; "userAgent": string; [key: string]: unknown; };
export type MethodPageRestoreStateParams__SessionCookie = { "domain": string; "expires"?: (number | null); "http_only": boolean; "name": string; "path": string; "same_site"?: (string | null); "secure": boolean; "value": string; [key: string]: unknown; };

export type MethodPageRestoreStateResult = Record<string, never>;

export type MethodPageSolveCaptchaParams = { "apiDomain"?: (string | null); "apiKey": string; "callback"?: (string | null); "captchaMode": string; "captchaType": string; "enterprisePayload"?: unknown; "pageId": string; "websiteKey": string; "websiteURL": string; [key: string]: unknown; };

export type MethodPageSolveCaptchaResult = { "injection": MethodPageSolveCaptchaResult__CaptchaInjection; [key: string]: unknown; };
export type MethodPageSolveCaptchaResult__CaptchaInjection = { "callbacks": Array<string>; "created": Array<string>; "errors": Array<string>; "kind": string; "solverUserAgent"?: (string | null); "updated_count": number; [key: string]: unknown; };

export type MethodRecordingCancelParams = { "recordingId": string; [key: string]: unknown; };

export type MethodRecordingCancelResult = { "artifact"?: (MethodRecordingCancelResult__Artifact | null); "durationMs"?: (number | null); "failure"?: (MethodRecordingCancelResult__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingCancelResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingCancelResult__Artifact = { "bytes": number; "container"?: (MethodRecordingCancelResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingCancelResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingCancelResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type MethodRecordingCancelResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingCancelResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingCancelResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingCancelResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingCancelResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodRecordingGetParams = { "recordingId": string; [key: string]: unknown; };

export type MethodRecordingGetResult = { "artifact"?: (MethodRecordingGetResult__Artifact | null); "durationMs"?: (number | null); "failure"?: (MethodRecordingGetResult__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingGetResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingGetResult__Artifact = { "bytes": number; "container"?: (MethodRecordingGetResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingGetResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingGetResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type MethodRecordingGetResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingGetResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingGetResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingGetResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingGetResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodRecordingStartParams = { "fps"?: (number | null); "maxHeight"?: (number | null); "maxWidth"?: (number | null); "pageId": string; "path": string; "recordingId": string; };

export type MethodRecordingStartResult = { "artifact"?: (MethodRecordingStartResult__Artifact | null); "durationMs"?: (number | null); "failure"?: (MethodRecordingStartResult__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingStartResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingStartResult__Artifact = { "bytes": number; "container"?: (MethodRecordingStartResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingStartResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingStartResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type MethodRecordingStartResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingStartResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingStartResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingStartResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingStartResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodRecordingStopParams = { "recordingId": string; [key: string]: unknown; };

export type MethodRecordingStopResult = { "artifact"?: (MethodRecordingStopResult__Artifact | null); "durationMs"?: (number | null); "failure"?: (MethodRecordingStopResult__RecordingFailure | null); "frames"?: (number | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingStopResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingStopResult__Artifact = { "bytes": number; "container"?: (MethodRecordingStopResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingStopResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingStopResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed" | "DialogOpen" | "NavigationPending" | "PageUnresponsive") & string);
export type MethodRecordingStopResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingStopResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingStopResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingStopResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingStopResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodReleaseAllInputsParams = { "pageId": string; };

export type MethodReleaseAllInputsResult = string;

export type MethodReloadParams = { "pageId": string; };

export type MethodReloadResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodReloadResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodReloadResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodReloadResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodReloadResult__OperationState; "status": MethodReloadResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodReloadResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodReloadResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodReloadResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodReloadResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodReloadResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodReloadResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodReloadResult__ErrorContext = { "action_status"?: (MethodReloadResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodReloadResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodReloadResult__ErrorDetail = { "code": string; "context"?: (MethodReloadResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodReloadResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodReloadResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodReloadResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodSaveStateParams = { "pageId": string; "path": string; [key: string]: unknown; };

export type MethodSaveStateResult = string;

export type MethodScreenshotParams = { "annotate"?: boolean; "clip"?: (MethodScreenshotParams__ScreenshotClip | null); "format"?: (MethodScreenshotParams__ScreenshotFormat | null); "full_page"?: boolean; "inline"?: boolean; "output"?: (string | null); "pageId": string; "quality"?: (number | null); [key: string]: unknown; };
export type MethodScreenshotParams__ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };
export type MethodScreenshotParams__ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type MethodScreenshotResult = (MethodScreenshotResult__InlineScreenshot | MethodScreenshotResult__ScreenshotFile);
export type MethodScreenshotResult__InlineScreenshot = { "dataBase64": string; "elements"?: (string | null); [key: string]: unknown; };
export type MethodScreenshotResult__ScreenshotFile = { "bytes": number; "elements"?: (string | null); "mimeType": string; "path": string; "size": number; [key: string]: unknown; };

export type MethodScrollParams = { "expect"?: (MethodScrollParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodScrollParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodScrollParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodScrollParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodScrollParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodScrollParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodScrollParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodScrollParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodScrollParams__LocatorQuery; [key: string]: unknown; };

export type MethodScrollResult = (string | MethodScrollResult__ActionResult);
export type MethodScrollResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodScrollResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodScrollResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodScrollResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodScrollResult__ActionResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodScrollResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodScrollResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodScrollResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodScrollResult__OperationState; "status": MethodScrollResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodScrollResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodScrollResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodScrollResult__ErrorContext = { "action_status"?: (MethodScrollResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodScrollResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodScrollResult__ErrorDetail = { "code": string; "context"?: (MethodScrollResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodScrollResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodScrollResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodScrollResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodSelectParams = { "expect"?: (MethodSelectParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); "value": string; [key: string]: unknown; };
export type MethodSelectParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodSelectParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodSelectParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodSelectParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodSelectParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodSelectParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodSelectParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodSelectParams__LocatorQuery; [key: string]: unknown; };

export type MethodSelectResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodSelectResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodSelectResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodSelectResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodSelectResult__OperationState; "status": MethodSelectResult__OperationStatus; "target_id": string; [key: string]: unknown; };
export type MethodSelectResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodSelectResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodSelectResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodSelectResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodSelectResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodSelectResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodSelectResult__ErrorContext = { "action_status"?: (MethodSelectResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodSelectResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodSelectResult__ErrorDetail = { "code": string; "context"?: (MethodSelectResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodSelectResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodSelectResult__OperationStatus = (("running" | "verified_complete" | "not_dispatched" | "outcome_unknown") & string);
export type MethodSelectResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodServerHelloParams = { "protocol": MethodServerHelloParams__ProtocolVersion; [key: string]: unknown; };
export type MethodServerHelloParams__ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };

export type MethodServerHelloResult = { "capabilities": Array<string>; "coreVersion": string; "limits": MethodServerHelloResult__TransportLimits; "methods": Array<string>; "protocol": MethodServerHelloResult__ProtocolVersion; "serverVersion": string; [key: string]: unknown; };
export type MethodServerHelloResult__ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };
export type MethodServerHelloResult__TransportLimits = { "maxInFlight": number; "maxMessageBytes": number; [key: string]: unknown; };

export type MethodServerShutdownParams = Record<string, never>;

export type MethodServerShutdownResult = Record<string, never>;

export type MethodSetCookieParams = { "domain"?: (string | null); "name": string; "pageId": string; "path"?: (string | null); "value": string; [key: string]: unknown; };

export type MethodSetCookieResult = string;

export type MethodSetStorageParams = { "key": string; "pageId": string; "session_storage"?: boolean; "value": string; [key: string]: unknown; };

export type MethodSetStorageResult = string;

export type MethodSnapshotParams = { "all"?: boolean; "interactive"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodSnapshotResult = string;

export type MethodSpaInfoParams = { "pageId": string; };

export type MethodSpaInfoResult = { "can_navigate": boolean; "current_path": string; "router_type": MethodSpaInfoResult__SpaRouter; [key: string]: unknown; };
export type MethodSpaInfoResult__SpaRouter = (("ReactRouter" | "NextJs" | "VueRouter" | "AngularRouter" | "HistoryApi" | "Unknown") & string);

export type MethodSpaNavigateParams = { "pageId": string; "path": string; [key: string]: unknown; };

export type MethodSpaNavigateResult = string;

export type MethodStorageParams = { "key"?: (string | null); "pageId": string; "session_storage"?: boolean; [key: string]: unknown; };

export type MethodStorageResult = ((string | null) | Record<string, string>);

export type MethodTextParams = { "pageId": string; };

export type MethodTextResult = string;

export type MethodWaitParams = { "expect"?: (MethodWaitParams__Expectation | null); "load"?: (string | null); "ms"?: (number | null); "network_idle"?: (number | null); "pageId": string; "selector"?: (string | null); "state"?: (MethodWaitParams__WaitSelectorState | null); "text"?: (string | null); "timeout"?: (number | null); "url"?: (string | null); [key: string]: unknown; };
export type MethodWaitParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodWaitParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodWaitParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodWaitParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodWaitParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodWaitParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodWaitParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodWaitParams__LocatorQuery; [key: string]: unknown; };
export type MethodWaitParams__WaitSelectorState = (("attached" & string) | ("visible" & string) | ("hidden" & string));

export type MethodWaitResult = string;

export type MethodWasmFindParams = { "end"?: (string | null); "max"?: number; "memory"?: (string | null); "pageId": string; "pattern": string; "start"?: (string | null); [key: string]: unknown; };

export type MethodWasmFindResult = string;

export type MethodWasmInfoParams = { "pageId": string; };

export type MethodWasmInfoResult = Array<MethodWasmInfoResult__WasmMemory>;
export type MethodWasmInfoResult__WasmMemory = { "pages": number; "path": string; "size": number; "type": string; [key: string]: unknown; };

export type MethodWasmReadParams = { "addr": string; "len": number; "memory"?: (string | null); "pageId": string; [key: string]: unknown; };

export type MethodWasmReadResult = string;

export type MethodWasmWriteParams = { "addr": string; "hex": string; "memory"?: (string | null); "pageId": string; [key: string]: unknown; };

export type MethodWasmWriteResult = string;

export type MethodParams = {
  "actions.perform": MethodActionsPerformParams;
  "agent.observe": MethodAgentObserveParams;
  "agent.step": MethodAgentStepParams;
  "back": MethodBackParams;
  "browser.close": MethodBrowserCloseParams;
  "browser.close_tab": MethodBrowserCloseTabParams;
  "browser.connect": MethodBrowserConnectParams;
  "browser.launch": MethodBrowserLaunchParams;
  "browser.new_page": MethodBrowserNewPageParams;
  "browser.tabs": MethodBrowserTabsParams;
  "captcha.datadome": MethodCaptchaDatadomeParams;
  "captcha.detect": MethodCaptchaDetectParams;
  "captcha.inject": MethodCaptchaInjectParams;
  "captcha.turnstile": MethodCaptchaTurnstileParams;
  "clear_cookies": MethodClearCookiesParams;
  "click": MethodClickParams;
  "click_shadow": MethodClickShadowParams;
  "clone_from": MethodCloneFromParams;
  "console": MethodConsoleParams;
  "content": MethodContentParams;
  "cookies": MethodCookiesParams;
  "delete_cookie": MethodDeleteCookieParams;
  "dialog.accept": MethodDialogAcceptParams;
  "dialog.dismiss": MethodDialogDismissParams;
  "double_click": MethodDoubleClickParams;
  "download.cancel": MethodDownloadCancelParams;
  "download.get": MethodDownloadGetParams;
  "download.list": MethodDownloadListParams;
  "download.start": MethodDownloadStartParams;
  "download.wait": MethodDownloadWaitParams;
  "dump_storage": MethodDumpStorageParams;
  "emulate": MethodEmulateParams;
  "errors": MethodErrorsParams;
  "eval": MethodEvalParams;
  "exec": MethodExecParams;
  "exists": MethodExistsParams;
  "fake_camera": MethodFakeCameraParams;
  "fetch": MethodFetchParams;
  "fill": MethodFillParams;
  "find": MethodFindParams;
  "forward": MethodForwardParams;
  "frame_eval": MethodFrameEvalParams;
  "frames": MethodFramesParams;
  "get_attribute": MethodGetAttributeParams;
  "get_text": MethodGetTextParams;
  "headers": MethodHeadersParams;
  "hover": MethodHoverParams;
  "info": MethodInfoParams;
  "intercept.add": MethodInterceptAddParams;
  "intercept.list": MethodInterceptListParams;
  "intercept.log": MethodInterceptLogParams;
  "intercept.remove": MethodInterceptRemoveParams;
  "js.allow": MethodJsAllowParams;
  "js.block": MethodJsBlockParams;
  "js.list": MethodJsListParams;
  "js.mode": MethodJsModeParams;
  "js.remove": MethodJsRemoveParams;
  "key": MethodKeyParams;
  "key.down": MethodKeyDownParams;
  "key.up": MethodKeyUpParams;
  "load_state": MethodLoadStateParams;
  "mouse.down": MethodMouseDownParams;
  "mouse.move": MethodMouseMoveParams;
  "mouse.up": MethodMouseUpParams;
  "network.clear": MethodNetworkClearParams;
  "network.export": MethodNetworkExportParams;
  "network.log": MethodNetworkLogParams;
  "network.record.start": MethodNetworkRecordStartParams;
  "network.record.status": MethodNetworkRecordStatusParams;
  "network.record.stop": MethodNetworkRecordStopParams;
  "network.show": MethodNetworkShowParams;
  "network.wait": MethodNetworkWaitParams;
  "observe": MethodObserveParams;
  "open": MethodOpenParams;
  "operations.cancel": MethodOperationsCancelParams;
  "operations.get": MethodOperationsGetParams;
  "operations.list": MethodOperationsListParams;
  "operations.wait": MethodOperationsWaitParams;
  "page.capture_state": MethodPageCaptureStateParams;
  "page.restore_state": MethodPageRestoreStateParams;
  "page.solve_captcha": MethodPageSolveCaptchaParams;
  "recording.cancel": MethodRecordingCancelParams;
  "recording.get": MethodRecordingGetParams;
  "recording.start": MethodRecordingStartParams;
  "recording.stop": MethodRecordingStopParams;
  "release_all_inputs": MethodReleaseAllInputsParams;
  "reload": MethodReloadParams;
  "save_state": MethodSaveStateParams;
  "screenshot": MethodScreenshotParams;
  "scroll": MethodScrollParams;
  "select": MethodSelectParams;
  "server.hello": MethodServerHelloParams;
  "server.shutdown": MethodServerShutdownParams;
  "set_cookie": MethodSetCookieParams;
  "set_storage": MethodSetStorageParams;
  "snapshot": MethodSnapshotParams;
  "spa.info": MethodSpaInfoParams;
  "spa.navigate": MethodSpaNavigateParams;
  "storage": MethodStorageParams;
  "text": MethodTextParams;
  "wait": MethodWaitParams;
  "wasm.find": MethodWasmFindParams;
  "wasm.info": MethodWasmInfoParams;
  "wasm.read": MethodWasmReadParams;
  "wasm.write": MethodWasmWriteParams;
};

export type MethodResults = {
  "actions.perform": MethodActionsPerformResult;
  "agent.observe": MethodAgentObserveResult;
  "agent.step": MethodAgentStepResult;
  "back": MethodBackResult;
  "browser.close": MethodBrowserCloseResult;
  "browser.close_tab": MethodBrowserCloseTabResult;
  "browser.connect": MethodBrowserConnectResult;
  "browser.launch": MethodBrowserLaunchResult;
  "browser.new_page": MethodBrowserNewPageResult;
  "browser.tabs": MethodBrowserTabsResult;
  "captcha.datadome": MethodCaptchaDatadomeResult;
  "captcha.detect": MethodCaptchaDetectResult;
  "captcha.inject": MethodCaptchaInjectResult;
  "captcha.turnstile": MethodCaptchaTurnstileResult;
  "clear_cookies": MethodClearCookiesResult;
  "click": MethodClickResult;
  "click_shadow": MethodClickShadowResult;
  "clone_from": MethodCloneFromResult;
  "console": MethodConsoleResult;
  "content": MethodContentResult;
  "cookies": MethodCookiesResult;
  "delete_cookie": MethodDeleteCookieResult;
  "dialog.accept": MethodDialogAcceptResult;
  "dialog.dismiss": MethodDialogDismissResult;
  "double_click": MethodDoubleClickResult;
  "download.cancel": MethodDownloadCancelResult;
  "download.get": MethodDownloadGetResult;
  "download.list": MethodDownloadListResult;
  "download.start": MethodDownloadStartResult;
  "download.wait": MethodDownloadWaitResult;
  "dump_storage": MethodDumpStorageResult;
  "emulate": MethodEmulateResult;
  "errors": MethodErrorsResult;
  "eval": MethodEvalResult;
  "exec": MethodExecResult;
  "exists": MethodExistsResult;
  "fake_camera": MethodFakeCameraResult;
  "fetch": MethodFetchResult;
  "fill": MethodFillResult;
  "find": MethodFindResult;
  "forward": MethodForwardResult;
  "frame_eval": MethodFrameEvalResult;
  "frames": MethodFramesResult;
  "get_attribute": MethodGetAttributeResult;
  "get_text": MethodGetTextResult;
  "headers": MethodHeadersResult;
  "hover": MethodHoverResult;
  "info": MethodInfoResult;
  "intercept.add": MethodInterceptAddResult;
  "intercept.list": MethodInterceptListResult;
  "intercept.log": MethodInterceptLogResult;
  "intercept.remove": MethodInterceptRemoveResult;
  "js.allow": MethodJsAllowResult;
  "js.block": MethodJsBlockResult;
  "js.list": MethodJsListResult;
  "js.mode": MethodJsModeResult;
  "js.remove": MethodJsRemoveResult;
  "key": MethodKeyResult;
  "key.down": MethodKeyDownResult;
  "key.up": MethodKeyUpResult;
  "load_state": MethodLoadStateResult;
  "mouse.down": MethodMouseDownResult;
  "mouse.move": MethodMouseMoveResult;
  "mouse.up": MethodMouseUpResult;
  "network.clear": MethodNetworkClearResult;
  "network.export": MethodNetworkExportResult;
  "network.log": MethodNetworkLogResult;
  "network.record.start": MethodNetworkRecordStartResult;
  "network.record.status": MethodNetworkRecordStatusResult;
  "network.record.stop": MethodNetworkRecordStopResult;
  "network.show": MethodNetworkShowResult;
  "network.wait": MethodNetworkWaitResult;
  "observe": MethodObserveResult;
  "open": MethodOpenResult;
  "operations.cancel": MethodOperationsCancelResult;
  "operations.get": MethodOperationsGetResult;
  "operations.list": MethodOperationsListResult;
  "operations.wait": MethodOperationsWaitResult;
  "page.capture_state": MethodPageCaptureStateResult;
  "page.restore_state": MethodPageRestoreStateResult;
  "page.solve_captcha": MethodPageSolveCaptchaResult;
  "recording.cancel": MethodRecordingCancelResult;
  "recording.get": MethodRecordingGetResult;
  "recording.start": MethodRecordingStartResult;
  "recording.stop": MethodRecordingStopResult;
  "release_all_inputs": MethodReleaseAllInputsResult;
  "reload": MethodReloadResult;
  "save_state": MethodSaveStateResult;
  "screenshot": MethodScreenshotResult;
  "scroll": MethodScrollResult;
  "select": MethodSelectResult;
  "server.hello": MethodServerHelloResult;
  "server.shutdown": MethodServerShutdownResult;
  "set_cookie": MethodSetCookieResult;
  "set_storage": MethodSetStorageResult;
  "snapshot": MethodSnapshotResult;
  "spa.info": MethodSpaInfoResult;
  "spa.navigate": MethodSpaNavigateResult;
  "storage": MethodStorageResult;
  "text": MethodTextResult;
  "wait": MethodWaitResult;
  "wasm.find": MethodWasmFindResult;
  "wasm.info": MethodWasmInfoResult;
  "wasm.read": MethodWasmReadResult;
  "wasm.write": MethodWasmWriteResult;
};

export type MethodName = keyof MethodParams;

export type ProtocolRequest<M extends MethodName = MethodName> = {
  [K in M]: { id: number; method: K; params: MethodParams[K] };
}[M];

export type ProtocolSuccessResponse<M extends MethodName = MethodName> = {
  [K in M]: { id: number; result: MethodResults[K]; error?: never };
}[M];

export type ProtocolErrorResponse = { id: number | null; error: ServerError; result?: never };

export type ProtocolResponse<M extends MethodName = MethodName> = ProtocolSuccessResponse<M> | ProtocolErrorResponse;

export const protocolVersion = {"major":4,"minor":0} as const;
export const methodNames = ["actions.perform","agent.observe","agent.step","back","browser.close","browser.close_tab","browser.connect","browser.launch","browser.new_page","browser.tabs","captcha.datadome","captcha.detect","captcha.inject","captcha.turnstile","clear_cookies","click","click_shadow","clone_from","console","content","cookies","delete_cookie","dialog.accept","dialog.dismiss","double_click","download.cancel","download.get","download.list","download.start","download.wait","dump_storage","emulate","errors","eval","exec","exists","fake_camera","fetch","fill","find","forward","frame_eval","frames","get_attribute","get_text","headers","hover","info","intercept.add","intercept.list","intercept.log","intercept.remove","js.allow","js.block","js.list","js.mode","js.remove","key","key.down","key.up","load_state","mouse.down","mouse.move","mouse.up","network.clear","network.export","network.log","network.record.start","network.record.status","network.record.stop","network.show","network.wait","observe","open","operations.cancel","operations.get","operations.list","operations.wait","page.capture_state","page.restore_state","page.solve_captcha","recording.cancel","recording.get","recording.start","recording.stop","release_all_inputs","reload","save_state","screenshot","scroll","select","server.hello","server.shutdown","set_cookie","set_storage","snapshot","spa.info","spa.navigate","storage","text","wait","wasm.find","wasm.info","wasm.read","wasm.write"] as const;
