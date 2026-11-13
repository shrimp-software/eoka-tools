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

export type ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);

export type ErrorData = { "cdp"?: (ErrorData__CdpErrorDetail | null); "detail"?: (ErrorData__ErrorDetail | null); "operation_id"?: (string | null); "partialPath"?: (string | null); "path"?: (string | null); "receipt"?: (ErrorData__ActionReceipt | null); "recording"?: (ErrorData__RecordingRecord | null); "recordingId"?: (string | null); "source"?: (ErrorData__ServerError | null); [key: string]: unknown; };
export type ErrorData__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type ErrorData__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": ErrorData__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": ErrorData__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type ErrorData__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type ErrorData__Artifact = { "bytes": number; "container"?: (ErrorData__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type ErrorData__ArtifactContainer = ("mp4" & string);
export type ErrorData__CdpErrorDetail = { "code"?: (number | null); "message": string; "method"?: (string | null); [key: string]: unknown; };
export type ErrorData__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type ErrorData__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type ErrorData__ErrorContext = { "action_status"?: (ErrorData__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (ErrorData__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type ErrorData__ErrorDetail = { "code": string; "context"?: (ErrorData__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type ErrorData__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };
export type ErrorData__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": ErrorData__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": ErrorData__RecordingFailureStage; [key: string]: unknown; };
export type ErrorData__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type ErrorData__RecordingRecord = { "artifact"?: (ErrorData__Artifact | null); "failure"?: (ErrorData__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": ErrorData__RecordingState; [key: string]: unknown; };
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

export type ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };

export type RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": RecordingFailure__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": RecordingFailure__RecordingFailureStage; [key: string]: unknown; };
export type RecordingFailure__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type RecordingFailure__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);

export type RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);

export type RecordingIdParams = { "recordingId": string; [key: string]: unknown; };

export type RecordingRecord = { "artifact"?: (RecordingRecord__Artifact | null); "failure"?: (RecordingRecord__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": RecordingRecord__RecordingState; [key: string]: unknown; };
export type RecordingRecord__Artifact = { "bytes": number; "container"?: (RecordingRecord__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type RecordingRecord__ArtifactContainer = ("mp4" & string);
export type RecordingRecord__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
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
export type Response__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type Response__ErrorContext = { "action_status"?: (Response__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (Response__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type Response__ErrorData = { "cdp"?: (Response__CdpErrorDetail | null); "detail"?: (Response__ErrorDetail | null); "operation_id"?: (string | null); "partialPath"?: (string | null); "path"?: (string | null); "receipt"?: (Response__ActionReceipt | null); "recording"?: (Response__RecordingRecord | null); "recordingId"?: (string | null); "source"?: (Response__ServerError | null); [key: string]: unknown; };
export type Response__ErrorDetail = { "code": string; "context"?: (Response__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type Response__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };
export type Response__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": Response__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": Response__RecordingFailureStage; [key: string]: unknown; };
export type Response__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type Response__RecordingRecord = { "artifact"?: (Response__Artifact | null); "failure"?: (Response__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": Response__RecordingState; [key: string]: unknown; };
export type Response__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);
export type Response__ServerError = { "code": Response__ErrorCode; "data"?: (Response__ErrorData | null); "message": string; [key: string]: unknown; };

export type ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };

export type ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type ScreenshotParams = { "clip"?: (ScreenshotParams__ScreenshotClip | null); "format"?: (ScreenshotParams__ScreenshotFormat | null); "fullPage"?: (boolean | null); "pageId": string; "quality"?: (number | null); [key: string]: unknown; };
export type ScreenshotParams__ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };
export type ScreenshotParams__ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type ScreenshotToFileParams = { "clip"?: (ScreenshotToFileParams__ScreenshotClip | null); "format"?: (ScreenshotToFileParams__ScreenshotFormat | null); "fullPage"?: (boolean | null); "pageId": string; "path": string; "quality"?: (number | null); [key: string]: unknown; };
export type ScreenshotToFileParams__ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };
export type ScreenshotToFileParams__ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type ServerError = { "code": ServerError__ErrorCode; "data"?: (ServerError__ErrorData | null); "message": string; [key: string]: unknown; };
export type ServerError__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type ServerError__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": ServerError__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": ServerError__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type ServerError__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type ServerError__Artifact = { "bytes": number; "container"?: (ServerError__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type ServerError__ArtifactContainer = ("mp4" & string);
export type ServerError__CdpErrorDetail = { "code"?: (number | null); "message": string; "method"?: (string | null); [key: string]: unknown; };
export type ServerError__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type ServerError__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type ServerError__ErrorContext = { "action_status"?: (ServerError__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (ServerError__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type ServerError__ErrorData = { "cdp"?: (ServerError__CdpErrorDetail | null); "detail"?: (ServerError__ErrorDetail | null); "operation_id"?: (string | null); "partialPath"?: (string | null); "path"?: (string | null); "receipt"?: (ServerError__ActionReceipt | null); "recording"?: (ServerError__RecordingRecord | null); "recordingId"?: (string | null); "source"?: (ServerError | null); [key: string]: unknown; };
export type ServerError__ErrorDetail = { "code": string; "context"?: (ServerError__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type ServerError__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };
export type ServerError__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": ServerError__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": ServerError__RecordingFailureStage; [key: string]: unknown; };
export type ServerError__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type ServerError__RecordingRecord = { "artifact"?: (ServerError__Artifact | null); "failure"?: (ServerError__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": ServerError__RecordingState; [key: string]: unknown; };
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

export type MethodBackParams = { "pageId": string; };

export type MethodBackResult = unknown;

export type MethodBrowserCloseParams = Record<string, never>;

export type MethodBrowserCloseResult = Record<string, never>;

export type MethodBrowserCloseTabParams = { "pageId": string; [key: string]: unknown; };

export type MethodBrowserCloseTabResult = { "cleanupError"?: (string | null); [key: string]: unknown; };

export type MethodBrowserConnectParams = { "autoSolveCloudflare"?: (MethodBrowserConnectParams__CloudflareAutoSolve | null); "wsEndpoint": string; };
export type MethodBrowserConnectParams__CloudflareAutoSolve = { "intervalMs"?: number; "timeoutMs"?: number; [key: string]: unknown; };

export type MethodBrowserConnectResult = { "browserVersion": string; "ownership": MethodBrowserConnectResult__BrowserOwnership; [key: string]: unknown; };
export type MethodBrowserConnectResult__BrowserOwnership = (("owned" | "borrowed") & string);

export type MethodBrowserLaunchParams = { "aggressiveCdpEvasion"?: boolean; "autoSolveCloudflare"?: (MethodBrowserLaunchParams__CloudflareAutoSolve | null); "deviceScaleFactor": (number | null); "executablePath"?: (string | null); "extraArgs"?: Array<string>; "geoAlign"?: boolean; "headless"?: boolean; "proxy"?: (MethodBrowserLaunchParams__ProxyParams | null); "runtimeDir"?: (string | null); "sandbox"?: boolean; "timezone"?: (string | null); "userAgent"?: (string | null); "userDataDir"?: (string | null); "viewportHeight"?: (number | null); "viewportWidth"?: (number | null); [key: string]: unknown; };
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

export type MethodCaptchaDatadomeResult = unknown;

export type MethodCaptchaDetectParams = { "pageId": string; };

export type MethodCaptchaDetectResult = unknown;

export type MethodCaptchaInjectParams = { "callback"?: (string | null); "captcha_type"?: string; "click_after"?: (string | null); "pageId": string; "token": string; [key: string]: unknown; };

export type MethodCaptchaInjectResult = unknown;

export type MethodCaptchaTurnstileParams = { "pageId": string; "timeout_ms"?: number; };

export type MethodCaptchaTurnstileResult = unknown;

export type MethodClearCookiesParams = { "pageId": string; };

export type MethodClearCookiesResult = unknown;

export type MethodClickParams = { "expect"?: (MethodClickParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodClickParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodClickParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodClickParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodClickParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodClickParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodClickParams__LocatorQuery; [key: string]: unknown; };

export type MethodClickResult = unknown;

export type MethodClickShadowParams = { "frame_id"?: (string | null); "pageId": string; "selector": string; "timeout_ms"?: number; [key: string]: unknown; };

export type MethodClickShadowResult = unknown;

export type MethodCloneFromParams = { "pageId": string; "source": string; "to"?: (string | null); [key: string]: unknown; };

export type MethodCloneFromResult = unknown;

export type MethodConsoleParams = { "clear"?: boolean; "level"?: (string | null); "pageId": string; [key: string]: unknown; };

export type MethodConsoleResult = unknown;

export type MethodContentParams = { "pageId": string; };

export type MethodContentResult = unknown;

export type MethodCookiesParams = { "pageId": string; };

export type MethodCookiesResult = unknown;

export type MethodDeleteCookieParams = { "domain"?: (string | null); "name": string; "pageId": string; [key: string]: unknown; };

export type MethodDeleteCookieResult = unknown;

export type MethodDialogAcceptParams = { "pageId": string; "prompt_text"?: (string | null); [key: string]: unknown; };

export type MethodDialogAcceptResult = unknown;

export type MethodDialogDismissParams = { "pageId": string; };

export type MethodDialogDismissResult = unknown;

export type MethodDoubleClickParams = { "expect"?: (MethodDoubleClickParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodDoubleClickParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodDoubleClickParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodDoubleClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodDoubleClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodDoubleClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodDoubleClickParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodDoubleClickParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodDoubleClickParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodDoubleClickParams__LocatorQuery; [key: string]: unknown; };

export type MethodDoubleClickResult = unknown;

export type MethodDownloadCancelParams = { "id": string; "pageId": string; [key: string]: unknown; };

export type MethodDownloadCancelResult = unknown;

export type MethodDownloadGetParams = { "id": string; "pageId": string; [key: string]: unknown; };

export type MethodDownloadGetResult = unknown;

export type MethodDownloadListParams = { "pageId": string; };

export type MethodDownloadListResult = unknown;

export type MethodDownloadStartParams = { "click"?: (string | null); "expect_bytes"?: (number | null); "operation_id"?: (string | null); "output": string; "pageId": string; "start_timeout_ms"?: number; "url"?: (string | null); [key: string]: unknown; };

export type MethodDownloadStartResult = unknown;

export type MethodDownloadWaitParams = { "id": string; "pageId": string; "timeout_ms"?: number; [key: string]: unknown; };

export type MethodDownloadWaitResult = unknown;

export type MethodDumpStorageParams = { "pageId": string; };

export type MethodDumpStorageResult = unknown;

export type MethodEmulateParams = { "desktop"?: boolean; "dpr"?: number; "height"?: number; "pageId": string; "reset"?: boolean; "width"?: number; [key: string]: unknown; };

export type MethodEmulateResult = unknown;

export type MethodErrorsParams = { "clear"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodErrorsResult = unknown;

export type MethodEvalParams = { "arguments"?: Array<unknown>; "code"?: (string | null); "file"?: (string | null); "max_size"?: (number | null); "no_await"?: boolean; "pageId": string; "raw"?: boolean; [key: string]: unknown; };

export type MethodEvalResult = unknown;

export type MethodExecParams = { "arguments"?: Array<unknown>; "code"?: (string | null); "file"?: (string | null); "max_size"?: (number | null); "no_await"?: boolean; "pageId": string; "raw"?: boolean; [key: string]: unknown; };

export type MethodExecResult = unknown;

export type MethodExistsParams = { "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodExistsResult = unknown;

export type MethodFakeCameraParams = { "file": string; "loop_video"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodFakeCameraResult = unknown;

export type MethodFetchParams = { "body"?: (string | null); "body_only"?: boolean; "headers"?: unknown; "max_body"?: (number | null); "method"?: (string | null); "pageId": string; "redirect"?: (string | null); "url": string; [key: string]: unknown; };

export type MethodFetchResult = unknown;

export type MethodFillParams = { "expect"?: (MethodFillParams__Expectation | null); "input_mode"?: MethodFillParams__InputMode; "operation_id"?: (string | null); "pageId": string; "target": string; "text": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodFillParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodFillParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodFillParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodFillParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodFillParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodFillParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodFillParams__LocatorQuery; [key: string]: unknown; };

export type MethodFillResult = unknown;

export type MethodFindParams = { "pageId": string; "text": string; [key: string]: unknown; };

export type MethodFindResult = unknown;

export type MethodForwardParams = { "pageId": string; };

export type MethodForwardResult = unknown;

export type MethodFrameEvalParams = { "code"?: (string | null); "file"?: (string | null); "frame": string; "pageId": string; [key: string]: unknown; };

export type MethodFrameEvalResult = unknown;

export type MethodFramesParams = { "pageId": string; };

export type MethodFramesResult = unknown;

export type MethodGetAttributeParams = { "name": string; "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodGetAttributeResult = unknown;

export type MethodGetTextParams = { "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodGetTextResult = unknown;

export type MethodHeadersParams = { "headers_json": string; "pageId": string; [key: string]: unknown; };

export type MethodHeadersResult = unknown;

export type MethodHoverParams = { "expect"?: (MethodHoverParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodHoverParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodHoverParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodHoverParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodHoverParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodHoverParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodHoverParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodHoverParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodHoverParams__LocatorQuery; [key: string]: unknown; };

export type MethodHoverResult = unknown;

export type MethodInfoParams = { "pageId": string; };

export type MethodInfoResult = unknown;

export type MethodInterceptAddParams = { "capture"?: (string | null); "pageId": string; "respond"?: (string | null); "status"?: number; "url_pattern": string; [key: string]: unknown; };

export type MethodInterceptAddResult = unknown;

export type MethodInterceptListParams = { "pageId": string; };

export type MethodInterceptListResult = unknown;

export type MethodInterceptLogParams = { "clear"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodInterceptLogResult = unknown;

export type MethodInterceptRemoveParams = { "id": string; "pageId": string; [key: string]: unknown; };

export type MethodInterceptRemoveResult = unknown;

export type MethodJsAllowParams = { "domain": string; "pageId": string; [key: string]: unknown; };

export type MethodJsAllowResult = unknown;

export type MethodJsBlockParams = { "domain": string; "pageId": string; [key: string]: unknown; };

export type MethodJsBlockResult = unknown;

export type MethodJsListParams = { "pageId": string; };

export type MethodJsListResult = unknown;

export type MethodJsModeParams = { "mode": string; "pageId": string; [key: string]: unknown; };

export type MethodJsModeResult = unknown;

export type MethodJsRemoveParams = { "domain": string; "pageId": string; [key: string]: unknown; };

export type MethodJsRemoveResult = unknown;

export type MethodKeyParams = { "expect"?: (MethodKeyParams__Expectation | null); "key": string; "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodKeyParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodKeyParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodKeyParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodKeyParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodKeyParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodKeyParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodKeyParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodKeyParams__LocatorQuery; [key: string]: unknown; };

export type MethodKeyResult = unknown;

export type MethodKeyDownParams = { "expect"?: (MethodKeyDownParams__Expectation | null); "key": string; "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodKeyDownParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodKeyDownParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodKeyDownParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodKeyDownParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodKeyDownParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodKeyDownParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodKeyDownParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodKeyDownParams__LocatorQuery; [key: string]: unknown; };

export type MethodKeyDownResult = unknown;

export type MethodKeyUpParams = { "expect"?: (MethodKeyUpParams__Expectation | null); "key": string; "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodKeyUpParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodKeyUpParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodKeyUpParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodKeyUpParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodKeyUpParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodKeyUpParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodKeyUpParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodKeyUpParams__LocatorQuery; [key: string]: unknown; };

export type MethodKeyUpResult = unknown;

export type MethodLoadStateParams = { "no_navigate"?: boolean; "pageId": string; "path": string; [key: string]: unknown; };

export type MethodLoadStateResult = unknown;

export type MethodMouseDownParams = { "button": MethodMouseDownParams__MouseButton; "pageId": string; "x": number; "y": number; [key: string]: unknown; };
export type MethodMouseDownParams__MouseButton = (("left" | "middle" | "right" | "back" | "forward") & string);

export type MethodMouseDownResult = unknown;

export type MethodMouseMoveParams = { "pageId": string; "x": number; "y": number; [key: string]: unknown; };

export type MethodMouseMoveResult = unknown;

export type MethodMouseUpParams = { "button": MethodMouseUpParams__MouseButton; "pageId": string; "x": number; "y": number; [key: string]: unknown; };
export type MethodMouseUpParams__MouseButton = (("left" | "middle" | "right" | "back" | "forward") & string);

export type MethodMouseUpResult = unknown;

export type MethodNetworkClearParams = { "pageId": string; };

export type MethodNetworkClearResult = unknown;

export type MethodNetworkExportParams = { "format"?: string; "pageId": string; "path": string; "settle_ms"?: (number | null); [key: string]: unknown; };

export type MethodNetworkExportResult = unknown;

export type MethodNetworkLogParams = { "compact"?: boolean; "limit"?: (number | null); "method"?: (string | null); "pageId": string; "pattern"?: (string | null); "since"?: (number | null); "status"?: (number | null); [key: string]: unknown; };

export type MethodNetworkLogResult = unknown;

export type MethodNetworkRecordStartParams = { "clear"?: boolean; "max_body_bytes"?: number; "no_bodies"?: boolean; "pageId": string; "patterns"?: Array<string>; [key: string]: unknown; };

export type MethodNetworkRecordStartResult = unknown;

export type MethodNetworkRecordStatusParams = { "pageId": string; };

export type MethodNetworkRecordStatusResult = unknown;

export type MethodNetworkRecordStopParams = { "pageId": string; };

export type MethodNetworkRecordStopResult = unknown;

export type MethodNetworkShowParams = { "body"?: boolean; "id": number; "max_body"?: (number | null); "pageId": string; [key: string]: unknown; };

export type MethodNetworkShowResult = unknown;

export type MethodNetworkWaitParams = { "include_existing"?: boolean; "method"?: (string | null); "pageId": string; "pattern"?: (string | null); "since"?: (number | null); "status"?: (number | null); "timeout"?: (number | null); [key: string]: unknown; };

export type MethodNetworkWaitResult = unknown;

export type MethodObserveParams = { "filter"?: (string | null); "max"?: (number | null); "pageId": string; "structured"?: boolean; [key: string]: unknown; };

export type MethodObserveResult = unknown;

export type MethodOpenParams = { "bypass_csp"?: boolean; "expect"?: (MethodOpenParams__Expectation | null); "headers"?: unknown; "inject_js"?: (string | null); "load_state"?: (string | null); "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); "url": string; "user_agent"?: (string | null); [key: string]: unknown; };
export type MethodOpenParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodOpenParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodOpenParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodOpenParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodOpenParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodOpenParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodOpenParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodOpenParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodOpenParams__LocatorQuery; [key: string]: unknown; };

export type MethodOpenResult = unknown;

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

export type MethodPageClickParams = { "operation_id": string; "options"?: MethodPageClickParams__ActionOptions; "pageId": string; "selector": string; [key: string]: unknown; };
export type MethodPageClickParams__ActionOptions = { "destination"?: MethodPageClickParams__Destination; "expect"?: (MethodPageClickParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageClickParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageClickParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageClickParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageClickParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageClickParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageClickParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageClickParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageClickResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageClickResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageClickResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageClickResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageClickResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageClickResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageClickResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageClickResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageClickResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageClickResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageClickResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageClickResult__ErrorContext = { "action_status"?: (MethodPageClickResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageClickResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageClickResult__ErrorDetail = { "code": string; "context"?: (MethodPageClickResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageClickResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageClickResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageClickShadowParams = { "frameId"?: (string | null); "pageId": string; "selector": string; "timeoutMs"?: number; [key: string]: unknown; };

export type MethodPageClickShadowResult = { "frameId": string; "selector": string; "x": number; "y": number; [key: string]: unknown; };

export type MethodPageClickTextParams = { "operation_id": string; "options"?: MethodPageClickTextParams__ActionOptions; "pageId": string; "text": string; [key: string]: unknown; };
export type MethodPageClickTextParams__ActionOptions = { "destination"?: MethodPageClickTextParams__Destination; "expect"?: (MethodPageClickTextParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageClickTextParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageClickTextParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageClickTextParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageClickTextParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageClickTextParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageClickTextParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageClickTextParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageClickTextParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageClickTextParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageClickTextParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageClickTextParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageClickTextParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageClickTextParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageClickTextParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageClickTextResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageClickTextResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageClickTextResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageClickTextResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageClickTextResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageClickTextResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageClickTextResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageClickTextResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageClickTextResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageClickTextResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageClickTextResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageClickTextResult__ErrorContext = { "action_status"?: (MethodPageClickTextResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageClickTextResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageClickTextResult__ErrorDetail = { "code": string; "context"?: (MethodPageClickTextResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageClickTextResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageClickTextResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageCloseParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageCloseResult = Record<string, never>;

export type MethodPageContentParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageContentResult = { "html": string; [key: string]: unknown; };

export type MethodPageEvaluateParams = { "arguments"?: Array<unknown>; "function": string; "pageId": string; [key: string]: unknown; };

export type MethodPageEvaluateResult = { "result": unknown; [key: string]: unknown; };

export type MethodPageEvaluateInFrameParams = { "frameId": string; "js": string; "pageId": string; [key: string]: unknown; };

export type MethodPageEvaluateInFrameResult = { "result": unknown; [key: string]: unknown; };

export type MethodPageEvaluateRawParams = { "js": string; "pageId": string; [key: string]: unknown; };

export type MethodPageEvaluateRawResult = { "result": unknown; [key: string]: unknown; };

export type MethodPageExecuteParams = { "arguments"?: Array<unknown>; "function": string; "pageId": string; [key: string]: unknown; };

export type MethodPageExecuteResult = Record<string, never>;

export type MethodPageExecuteRawParams = { "js": string; "pageId": string; [key: string]: unknown; };

export type MethodPageExecuteRawResult = Record<string, never>;

export type MethodPageExistsParams = { "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodPageExistsResult = { "exists": boolean; [key: string]: unknown; };

export type MethodPageFetchParams = { "body"?: (string | null); "headers"?: Record<string, string>; "method"?: string; "pageId": string; "redirect"?: string; "url": string; [key: string]: unknown; };

export type MethodPageFetchResult = { "body": string; "headers": Record<string, string>; "ok": boolean; "status": number; "url": string; [key: string]: unknown; };

export type MethodPageFetchBoundedParams = { "body"?: (string | null); "headers"?: Record<string, string>; "maxBytes"?: number; "method"?: string; "pageId": string; "redirect"?: string; "url": string; [key: string]: unknown; };

export type MethodPageFetchBoundedResult = { "body": string; "bodyOmitted"?: (string | null); "bytesRead": number; "headers": Record<string, string>; "redirected": boolean; "status": number; "statusText": string; "truncated": boolean; "type": string; "url": string; [key: string]: unknown; };

export type MethodPageFillParams = { "inputMode"?: MethodPageFillParams__InputMode; "operation_id": string; "options"?: MethodPageFillParams__ActionOptions; "pageId": string; "selector": string; "value": string; [key: string]: unknown; };
export type MethodPageFillParams__ActionOptions = { "destination"?: MethodPageFillParams__Destination; "expect"?: (MethodPageFillParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageFillParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageFillParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageFillParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageFillParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageFillParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodPageFillParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageFillParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageFillParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageFillResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageFillResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageFillResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageFillResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageFillResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageFillResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageFillResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageFillResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageFillResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageFillResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageFillResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageFillResult__ErrorContext = { "action_status"?: (MethodPageFillResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageFillResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageFillResult__ErrorDetail = { "code": string; "context"?: (MethodPageFillResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageFillResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageFillResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageFramesParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageFramesResult = Array<MethodPageFramesResult__FrameInfo>;
export type MethodPageFramesResult__FrameInfo = { "id": string; "name"?: (string | null); "url": string; [key: string]: unknown; };

export type MethodPageGetAttributeParams = { "name": string; "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodPageGetAttributeResult = { "value"?: (string | null); [key: string]: unknown; };

export type MethodPageGetTextParams = { "pageId": string; "selector": string; [key: string]: unknown; };

export type MethodPageGetTextResult = { "text": string; [key: string]: unknown; };

export type MethodPageGotoParams = { "operation_id": string; "options"?: MethodPageGotoParams__ActionOptions; "pageId": string; "url": string; [key: string]: unknown; };
export type MethodPageGotoParams__ActionOptions = { "destination"?: MethodPageGotoParams__Destination; "expect"?: (MethodPageGotoParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageGotoParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageGotoParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageGotoParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageGotoParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageGotoParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageGotoParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageGotoParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageGotoParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageGotoParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageGotoParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageGotoParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageGotoParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageGotoParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageGotoParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageGotoResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageGotoResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageGotoResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageGotoResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageGotoResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageGotoResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageGotoResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageGotoResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageGotoResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageGotoResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageGotoResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageGotoResult__ErrorContext = { "action_status"?: (MethodPageGotoResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageGotoResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageGotoResult__ErrorDetail = { "code": string; "context"?: (MethodPageGotoResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageGotoResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageGotoResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageHoverParams = { "operation_id": string; "options"?: MethodPageHoverParams__ActionOptions; "pageId": string; "selector": string; [key: string]: unknown; };
export type MethodPageHoverParams__ActionOptions = { "destination"?: MethodPageHoverParams__Destination; "expect"?: (MethodPageHoverParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageHoverParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageHoverParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageHoverParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageHoverParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageHoverParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageHoverParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageHoverParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageHoverParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageHoverParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageHoverParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageHoverParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageHoverParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageHoverResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageHoverResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageHoverResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageHoverResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageHoverResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageHoverResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageHoverResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageHoverResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageHoverResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageHoverResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageHoverResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageHoverResult__ErrorContext = { "action_status"?: (MethodPageHoverResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageHoverResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageHoverResult__ErrorDetail = { "code": string; "context"?: (MethodPageHoverResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageHoverResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageHoverResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageHumanClickParams = { "operation_id": string; "options"?: MethodPageHumanClickParams__ActionOptions; "pageId": string; "selector": string; [key: string]: unknown; };
export type MethodPageHumanClickParams__ActionOptions = { "destination"?: MethodPageHumanClickParams__Destination; "expect"?: (MethodPageHumanClickParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageHumanClickParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageHumanClickParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageHumanClickParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageHumanClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageHumanClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageHumanClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageHumanClickParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageHumanClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageHumanClickParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageHumanClickParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageHumanClickParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageHumanClickParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageHumanClickParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageHumanClickParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageHumanClickResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageHumanClickResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageHumanClickResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageHumanClickResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageHumanClickResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageHumanClickResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageHumanClickResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageHumanClickResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageHumanClickResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageHumanClickResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageHumanClickResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageHumanClickResult__ErrorContext = { "action_status"?: (MethodPageHumanClickResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageHumanClickResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageHumanClickResult__ErrorDetail = { "code": string; "context"?: (MethodPageHumanClickResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageHumanClickResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageHumanClickResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageHumanClickTextParams = { "operation_id": string; "options"?: MethodPageHumanClickTextParams__ActionOptions; "pageId": string; "text": string; [key: string]: unknown; };
export type MethodPageHumanClickTextParams__ActionOptions = { "destination"?: MethodPageHumanClickTextParams__Destination; "expect"?: (MethodPageHumanClickTextParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageHumanClickTextParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageHumanClickTextParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageHumanClickTextParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageHumanClickTextParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageHumanClickTextParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageHumanClickTextParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageHumanClickTextParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageHumanClickTextResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageHumanClickTextResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageHumanClickTextResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageHumanClickTextResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageHumanClickTextResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageHumanClickTextResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageHumanClickTextResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageHumanClickTextResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageHumanClickTextResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageHumanClickTextResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageHumanClickTextResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageHumanClickTextResult__ErrorContext = { "action_status"?: (MethodPageHumanClickTextResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageHumanClickTextResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageHumanClickTextResult__ErrorDetail = { "code": string; "context"?: (MethodPageHumanClickTextResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageHumanClickTextResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageHumanClickTextResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageHumanFillParams = { "inputMode"?: MethodPageHumanFillParams__InputMode; "operation_id": string; "options"?: MethodPageHumanFillParams__ActionOptions; "pageId": string; "selector": string; "value": string; [key: string]: unknown; };
export type MethodPageHumanFillParams__ActionOptions = { "destination"?: MethodPageHumanFillParams__Destination; "expect"?: (MethodPageHumanFillParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageHumanFillParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageHumanFillParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageHumanFillParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageHumanFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageHumanFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageHumanFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageHumanFillParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageHumanFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageHumanFillParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageHumanFillParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageHumanFillParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageHumanFillParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodPageHumanFillParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageHumanFillParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageHumanFillParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageHumanFillResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageHumanFillResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageHumanFillResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageHumanFillResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageHumanFillResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageHumanFillResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageHumanFillResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageHumanFillResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageHumanFillResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageHumanFillResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageHumanFillResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageHumanFillResult__ErrorContext = { "action_status"?: (MethodPageHumanFillResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageHumanFillResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageHumanFillResult__ErrorDetail = { "code": string; "context"?: (MethodPageHumanFillResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageHumanFillResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageHumanFillResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageKeyDownParams = { "expect"?: (MethodPageKeyDownParams__Expectation | null); "key": string; "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodPageKeyDownParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageKeyDownParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageKeyDownParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageKeyDownParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageKeyDownParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageKeyDownParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageKeyDownParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageKeyDownParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageKeyDownParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageKeyDownResult = Record<string, never>;

export type MethodPageKeyUpParams = { "expect"?: (MethodPageKeyUpParams__Expectation | null); "key": string; "operation_id"?: (string | null); "pageId": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodPageKeyUpParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageKeyUpParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageKeyUpParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageKeyUpParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageKeyUpParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageKeyUpParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageKeyUpParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageKeyUpParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageKeyUpParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageKeyUpResult = Record<string, never>;

export type MethodPageMouseDownParams = { "button": MethodPageMouseDownParams__MouseButton; "pageId": string; "x": number; "y": number; [key: string]: unknown; };
export type MethodPageMouseDownParams__MouseButton = (("left" | "middle" | "right" | "back" | "forward") & string);

export type MethodPageMouseDownResult = Record<string, never>;

export type MethodPageMouseMoveParams = { "pageId": string; "x": number; "y": number; [key: string]: unknown; };

export type MethodPageMouseMoveResult = Record<string, never>;

export type MethodPageMouseUpParams = { "button": MethodPageMouseUpParams__MouseButton; "pageId": string; "x": number; "y": number; [key: string]: unknown; };
export type MethodPageMouseUpParams__MouseButton = (("left" | "middle" | "right" | "back" | "forward") & string);

export type MethodPageMouseUpResult = Record<string, never>;

export type MethodPagePressKeyParams = { "key": string; "operation_id": string; "options"?: MethodPagePressKeyParams__ActionOptions; "pageId": string; [key: string]: unknown; };
export type MethodPagePressKeyParams__ActionOptions = { "destination"?: MethodPagePressKeyParams__Destination; "expect"?: (MethodPagePressKeyParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPagePressKeyParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPagePressKeyParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPagePressKeyParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPagePressKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPagePressKeyParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPagePressKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPagePressKeyParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPagePressKeyParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPagePressKeyParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPagePressKeyParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPagePressKeyParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPagePressKeyParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPagePressKeyParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPagePressKeyParams__LocatorQuery; [key: string]: unknown; };

export type MethodPagePressKeyResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPagePressKeyResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPagePressKeyResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPagePressKeyResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPagePressKeyResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPagePressKeyResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPagePressKeyResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPagePressKeyResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPagePressKeyResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPagePressKeyResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPagePressKeyResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPagePressKeyResult__ErrorContext = { "action_status"?: (MethodPagePressKeyResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPagePressKeyResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPagePressKeyResult__ErrorDetail = { "code": string; "context"?: (MethodPagePressKeyResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPagePressKeyResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPagePressKeyResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageReleaseAllInputsParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageReleaseAllInputsResult = Record<string, never>;

export type MethodPageResetViewportParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageResetViewportResult = Record<string, never>;

export type MethodPageRestoreStateParams = { "pageId": string; "state": MethodPageRestoreStateParams__BrowserState; [key: string]: unknown; };
export type MethodPageRestoreStateParams__BrowserState = { "cookies": Array<MethodPageRestoreStateParams__SessionCookie>; "localStorage"?: Record<string, string>; "sessionStorage"?: Record<string, string>; "url": string; "userAgent": string; [key: string]: unknown; };
export type MethodPageRestoreStateParams__SessionCookie = { "domain": string; "expires"?: (number | null); "http_only": boolean; "name": string; "path": string; "same_site"?: (string | null); "secure": boolean; "value": string; [key: string]: unknown; };

export type MethodPageRestoreStateResult = Record<string, never>;

export type MethodPageScreenshotParams = { "clip"?: (MethodPageScreenshotParams__ScreenshotClip | null); "format"?: (MethodPageScreenshotParams__ScreenshotFormat | null); "fullPage"?: (boolean | null); "pageId": string; "quality"?: (number | null); [key: string]: unknown; };
export type MethodPageScreenshotParams__ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };
export type MethodPageScreenshotParams__ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type MethodPageScreenshotResult = { "dataBase64": string; [key: string]: unknown; };

export type MethodPageScreenshotToFileParams = { "clip"?: (MethodPageScreenshotToFileParams__ScreenshotClip | null); "format"?: (MethodPageScreenshotToFileParams__ScreenshotFormat | null); "fullPage"?: (boolean | null); "pageId": string; "path": string; "quality"?: (number | null); [key: string]: unknown; };
export type MethodPageScreenshotToFileParams__ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };
export type MethodPageScreenshotToFileParams__ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type MethodPageScreenshotToFileResult = { "bytes": number; "container"?: (MethodPageScreenshotToFileResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodPageScreenshotToFileResult__ArtifactContainer = ("mp4" & string);

export type MethodPageSelectParams = { "inputMode"?: MethodPageSelectParams__InputMode; "operation_id": string; "options"?: MethodPageSelectParams__ActionOptions; "pageId": string; "selector": string; "value": string; [key: string]: unknown; };
export type MethodPageSelectParams__ActionOptions = { "destination"?: MethodPageSelectParams__Destination; "expect"?: (MethodPageSelectParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageSelectParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageSelectParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageSelectParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageSelectParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageSelectParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageSelectParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageSelectParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageSelectParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageSelectParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodPageSelectParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageSelectParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageSelectParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageSelectResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageSelectResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageSelectResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageSelectResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageSelectResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageSelectResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageSelectResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageSelectResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageSelectResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageSelectResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageSelectResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageSelectResult__ErrorContext = { "action_status"?: (MethodPageSelectResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageSelectResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageSelectResult__ErrorDetail = { "code": string; "context"?: (MethodPageSelectResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageSelectResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageSelectResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageSetViewportParams = { "deviceScaleFactor"?: number; "height": number; "pageId": string; "width": number; [key: string]: unknown; };

export type MethodPageSetViewportResult = Record<string, never>;

export type MethodPageSolveCaptchaParams = { "apiDomain"?: (string | null); "apiKey": string; "callback"?: (string | null); "captchaMode": string; "captchaType": string; "enterprisePayload"?: unknown; "pageId": string; "websiteKey": string; "websiteURL": string; [key: string]: unknown; };

export type MethodPageSolveCaptchaResult = { "injection": MethodPageSolveCaptchaResult__CaptchaInjection; [key: string]: unknown; };
export type MethodPageSolveCaptchaResult__CaptchaInjection = { "callbacks": Array<string>; "created": Array<string>; "errors": Array<string>; "kind": string; "solverUserAgent"?: (string | null); "updated_count": number; [key: string]: unknown; };

export type MethodPageSolveTurnstileParams = { "pageId": string; "timeoutMs"?: number; [key: string]: unknown; };

export type MethodPageSolveTurnstileResult = { "clicks": number; "elapsedMs": number; [key: string]: unknown; };

export type MethodPageTextParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageTextResult = { "text": string; [key: string]: unknown; };

export type MethodPageTitleParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageTitleResult = { "title": string; [key: string]: unknown; };

export type MethodPageTypeIntoParams = { "inputMode"?: MethodPageTypeIntoParams__InputMode; "operation_id": string; "options"?: MethodPageTypeIntoParams__ActionOptions; "pageId": string; "selector": string; "text": string; [key: string]: unknown; };
export type MethodPageTypeIntoParams__ActionOptions = { "destination"?: MethodPageTypeIntoParams__Destination; "expect"?: (MethodPageTypeIntoParams__Expectation | null); "timeout_ms"?: number; "wait_for_target"?: boolean; [key: string]: unknown; };
export type MethodPageTypeIntoParams__Destination = (("current" & string) | ("new_tab" & string) | ("current_or_new" & string));
export type MethodPageTypeIntoParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodPageTypeIntoParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodPageTypeIntoParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodPageTypeIntoParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodPageTypeIntoParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodPageTypeIntoParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodPageTypeIntoParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodPageTypeIntoParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodPageTypeIntoParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodPageTypeIntoParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodPageTypeIntoParams__InputMode = (("sequential" & string) | ("bulk" & string));
export type MethodPageTypeIntoParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodPageTypeIntoParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodPageTypeIntoParams__LocatorQuery; [key: string]: unknown; };

export type MethodPageTypeIntoResult = { "action": string; "dispatch_intent": boolean; "error"?: (MethodPageTypeIntoResult__ErrorDetail | null); "error_code"?: (string | null); "finished_at_ms"?: (number | null); "id": string; "outcome": MethodPageTypeIntoResult__ActionOutcome; "preparation_intent": boolean; "receipt"?: (MethodPageTypeIntoResult__ActionReceipt | null); "runtime_id": string; "started_at_ms": number; "state": MethodPageTypeIntoResult__OperationState; "target_id": string; [key: string]: unknown; };
export type MethodPageTypeIntoResult__ActionOutcome = (("not_dispatched" & string) | ("outcome_unknown" & string) | ("verified_complete" & string));
export type MethodPageTypeIntoResult__ActionReceipt = { "action": string; "already_satisfied": boolean; "cleanup_pending": boolean; "destination_target_id"?: (string | null); "dispatch": MethodPageTypeIntoResult__DispatchState; "download_guid"?: (string | null); "elapsed_ms": number; "finished_at_ms": number; "outcome": MethodPageTypeIntoResult__ActionOutcome; "preparation_may_have_executed": boolean; "started_at_ms": number; "target_id": string; "verification"?: (string | null); [key: string]: unknown; };
export type MethodPageTypeIntoResult__ActionStatus = (("not_attempted" | "may_have_executed") & string);
export type MethodPageTypeIntoResult__DispatchState = (("not_sent" & string) | ("may_have_been_sent" & string) | ("acknowledged" & string));
export type MethodPageTypeIntoResult__ErrorContext = { "action_status"?: (MethodPageTypeIntoResult__ActionStatus | null); "download_id"?: (string | null); "operation_id"?: (string | null); "page"?: (MethodPageTypeIntoResult__PageContext | null); "phase"?: (string | null); [key: string]: unknown; };
export type MethodPageTypeIntoResult__ErrorDetail = { "code": string; "context"?: (MethodPageTypeIntoResult__ErrorContext | null); "hint"?: (string | null); "message": string; "retryable"?: (boolean | null); [key: string]: unknown; };
export type MethodPageTypeIntoResult__OperationState = (("running" | "finished" | "interrupted" | "expired") & string);
export type MethodPageTypeIntoResult__PageContext = { "headings": Array<string>; "messages": Array<string>; "ready_state": string; "tab_id": string; "text_excerpt": string; "title": string; "truncated": boolean; "url": string; [key: string]: unknown; };

export type MethodPageUrlParams = { "pageId": string; [key: string]: unknown; };

export type MethodPageUrlResult = { "url": string; [key: string]: unknown; };

export type MethodPageWaitForParams = { "pageId": string; "selector": string; "timeoutMs": number; [key: string]: unknown; };

export type MethodPageWaitForResult = Record<string, never>;

export type MethodPageWaitForTextParams = { "pageId": string; "text": string; "timeoutMs": number; [key: string]: unknown; };

export type MethodPageWaitForTextResult = Record<string, never>;

export type MethodPageWaitForVisibleParams = { "pageId": string; "selector": string; "timeoutMs": number; [key: string]: unknown; };

export type MethodPageWaitForVisibleResult = Record<string, never>;

export type MethodRecordingCancelParams = { "recordingId": string; [key: string]: unknown; };

export type MethodRecordingCancelResult = { "artifact"?: (MethodRecordingCancelResult__Artifact | null); "failure"?: (MethodRecordingCancelResult__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingCancelResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingCancelResult__Artifact = { "bytes": number; "container"?: (MethodRecordingCancelResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingCancelResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingCancelResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type MethodRecordingCancelResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingCancelResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingCancelResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingCancelResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingCancelResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodRecordingGetParams = { "recordingId": string; [key: string]: unknown; };

export type MethodRecordingGetResult = { "artifact"?: (MethodRecordingGetResult__Artifact | null); "failure"?: (MethodRecordingGetResult__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingGetResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingGetResult__Artifact = { "bytes": number; "container"?: (MethodRecordingGetResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingGetResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingGetResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type MethodRecordingGetResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingGetResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingGetResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingGetResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingGetResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodRecordingStartParams = { "fps"?: (number | null); "maxHeight"?: (number | null); "maxWidth"?: (number | null); "pageId": string; "path": string; "recordingId": string; };

export type MethodRecordingStartResult = { "artifact"?: (MethodRecordingStartResult__Artifact | null); "failure"?: (MethodRecordingStartResult__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingStartResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingStartResult__Artifact = { "bytes": number; "container"?: (MethodRecordingStartResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingStartResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingStartResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type MethodRecordingStartResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingStartResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingStartResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingStartResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingStartResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodRecordingStopParams = { "recordingId": string; [key: string]: unknown; };

export type MethodRecordingStopResult = { "artifact"?: (MethodRecordingStopResult__Artifact | null); "failure"?: (MethodRecordingStopResult__RecordingFailure | null); "pageId": string; "path": string; "recordingId": string; "state": MethodRecordingStopResult__RecordingState; [key: string]: unknown; };
export type MethodRecordingStopResult__Artifact = { "bytes": number; "container"?: (MethodRecordingStopResult__ArtifactContainer | null); "mimeType": string; "path": string; [key: string]: unknown; };
export type MethodRecordingStopResult__ArtifactContainer = ("mp4" & string);
export type MethodRecordingStopResult__ErrorCode = (("ElementNotFound" | "ElementNotVisible" | "Timeout" | "RetryExhausted" | "Cdp" | "InvalidPage" | "InvalidParams" | "UnknownMethod" | "Internal" | "ProtocolMismatch" | "MessageTooLarge" | "ServerBusy" | "ShuttingDown" | "Cancelled" | "Action" | "Operation" | "ScreenshotTooLarge" | "FileOutput" | "RecordingUnsupported" | "RecordingNotFound" | "RecordingConflict" | "RecordingAlreadyActive" | "RecordingInvalidState" | "RecordingFailed") & string);
export type MethodRecordingStopResult__RecordingFailure = { "cleanupUncertain"?: (boolean | null); "code": MethodRecordingStopResult__ErrorCode; "message": string; "partialPath"?: (string | null); "stage": MethodRecordingStopResult__RecordingFailureStage; [key: string]: unknown; };
export type MethodRecordingStopResult__RecordingFailureStage = (("start" | "stop" | "read" | "write" | "flush" | "close" | "publish" | "cancel" | "disconnect") & string);
export type MethodRecordingStopResult__RecordingState = (("running" | "finalizing" | "completed" | "cancelled" | "failed") & string);

export type MethodReleaseAllInputsParams = { "pageId": string; };

export type MethodReleaseAllInputsResult = unknown;

export type MethodReloadParams = { "pageId": string; };

export type MethodReloadResult = unknown;

export type MethodSaveStateParams = { "pageId": string; "path": string; [key: string]: unknown; };

export type MethodSaveStateResult = unknown;

export type MethodScreenshotParams = { "annotate"?: boolean; "clip"?: (MethodScreenshotParams__ScreenshotClip | null); "format"?: (MethodScreenshotParams__ScreenshotFormat | null); "full_page"?: boolean; "output"?: (string | null); "pageId": string; "quality"?: (number | null); [key: string]: unknown; };
export type MethodScreenshotParams__ScreenshotClip = { "height": number; "width": number; "x": number; "y": number; [key: string]: unknown; };
export type MethodScreenshotParams__ScreenshotFormat = (("png" | "jpeg" | "webp") & string);

export type MethodScreenshotResult = unknown;

export type MethodScrollParams = { "expect"?: (MethodScrollParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); [key: string]: unknown; };
export type MethodScrollParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodScrollParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodScrollParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodScrollParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodScrollParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodScrollParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodScrollParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodScrollParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodScrollParams__LocatorQuery; [key: string]: unknown; };

export type MethodScrollResult = unknown;

export type MethodSelectParams = { "expect"?: (MethodSelectParams__Expectation | null); "operation_id"?: (string | null); "pageId": string; "target": string; "timeout_ms"?: (number | null); "value": string; [key: string]: unknown; };
export type MethodSelectParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodSelectParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodSelectParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodSelectParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodSelectParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodSelectParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodSelectParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodSelectParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodSelectParams__LocatorQuery; [key: string]: unknown; };

export type MethodSelectResult = unknown;

export type MethodServerHelloParams = { "protocol": MethodServerHelloParams__ProtocolVersion; [key: string]: unknown; };
export type MethodServerHelloParams__ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };

export type MethodServerHelloResult = { "capabilities": Array<string>; "coreVersion": string; "limits": MethodServerHelloResult__TransportLimits; "methods": Array<string>; "protocol": MethodServerHelloResult__ProtocolVersion; "serverVersion": string; [key: string]: unknown; };
export type MethodServerHelloResult__ProtocolVersion = { "major": number; "minor": number; [key: string]: unknown; };
export type MethodServerHelloResult__TransportLimits = { "maxInFlight": number; "maxMessageBytes": number; [key: string]: unknown; };

export type MethodServerShutdownParams = Record<string, never>;

export type MethodServerShutdownResult = Record<string, never>;

export type MethodSetCookieParams = { "domain"?: (string | null); "name": string; "pageId": string; "path"?: (string | null); "value": string; [key: string]: unknown; };

export type MethodSetCookieResult = unknown;

export type MethodSetStorageParams = { "key": string; "pageId": string; "session_storage"?: boolean; "value": string; [key: string]: unknown; };

export type MethodSetStorageResult = unknown;

export type MethodSnapshotParams = { "all"?: boolean; "interactive"?: boolean; "pageId": string; [key: string]: unknown; };

export type MethodSnapshotResult = unknown;

export type MethodSpaInfoParams = { "pageId": string; };

export type MethodSpaInfoResult = unknown;

export type MethodSpaNavigateParams = { "pageId": string; "path": string; [key: string]: unknown; };

export type MethodSpaNavigateResult = unknown;

export type MethodStorageParams = { "key"?: (string | null); "pageId": string; "session_storage"?: boolean; [key: string]: unknown; };

export type MethodStorageResult = unknown;

export type MethodTextParams = { "pageId": string; };

export type MethodTextResult = unknown;

export type MethodWaitParams = { "expect"?: (MethodWaitParams__Expectation | null); "load"?: (string | null); "ms"?: (number | null); "pageId": string; "selector"?: (string | null); "state"?: (MethodWaitParams__WaitSelectorState | null); "text"?: (string | null); "timeout"?: (number | null); "url"?: (string | null); [key: string]: unknown; };
export type MethodWaitParams__Expectation = ({ "kind": ("document_ready" & string); [key: string]: unknown; } | { "conditions": Array<MethodWaitParams__Expectation>; "kind": ("all" & string); [key: string]: unknown; } | { "enabled": boolean; "kind": ("enabled" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("download_started" & string); [key: string]: unknown; } | { "kind": ("focused" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("url" & string); "value": string; [key: string]: unknown; } | { "kind": ("visible" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("hidden" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "contains"?: boolean; "kind": ("text" & string); "target": MethodWaitParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "kind": ("value" & string); "target": MethodWaitParams__LocatorSpec; "value": string; [key: string]: unknown; } | { "checked": boolean; "kind": ("checked" & string); "target": MethodWaitParams__LocatorSpec; [key: string]: unknown; } | { "kind": ("selected" & string); "target": MethodWaitParams__LocatorSpec; "values": Array<string>; [key: string]: unknown; });
export type MethodWaitParams__LocatorQuery = ({ "id": number; "kind": ("node" & string); [key: string]: unknown; } | { "kind": ("css" & string); "selector": string; [key: string]: unknown; } | { "exact"?: boolean; "kind": ("text" & string); "text": string; [key: string]: unknown; } | { "kind": ("placeholder" & string); "text": string; [key: string]: unknown; } | { "kind": ("role" & string); "role": string; [key: string]: unknown; } | { "id": number; "kind": ("backend_node" & string); [key: string]: unknown; });
export type MethodWaitParams__LocatorSpec = { "frame_id"?: (string | null); "index"?: (number | null); "query": MethodWaitParams__LocatorQuery; [key: string]: unknown; };
export type MethodWaitParams__WaitSelectorState = (("attached" & string) | ("visible" & string) | ("hidden" & string));

export type MethodWaitResult = unknown;

export type MethodWasmFindParams = { "end"?: (string | null); "max"?: number; "memory"?: (string | null); "pageId": string; "pattern": string; "start"?: (string | null); [key: string]: unknown; };

export type MethodWasmFindResult = unknown;

export type MethodWasmInfoParams = { "pageId": string; };

export type MethodWasmInfoResult = unknown;

export type MethodWasmReadParams = { "addr": string; "len": number; "memory"?: (string | null); "pageId": string; [key: string]: unknown; };

export type MethodWasmReadResult = unknown;

export type MethodWasmWriteParams = { "addr": string; "hex": string; "memory"?: (string | null); "pageId": string; [key: string]: unknown; };

export type MethodWasmWriteResult = unknown;

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
  "page.click": MethodPageClickParams;
  "page.click_shadow": MethodPageClickShadowParams;
  "page.click_text": MethodPageClickTextParams;
  "page.close": MethodPageCloseParams;
  "page.content": MethodPageContentParams;
  "page.evaluate": MethodPageEvaluateParams;
  "page.evaluate_in_frame": MethodPageEvaluateInFrameParams;
  "page.evaluate_raw": MethodPageEvaluateRawParams;
  "page.execute": MethodPageExecuteParams;
  "page.execute_raw": MethodPageExecuteRawParams;
  "page.exists": MethodPageExistsParams;
  "page.fetch": MethodPageFetchParams;
  "page.fetch_bounded": MethodPageFetchBoundedParams;
  "page.fill": MethodPageFillParams;
  "page.frames": MethodPageFramesParams;
  "page.get_attribute": MethodPageGetAttributeParams;
  "page.get_text": MethodPageGetTextParams;
  "page.goto": MethodPageGotoParams;
  "page.hover": MethodPageHoverParams;
  "page.human_click": MethodPageHumanClickParams;
  "page.human_click_text": MethodPageHumanClickTextParams;
  "page.human_fill": MethodPageHumanFillParams;
  "page.key_down": MethodPageKeyDownParams;
  "page.key_up": MethodPageKeyUpParams;
  "page.mouse_down": MethodPageMouseDownParams;
  "page.mouse_move": MethodPageMouseMoveParams;
  "page.mouse_up": MethodPageMouseUpParams;
  "page.press_key": MethodPagePressKeyParams;
  "page.release_all_inputs": MethodPageReleaseAllInputsParams;
  "page.reset_viewport": MethodPageResetViewportParams;
  "page.restore_state": MethodPageRestoreStateParams;
  "page.screenshot": MethodPageScreenshotParams;
  "page.screenshot_to_file": MethodPageScreenshotToFileParams;
  "page.select": MethodPageSelectParams;
  "page.set_viewport": MethodPageSetViewportParams;
  "page.solve_captcha": MethodPageSolveCaptchaParams;
  "page.solve_turnstile": MethodPageSolveTurnstileParams;
  "page.text": MethodPageTextParams;
  "page.title": MethodPageTitleParams;
  "page.type_into": MethodPageTypeIntoParams;
  "page.url": MethodPageUrlParams;
  "page.wait_for": MethodPageWaitForParams;
  "page.wait_for_text": MethodPageWaitForTextParams;
  "page.wait_for_visible": MethodPageWaitForVisibleParams;
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
  "page.click": MethodPageClickResult;
  "page.click_shadow": MethodPageClickShadowResult;
  "page.click_text": MethodPageClickTextResult;
  "page.close": MethodPageCloseResult;
  "page.content": MethodPageContentResult;
  "page.evaluate": MethodPageEvaluateResult;
  "page.evaluate_in_frame": MethodPageEvaluateInFrameResult;
  "page.evaluate_raw": MethodPageEvaluateRawResult;
  "page.execute": MethodPageExecuteResult;
  "page.execute_raw": MethodPageExecuteRawResult;
  "page.exists": MethodPageExistsResult;
  "page.fetch": MethodPageFetchResult;
  "page.fetch_bounded": MethodPageFetchBoundedResult;
  "page.fill": MethodPageFillResult;
  "page.frames": MethodPageFramesResult;
  "page.get_attribute": MethodPageGetAttributeResult;
  "page.get_text": MethodPageGetTextResult;
  "page.goto": MethodPageGotoResult;
  "page.hover": MethodPageHoverResult;
  "page.human_click": MethodPageHumanClickResult;
  "page.human_click_text": MethodPageHumanClickTextResult;
  "page.human_fill": MethodPageHumanFillResult;
  "page.key_down": MethodPageKeyDownResult;
  "page.key_up": MethodPageKeyUpResult;
  "page.mouse_down": MethodPageMouseDownResult;
  "page.mouse_move": MethodPageMouseMoveResult;
  "page.mouse_up": MethodPageMouseUpResult;
  "page.press_key": MethodPagePressKeyResult;
  "page.release_all_inputs": MethodPageReleaseAllInputsResult;
  "page.reset_viewport": MethodPageResetViewportResult;
  "page.restore_state": MethodPageRestoreStateResult;
  "page.screenshot": MethodPageScreenshotResult;
  "page.screenshot_to_file": MethodPageScreenshotToFileResult;
  "page.select": MethodPageSelectResult;
  "page.set_viewport": MethodPageSetViewportResult;
  "page.solve_captcha": MethodPageSolveCaptchaResult;
  "page.solve_turnstile": MethodPageSolveTurnstileResult;
  "page.text": MethodPageTextResult;
  "page.title": MethodPageTitleResult;
  "page.type_into": MethodPageTypeIntoResult;
  "page.url": MethodPageUrlResult;
  "page.wait_for": MethodPageWaitForResult;
  "page.wait_for_text": MethodPageWaitForTextResult;
  "page.wait_for_visible": MethodPageWaitForVisibleResult;
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

export const protocolVersion = {"major":2,"minor":1} as const;
export const methodNames = ["actions.perform","agent.observe","agent.step","back","browser.close","browser.close_tab","browser.connect","browser.launch","browser.new_page","browser.tabs","captcha.datadome","captcha.detect","captcha.inject","captcha.turnstile","clear_cookies","click","click_shadow","clone_from","console","content","cookies","delete_cookie","dialog.accept","dialog.dismiss","double_click","download.cancel","download.get","download.list","download.start","download.wait","dump_storage","emulate","errors","eval","exec","exists","fake_camera","fetch","fill","find","forward","frame_eval","frames","get_attribute","get_text","headers","hover","info","intercept.add","intercept.list","intercept.log","intercept.remove","js.allow","js.block","js.list","js.mode","js.remove","key","key.down","key.up","load_state","mouse.down","mouse.move","mouse.up","network.clear","network.export","network.log","network.record.start","network.record.status","network.record.stop","network.show","network.wait","observe","open","operations.cancel","operations.get","operations.list","operations.wait","page.capture_state","page.click","page.click_shadow","page.click_text","page.close","page.content","page.evaluate","page.evaluate_in_frame","page.evaluate_raw","page.execute","page.execute_raw","page.exists","page.fetch","page.fetch_bounded","page.fill","page.frames","page.get_attribute","page.get_text","page.goto","page.hover","page.human_click","page.human_click_text","page.human_fill","page.key_down","page.key_up","page.mouse_down","page.mouse_move","page.mouse_up","page.press_key","page.release_all_inputs","page.reset_viewport","page.restore_state","page.screenshot","page.screenshot_to_file","page.select","page.set_viewport","page.solve_captcha","page.solve_turnstile","page.text","page.title","page.type_into","page.url","page.wait_for","page.wait_for_text","page.wait_for_visible","recording.cancel","recording.get","recording.start","recording.stop","release_all_inputs","reload","save_state","screenshot","scroll","select","server.hello","server.shutdown","set_cookie","set_storage","snapshot","spa.info","spa.navigate","storage","text","wait","wasm.find","wasm.info","wasm.read","wasm.write"] as const;
