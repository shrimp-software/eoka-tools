package eoka

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
	"testing"
	"time"
)

func wireTransport(t *testing.T) (*transport, *bufio.Reader, *io.PipeWriter) {
	t.Helper()
	reqR, reqW := io.Pipe()
	respR, respW := io.Pipe()
	tr := newTransport(reqW, respR)
	go tr.readLoop()
	t.Cleanup(func() {
		tr.shutdown()
		_ = reqR.Close()
		_ = respW.Close()
	})
	return tr, bufio.NewReader(reqR), respW
}

func wireRequest(t *testing.T, r *bufio.Reader) rpcRequest {
	t.Helper()
	line, err := r.ReadBytes('\n')
	if err != nil {
		t.Fatal(err)
	}
	var request rpcRequest
	if err := json.Unmarshal(line, &request); err != nil {
		t.Fatal(err)
	}
	if request.ID <= 0 || request.ID > maxRequestID {
		t.Fatalf("unsafe request ID: %d", request.ID)
	}
	return request
}

func callContext(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	t.Cleanup(cancel)
	return ctx
}

func TestTransportOutOfOrderResults(t *testing.T) {
	tr, requests, replies := wireTransport(t)
	ctx := callContext(t)
	type outcome struct {
		method string
		result json.RawMessage
		err    error
	}
	results := make(chan outcome, 2)
	for _, method := range []string{"operations.wait", "browser.tabs"} {
		go func(method string) {
			var result json.RawMessage
			err := tr.call(ctx, method, map[string]any{}, &result)
			results <- outcome{method: method, result: result, err: err}
		}(method)
	}
	first := wireRequest(t, requests)
	second := wireRequest(t, requests)
	for _, request := range []rpcRequest{second, first} {
		if _, err := fmt.Fprintf(replies, "{\"id\":%d,\"result\":%q}\n", request.ID, request.Method); err != nil {
			t.Fatal(err)
		}
	}
	for i := 0; i < 2; i++ {
		got := <-results
		if got.err != nil || string(got.result) != fmt.Sprintf("%q", got.method) {
			t.Fatalf("uncorrelated result: %+v", got)
		}
	}
}

func TestTransportSplitUTF8AndResultShapes(t *testing.T) {
	for _, result := range []string{`null`, `42`, `["雪",true]`, `"雪"`} {
		t.Run(result, func(t *testing.T) {
			tr, requests, replies := wireTransport(t)
			ctx := callContext(t)
			done := make(chan error, 1)
			var got json.RawMessage
			go func() { done <- tr.call(ctx, "page.evaluate", map[string]any{}, &got) }()
			request := wireRequest(t, requests)
			line := fmt.Sprintf("{\"id\":%d,\"result\":%s}\n", request.ID, result)
			for i := 0; i < len(line); i++ {
				if _, err := replies.Write([]byte{line[i]}); err != nil {
					t.Fatal(err)
				}
			}
			if err := <-done; err != nil || string(got) != result {
				t.Fatalf("result = %s, err = %v", got, err)
			}
		})
	}
}

func TestTransportRejectsInvalidReplies(t *testing.T) {
	for _, reply := range []string{
		`{`, `null`, `[]`, `{}`, `{"id":null,"result":{}}`,
		`{"id":0,"result":{}}`, `{"id":-1,"result":{}}`,
		`{"id":1.5,"result":{}}`, `{"id":9007199254740992,"result":{}}`,
		`{"id":1}`, `{"id":1,"result":null,"error":null}`,
		`{"id":1,"error":null}`, `{"id":1,"error":{}}`,
		`{"id":1,"error":{"code":"Internal","message":null}}`,
		`{"id":1,"error":{"code":"Internal","message":42}}`,
		`{"id":99,"result":{}}`, "{\"id\":1,\"result\":\"\xff\"}",
	} {
		t.Run(reply, func(t *testing.T) {
			tr, requests, replies := wireTransport(t)
			ctx := callContext(t)
			done := make(chan error, 2)
			go func() { done <- tr.call(ctx, "browser.tabs", map[string]any{}, nil) }()
			go func() { done <- tr.call(ctx, "operations.wait", map[string]any{}, nil) }()
			_ = wireRequest(t, requests)
			_ = wireRequest(t, requests)
			_, _ = io.WriteString(replies, reply+"\n")
			for i := 0; i < 2; i++ {
				if err := <-done; err == nil || errors.Is(err, context.DeadlineExceeded) {
					t.Fatalf("invalid reply did not fail pending call: %v", err)
				}
			}
			if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); err == nil {
				t.Fatal("failed transport accepted another request")
			}
		})
	}
}

func TestTransportRejectsDuplicateResponseID(t *testing.T) {
	tr, requests, replies := wireTransport(t)
	ctx := callContext(t)
	done := make(chan error, 1)
	go func() { done <- tr.call(ctx, "browser.tabs", map[string]any{}, nil) }()
	first := wireRequest(t, requests)
	line := fmt.Sprintf("{\"id\":%d,\"result\":null}\n", first.ID)
	_, _ = io.WriteString(replies, line)
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	go func() { done <- tr.call(ctx, "operations.wait", map[string]any{}, nil) }()
	_ = wireRequest(t, requests)
	_, _ = io.WriteString(replies, line)
	if err := <-done; err == nil || !strings.Contains(err.Error(), "duplicate") {
		t.Fatalf("duplicate reply error = %v", err)
	}
}

func TestTransportRejectsPartialResponseAtEOF(t *testing.T) {
	tr, requests, replies := wireTransport(t)
	ctx := callContext(t)
	done := make(chan error, 1)
	go func() { done <- tr.call(ctx, "browser.tabs", map[string]any{}, nil) }()
	request := wireRequest(t, requests)
	_, _ = fmt.Fprintf(replies, "{\"id\":%d,\"result\":null}", request.ID)
	_ = replies.Close()
	if err := <-done; err == nil || !strings.Contains(err.Error(), "incomplete") {
		t.Fatalf("partial line error = %v", err)
	}
}

func TestTransportLocalCancellationDiscardsOnlyLateResponse(t *testing.T) {
	tr, requests, replies := wireTransport(t)
	ctx, cancel := context.WithCancel(callContext(t))
	done := make(chan error, 1)
	go func() { done <- tr.call(ctx, "operations.wait", map[string]any{"id": "op"}, nil) }()
	cancelledRequest := wireRequest(t, requests)
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("local cancellation error = %v", err)
	}
	ctx = callContext(t)
	go func() { done <- tr.call(ctx, "browser.tabs", map[string]any{}, nil) }()
	next := wireRequest(t, requests)
	if next.Method != "browser.tabs" {
		t.Fatalf("cancellation sent unexpected remote request: %s", next.Method)
	}
	_, _ = fmt.Fprintf(replies, "{\"id\":%d,\"result\":{}}\n{\"id\":%d,\"result\":{}}\n", cancelledRequest.ID, next.ID)
	if err := <-done; err != nil {
		t.Fatalf("late cancelled reply poisoned transport: %v", err)
	}
	tr.mu.Lock()
	abandoned := len(tr.abandoned)
	tr.mu.Unlock()
	if abandoned != 0 {
		t.Fatalf("late response retained %d abandoned IDs", abandoned)
	}
}

func TestTransportSafeIDExhaustion(t *testing.T) {
	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if id != maxRequestID {
			t.Errorf("request ID = %d", id)
		}
		return nil, nil
	})
	tr.nextID.Store(maxRequestID - 1)
	ctx := callContext(t)
	if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); err != nil {
		t.Fatal(err)
	}
	if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); err == nil || !strings.Contains(err.Error(), "exhausted") {
		t.Fatalf("ID exhaustion error = %v", err)
	}
}

func TestTransportBoundsOutstandingCancelledRequests(t *testing.T) {
	tr, requests, replies := wireTransport(t)
	tr.mu.Lock()
	tr.inFlightLimit = 1
	tr.mu.Unlock()
	ctx, cancel := context.WithCancel(callContext(t))
	done := make(chan error, 1)
	go func() { done <- tr.call(ctx, "operations.wait", map[string]any{}, nil) }()
	request := wireRequest(t, requests)
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	ctx = callContext(t)
	if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); !HasCode(err, ErrCodeServerBusy) {
		t.Fatalf("outstanding request bound error = %v", err)
	}
	go func() { done <- tr.call(ctx, "server.shutdown", map[string]any{}, nil) }()
	shutdown := wireRequest(t, requests)
	if shutdown.Method != "server.shutdown" {
		t.Fatalf("shutdown blocked by in-flight bound: %+v", shutdown)
	}
	_, _ = fmt.Fprintf(replies, "{\"id\":%d,\"result\":{}}\n{\"id\":%d,\"result\":{}}\n", request.ID, shutdown.ID)
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

func TestTransportBoundsMessages(t *testing.T) {
	t.Run("request", func(t *testing.T) {
		tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
			t.Error("oversized request reached server")
			return nil, nil
		})
		tr.mu.Lock()
		tr.messageLimit = 128
		tr.mu.Unlock()
		if err := tr.call(callContext(t), "page.execute", map[string]any{"js": strings.Repeat("x", 128)}, nil); !HasCode(err, ErrCodeMessageTooLarge) {
			t.Fatalf("oversized request error = %v", err)
		}
		tr.mu.Lock()
		pending := len(tr.pending)
		tr.mu.Unlock()
		if pending != 0 {
			t.Fatalf("oversized request retained %d pending calls", pending)
		}
	})
	t.Run("32MiB response", func(t *testing.T) {
		tr, requests, replies := wireTransport(t)
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		done := make(chan error, 1)
		go func() { done <- tr.call(ctx, "page.content", map[string]any{}, nil) }()
		_ = wireRequest(t, requests)
		chunk := strings.Repeat("x", 256*1024)
		for i := 0; i < maxMessageBytes/len(chunk); i++ {
			if _, err := io.WriteString(replies, chunk); err != nil {
				t.Fatal(err)
			}
		}
		_, _ = io.WriteString(replies, "x\n")
		if err := <-done; !HasCode(err, ErrCodeMessageTooLarge) {
			t.Fatalf("oversized response error = %v", err)
		}
	})
	t.Run("exact negotiated boundary", func(t *testing.T) {
		tr, requests, replies := wireTransport(t)
		tr.mu.Lock()
		tr.messageLimit = 128
		tr.mu.Unlock()
		ctx := callContext(t)
		done := make(chan error, 1)
		go func() { done <- tr.call(ctx, "browser.tabs", map[string]any{}, nil) }()
		request := wireRequest(t, requests)
		line := fmt.Sprintf("{\"id\":%d,\"result\":null}", request.ID)
		_, _ = io.WriteString(replies, line+strings.Repeat(" ", 128-len(line))+"\n")
		if err := <-done; err != nil {
			t.Fatalf("exact boundary response error = %v", err)
		}
	})
}

func TestTransportShutdownUnblocksBlockedWrite(t *testing.T) {
	reqR, reqW := io.Pipe()
	respR, respW := io.Pipe()
	defer reqR.Close()
	defer respW.Close()
	tr := newTransport(reqW, respR)
	go tr.readLoop()
	ctx := callContext(t)
	done := make(chan error, 1)
	go func() { done <- tr.call(ctx, "browser.tabs", map[string]any{}, nil) }()
	tr.shutdown()
	if err := <-done; err == nil || errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("shutdown did not fail blocked call: %v", err)
	}
	if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); err == nil {
		t.Fatal("closed transport accepted request")
	}
}

func TestTransportErrorPreservesStructuredData(t *testing.T) {
	data := json.RawMessage(`{"operationId":"op1","receipt":{"outcome":"outcome_unknown"},"cdp":{"code":-32000},"detail":{"kind":"dispatch"}}`)
	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return nil, &rpcError{Code: "ActionOutcomeUnknown", Message: "response lost", Data: data}
	})
	err := tr.call(callContext(t), "actions.perform", map[string]any{}, nil)
	var typed *Error
	if !errors.As(err, &typed) || string(typed.Data) != string(data) {
		t.Fatalf("structured error data lost: %#v", typed)
	}
}

type failedRequestWriter struct {
	err error
}

func (w failedRequestWriter) Write(data []byte) (int, error) {
	if w.err == nil {
		return len(data) - 1, nil
	}
	return 0, w.err
}

func (w failedRequestWriter) Close() error {
	return nil
}

func TestTransportWriteFailureClosesPendingAndFutureCalls(t *testing.T) {
	writeErr := errors.New("request pipe failed")
	for _, test := range []struct {
		name   string
		writer failedRequestWriter
		want   error
	}{
		{name: "write failure", writer: failedRequestWriter{err: writeErr}, want: writeErr},
		{name: "short write", writer: failedRequestWriter{}, want: io.ErrShortWrite},
	} {
		t.Run(test.name, func(t *testing.T) {
			responseReader, responseWriter := io.Pipe()
			defer responseWriter.Close()
			tr := newTransport(test.writer, responseReader)
			go tr.readLoop()
			t.Cleanup(tr.shutdown)
			ctx := callContext(t)
			if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); !errors.Is(err, test.want) {
				t.Fatalf("write failure error = %v", err)
			}
			if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); !errors.Is(err, test.want) {
				t.Fatalf("future call after write failure error = %v", err)
			}
		})
	}
}
