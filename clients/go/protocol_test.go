package eoka

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"sync/atomic"
	"testing"
)

func TestHelloAcceptsCompatibleMinorAndNegotiatesLimits(t *testing.T) {
	info := fakeServerInfo()
	info.Protocol.Minor = 4
	info.Limits = TransportLimits{MaxMessageBytes: 1024, MaxInFlight: 2}
	info.Capabilities = append(info.Capabilities, "futureCapability")
	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method != "server.hello" {
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
		var request struct {
			Protocol ProtocolVersion `json:"protocol"`
		}
		if err := json.Unmarshal(params, &request); err != nil || request.Protocol != (ProtocolVersion{Major: 1, Minor: 1}) {
			t.Errorf("hello params = %s, error = %v", params, err)
		}
		return info, nil
	})
	b := &Browser{t: tr}
	if err := b.hello(callContext(t)); err != nil {
		t.Fatal(err)
	}
	if tr.messageLimit != 1024 || tr.inFlightLimit != 2 || !tr.capabilities["futureCapability"] {
		t.Fatalf("negotiated limits/capabilities lost: %+v", b.ServerInfo())
	}
	got := b.ServerInfo()
	got.Methods[0] = "changed"
	got.Capabilities[0] = "changed"
	if b.ServerInfo().Methods[0] == "changed" || b.ServerInfo().Capabilities[0] == "changed" {
		t.Fatal("ServerInfo exposed mutable negotiation state")
	}
}

func TestProtocol13PreservesLegacyScreenshotAndRecordingErrorEvidence(t *testing.T) {
	info := fakeServerInfo()
	info.Protocol.Minor = 3
	info.Capabilities = append(info.Capabilities, "screenshotOptions", "screenshotFile", "nativeRecording")
	info.Methods = append(info.Methods, "recording.get")
	evidence := json.RawMessage(`{"recordingId":"b9b87b7a-cc6d-4a25-b098-73271cc9c4cb","recording":{"recordingId":"b9b87b7a-cc6d-4a25-b098-73271cc9c4cb","pageId":"p","state":"failed","path":"/tmp/capture.mp4","failure":{"stage":"write","code":"RecordingFailed","message":"disk full","partialPath":"/tmp/.capture.partial","cleanupUncertain":true}}}`)
	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "server.hello":
			return info, nil
		case "page.screenshot":
			var request map[string]any
			if err := json.Unmarshal(params, &request); err != nil || request["pageId"] != "p" || len(request) != 1 {
				t.Errorf("legacy screenshot params = %s, error = %v", params, err)
			}
			return map[string]any{"dataBase64": "AAEC"}, nil
		case "recording.get":
			return nil, &rpcError{Code: ErrCodeRecordingFailed, Message: "disk full", Data: evidence}
		default:
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
	})
	b := &Browser{t: tr}
	ctx := callContext(t)
	if err := b.hello(ctx); err != nil {
		t.Fatal(err)
	}
	p := &Page{b: b, id: "p"}
	data, err := p.Screenshot(ctx)
	if err != nil || string(data) != string([]byte{0, 1, 2}) {
		t.Fatalf("legacy screenshot = %v, error = %v", data, err)
	}
	err = tr.call(ctx, "recording.get", map[string]any{"recordingId": "b9b87b7a-cc6d-4a25-b098-73271cc9c4cb"}, nil)
	var typed *Error
	if !errors.As(err, &typed) || typed.Code != ErrCodeRecordingFailed || string(typed.Data) != string(evidence) {
		t.Fatalf("recording failure evidence lost: %v", err)
	}
}

func TestHelloNeverRaisesLocalTransportCeilings(t *testing.T) {
	info := fakeServerInfo()
	info.Limits = TransportLimits{MaxMessageBytes: maxMessageBytes * 2, MaxInFlight: maxInFlight * 2}
	b := &Browser{t: newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) { return info, nil })}
	if err := b.hello(callContext(t)); err != nil {
		t.Fatal(err)
	}
	if b.t.messageLimit != maxMessageBytes || b.t.inFlightLimit != maxInFlight {
		t.Fatal("server raised local transport safety ceilings")
	}
}

func TestHelloRejectsIncompatibleOrIncompleteServers(t *testing.T) {
	cases := map[string]func(*ServerInfo){
		"major mismatch":          func(info *ServerInfo) { info.Protocol.Major = 2 },
		"old minor":               func(info *ServerInfo) { info.Protocol.Minor = 0 },
		"no server version":       func(info *ServerInfo) { info.ServerVersion = "" },
		"no core version":         func(info *ServerInfo) { info.CoreVersion = "" },
		"invalid message bound":   func(info *ServerInfo) { info.Limits.MaxMessageBytes = 0 },
		"invalid in-flight bound": func(info *ServerInfo) { info.Limits.MaxInFlight = -1 },
		"no methods":              func(info *ServerInfo) { info.Methods = nil },
		"no shutdown method":      func(info *ServerInfo) { info.Methods = []string{"server.hello", "browser.launch"} },
		"no capabilities":         func(info *ServerInfo) { info.Capabilities = nil },
		"no graceful shutdown":    func(info *ServerInfo) { info.Capabilities = []string{"durableActions"} },
	}
	for name, mutate := range cases {
		t.Run(name, func(t *testing.T) {
			info := fakeServerInfo()
			mutate(&info)
			b := &Browser{t: newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) { return info, nil })}
			if err := b.hello(callContext(t)); !HasCode(err, ErrCodeProtocolMismatch) {
				t.Fatalf("incompatible hello error = %v", err)
			}
		})
	}
	for _, raw := range []json.RawMessage{json.RawMessage(`null`), json.RawMessage(`{"protocol":{"major":1}}`), json.RawMessage(`{"protocol":{"minor":0}}`)} {
		b := &Browser{t: newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) { return raw, nil })}
		if err := b.hello(callContext(t)); !HasCode(err, ErrCodeProtocolMismatch) {
			t.Fatalf("incomplete protocol error = %v", err)
		}
	}
}

func TestHelloReportsPinnedLegacyServerIncompatibility(t *testing.T) {
	data := json.RawMessage(`{"supported":{"major":0,"minor":3}}`)
	for _, code := range []string{ErrCodeUnknownMethod, ErrCodeProtocolMismatch} {
		b := &Browser{t: newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
			return nil, &rpcError{Code: code, Message: "legacy server", Data: data}
		})}
		err := b.hello(callContext(t))
		var typed *Error
		if !errors.As(err, &typed) || typed.Code != ErrCodeProtocolMismatch || string(typed.Data) != string(data) || !strings.Contains(err.Error(), "WithServerPath") || !strings.Contains(err.Error(), "0.3.0") {
			t.Fatalf("legacy compatibility error = %v", err)
		}
	}
}

func TestCapabilitiesAndMethodListPreventUnsupportedDispatch(t *testing.T) {
	info := fakeServerInfo()
	info.Capabilities = []string{"gracefulShutdown"}
	info.Methods = []string{"server.hello", "browser.launch", "server.shutdown", "actions.perform", "operations.cancel"}
	var dispatched atomic.Int32
	b := &Browser{t: newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method == "server.hello" {
			return info, nil
		}
		dispatched.Add(1)
		return nil, nil
	})}
	ctx := callContext(t)
	if err := b.hello(ctx); err != nil {
		t.Fatal(err)
	}
	page := &Page{id: "P1", b: b}
	_, err := page.Perform(ctx, "op1", map[string]any{"kind": "click"}, ActionOptions{})
	var operationErr *OperationError
	if !HasCode(err, ErrCodeCapabilityUnavailable) || !errors.As(err, &operationErr) || operationErr.OperationID != "op1" || operationErr.Uncertain {
		t.Fatalf("unsupported durable action error = %v", err)
	}
	if _, err := b.CancelOperation(ctx, "op1"); !HasCode(err, ErrCodeCapabilityUnavailable) {
		t.Fatalf("unsupported cancellation error = %v", err)
	}
	if _, err := b.Operation(ctx, "op1"); !HasCode(err, ErrCodeCapabilityUnavailable) {
		t.Fatalf("unsupported recovery error = %v", err)
	}
	if _, err := b.Tabs(ctx); !HasCode(err, ErrCodeUnknownMethod) {
		t.Fatalf("unadvertised method error = %v", err)
	}
	if dispatched.Load() != 0 {
		t.Fatal("unsupported operation reached the server")
	}
}

func TestPerformCancellationRetainsIDAndAllowsRecoveryWithoutReplay(t *testing.T) {
	started := make(chan struct{})
	release := make(chan struct{})
	t.Cleanup(func() { close(release) })
	var performs atomic.Int32
	b := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "actions.perform":
			performs.Add(1)
			close(started)
			<-release
			return OperationRecord{ID: "op1", State: "finished", Outcome: "verified_complete"}, nil
		case "operations.get":
			return OperationRecord{ID: "op1", State: "finished", Outcome: "verified_complete", Receipt: json.RawMessage(`{"outcome":"verified_complete"}`)}, nil
		default:
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
	})
	page := &Page{id: "P1", b: b}
	ctx, cancel := context.WithCancel(callContext(t))
	done := make(chan error, 1)
	go func() {
		_, err := page.Perform(ctx, "op1", map[string]any{"kind": "click"}, ActionOptions{})
		done <- err
	}()
	<-started
	cancel()
	err := <-done
	var operationErr *OperationError
	if !errors.Is(err, context.Canceled) || !errors.As(err, &operationErr) || operationErr.OperationID != "op1" || !operationErr.Uncertain {
		t.Fatalf("uncertain action error = %v", err)
	}
	record, err := b.Operation(callContext(t), operationErr.OperationID)
	if err != nil || record.ID != "op1" || record.Outcome != "verified_complete" || len(record.Receipt) == 0 {
		t.Fatalf("operation recovery = %+v, %v", record, err)
	}
	if performs.Load() != 1 {
		t.Fatalf("action replayed %d times", performs.Load())
	}
}

func TestPerformServerErrorRetainsReceiptAndDoesNotClaimTransportLoss(t *testing.T) {
	data := json.RawMessage(`{"operation_id":"op1","receipt":{"outcome":"failed_before_dispatch"}}`)
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return nil, &rpcError{Code: ErrCodeInvalidParams, Message: "expectation required", Data: data}
	})
	_, err := page.Perform(callContext(t), "op1", map[string]any{"kind": "click"}, ActionOptions{})
	var operationErr *OperationError
	var typed *Error
	if !errors.As(err, &operationErr) || !errors.As(err, &typed) || operationErr.OperationID != "op1" || operationErr.Uncertain || string(typed.Data) != string(data) || string(operationErr.Receipt) != `{"outcome":"failed_before_dispatch"}` {
		t.Fatalf("server action error lost details: %v", err)
	}
}

func TestPerformCanonicalUncertainReceiptRemainsUncertain(t *testing.T) {
	data := json.RawMessage(`{"operation_id":"op1","receipt":{"outcome":"outcome_unknown","dispatch":"may_have_been_sent"},"detail":{"code":"outcome_unknown","message":"connection interrupted"},"source":{"code":"Cdp","message":"connection interrupted","data":{"cdp":{"code":-32000,"method":"Input.dispatchMouseEvent","message":"connection interrupted"}}}}`)
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return nil, &rpcError{Code: ErrCodeAction, Message: "action outcome unknown", Data: data}
	})
	_, err := page.Perform(callContext(t), "op1", map[string]any{"kind": "click"}, ActionOptions{})
	var operationErr *OperationError
	var typed *Error
	if !errors.As(err, &operationErr) || !errors.As(err, &typed) || !operationErr.Uncertain || operationErr.OperationID != "op1" || string(typed.Data) != string(data) || string(operationErr.Receipt) != `{"outcome":"outcome_unknown","dispatch":"may_have_been_sent"}` {
		t.Fatalf("canonical uncertain action data lost: %v", err)
	}
}

func TestPerformOversizedLostResponseRemainsUncertain(t *testing.T) {
	b := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return map[string]any{"receipt": strings.Repeat("x", 1024)}, nil
	})
	b.t.mu.Lock()
	b.t.messageLimit = 256
	b.t.mu.Unlock()
	page := &Page{id: "P1", b: b}
	_, err := page.Perform(callContext(t), "op1", map[string]any{"kind": "click"}, ActionOptions{})
	var operationErr *OperationError
	if !HasCode(err, ErrCodeMessageTooLarge) || !errors.As(err, &operationErr) || !operationErr.Uncertain || operationErr.OperationID != "op1" {
		t.Fatalf("oversized action reply was not treated as uncertain: %v", err)
	}
}

func TestConvenienceActionKeepsAcceptedRecordWhenWaitFails(t *testing.T) {
	receipt := json.RawMessage(`{"outcome":"outcome_unknown","dispatch":"may_have_been_sent"}`)
	var performs atomic.Int32
	browser := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "actions.perform":
			performs.Add(1)
			var request struct {
				OperationID string `json:"operation_id"`
			}
			if err := json.Unmarshal(params, &request); err != nil {
				return nil, &rpcError{Code: ErrCodeInvalidParams, Message: err.Error()}
			}
			return OperationRecord{ID: request.OperationID, State: "running", Outcome: "outcome_unknown", Receipt: receipt}, nil
		case "operations.wait":
			return nil, &rpcError{Code: ErrCodeServerBusy, Message: "wait capacity exhausted"}
		default:
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
	})
	page := &Page{id: "P1", b: browser}
	err := page.Click(callContext(t), "#submit", ActionOptions{Expect: map[string]any{"kind": "text"}})
	var operationErr *OperationError
	if !errors.As(err, &operationErr) || !HasCode(err, ErrCodeServerBusy) {
		t.Fatalf("expected recoverable operation error, got %v", err)
	}
	if !operationErr.Uncertain || operationErr.Record.State != "running" || operationErr.Record.ID != operationErr.OperationID {
		t.Fatalf("accepted action lost uncertainty or recovery record: %+v", operationErr)
	}
	if string(operationErr.Receipt) != string(receipt) || performs.Load() != 1 {
		t.Fatalf("lost receipt or replayed action: receipt=%s performs=%d", operationErr.Receipt, performs.Load())
	}
}

func TestConvenienceTerminalFailureExposesReceipt(t *testing.T) {
	receipt := json.RawMessage(`{"outcome":"not_dispatched","dispatch":"not_sent"}`)
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var request struct {
			OperationID string `json:"operation_id"`
		}
		if err := json.Unmarshal(params, &request); err != nil {
			return nil, &rpcError{Code: ErrCodeInvalidParams, Message: err.Error()}
		}
		return OperationRecord{ID: request.OperationID, State: "finished", Outcome: "not_dispatched", Receipt: receipt, ErrorCode: "element_not_found"}, nil
	})
	err := page.Fill(callContext(t), "#missing", "value")
	var operationErr *OperationError
	if !errors.As(err, &operationErr) || operationErr.Uncertain || string(operationErr.Receipt) != string(receipt) {
		t.Fatalf("terminal failure lost dispatch evidence: %+v", operationErr)
	}
}
