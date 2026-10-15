package eoka

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sync"
	"sync/atomic"
	"unicode/utf8"
)

const maxMessageBytes = 32 * 1024 * 1024
const maxInFlight = 128
const maxRequestID int64 = 1<<53 - 1

var errTransportClosed = errors.New("eoka: transport closed")

type transportFailure struct {
	err error
}

func (e *transportFailure) Error() string {
	return e.err.Error()
}

func (e *transportFailure) Unwrap() error {
	return e.err
}

type rpcRequest struct {
	ID     int64  `json:"id"`
	Method string `json:"method"`
	Params any    `json:"params"`
}

type rpcError struct {
	Code    string          `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}

type rpcResponse struct {
	ID     int64           `json:"id"`
	Result json.RawMessage `json:"result,omitempty"`
	Error  *rpcError       `json:"error,omitempty"`
}

type pendingCall struct {
	response chan *rpcResponse
	sent     bool
}

type queuedRequest struct {
	id   int64
	call *pendingCall
	data []byte
}

type transport struct {
	w      io.WriteCloser
	r      *bufio.Reader
	reader io.Closer

	mu            sync.Mutex
	pending       map[int64]*pendingCall
	abandoned     map[int64]struct{}
	closeErr      error
	messageLimit  int
	inFlightLimit int
	methods       map[string]bool
	capabilities  map[string]bool

	nextID atomic.Int64
	writes chan queuedRequest
	done   chan struct{}
}

func newTransport(w io.WriteCloser, r io.Reader) *transport {
	t := &transport{
		w:             w,
		r:             bufio.NewReaderSize(r, 256*1024),
		pending:       make(map[int64]*pendingCall),
		abandoned:     make(map[int64]struct{}),
		messageLimit:  maxMessageBytes,
		inFlightLimit: maxInFlight,
		writes:        make(chan queuedRequest, maxInFlight+1),
		done:          make(chan struct{}),
	}
	t.reader, _ = r.(io.Closer)
	go t.writeLoop()
	return t
}

func (t *transport) readLoop() {
	for {
		var line []byte
		for {
			part, err := t.r.ReadSlice('\n')
			t.mu.Lock()
			limit := t.messageLimit
			t.mu.Unlock()
			payload := part
			if err == nil {
				payload = part[:len(part)-1]
			}
			if len(line)+len(payload) > limit {
				t.fail(&Error{Code: ErrCodeMessageTooLarge, Message: "response exceeds negotiated message limit"})
				return
			}
			line = append(line, payload...)
			if err == bufio.ErrBufferFull {
				continue
			}
			if err != nil {
				if len(line) != 0 {
					t.fail(fmt.Errorf("eoka: incomplete response from eoka-server: %w", err))
				} else {
					t.fail(fmt.Errorf("eoka: reading from eoka-server: %w", err))
				}
				return
			}
			if !t.handleLine(line) {
				return
			}
			break
		}
	}
}

func (t *transport) handleLine(line []byte) bool {
	var envelope map[string]json.RawMessage
	if !utf8.Valid(line) {
		t.fail(errors.New("eoka: invalid UTF-8 response from eoka-server"))
		return false
	}
	if err := json.Unmarshal(line, &envelope); err != nil || envelope == nil {
		t.fail(errors.New("eoka: malformed response from eoka-server"))
		return false
	}
	var resp rpcResponse
	if err := json.Unmarshal(envelope["id"], &resp.ID); err != nil || resp.ID <= 0 || resp.ID > maxRequestID {
		t.fail(errors.New("eoka: invalid response ID from eoka-server"))
		return false
	}
	result, hasResult := envelope["result"]
	errorData, hasError := envelope["error"]
	if hasResult == hasError {
		t.fail(errors.New("eoka: response must contain exactly one result or error"))
		return false
	}
	if hasError {
		var fields map[string]json.RawMessage
		if err := json.Unmarshal(errorData, &fields); err != nil || fields == nil || fields["code"] == nil || fields["message"] == nil || bytes.Equal(fields["message"], []byte("null")) {
			t.fail(errors.New("eoka: malformed error from eoka-server"))
			return false
		}
		if err := json.Unmarshal(errorData, &resp.Error); err != nil || resp.Error == nil || resp.Error.Code == "" {
			t.fail(errors.New("eoka: malformed error from eoka-server"))
			return false
		}
	} else {
		resp.Result = result
	}

	t.mu.Lock()
	call, ok := t.pending[resp.ID]
	if ok && call.sent {
		delete(t.pending, resp.ID)
		call.response <- &resp
		t.mu.Unlock()
		return true
	}
	if _, cancelled := t.abandoned[resp.ID]; cancelled {
		delete(t.abandoned, resp.ID)
		t.mu.Unlock()
		return true
	}
	t.mu.Unlock()
	t.fail(fmt.Errorf("eoka: unknown or duplicate response ID %d", resp.ID))
	return false
}

func (t *transport) writeLoop() {
	for {
		select {
		case <-t.done:
			return
		case request := <-t.writes:
			t.mu.Lock()
			if t.closeErr != nil {
				t.mu.Unlock()
				return
			}
			if t.pending[request.id] != request.call {
				t.mu.Unlock()
				continue
			}
			request.call.sent = true
			t.mu.Unlock()
			n, err := t.w.Write(request.data)
			if err == nil && n != len(request.data) {
				err = io.ErrShortWrite
			}
			if err != nil {
				t.fail(fmt.Errorf("eoka: writing request to eoka-server: %w", err))
				return
			}
		}
	}
}

func (t *transport) fail(err error) {
	t.mu.Lock()
	if t.closeErr != nil {
		t.mu.Unlock()
		return
	}
	t.closeErr = &transportFailure{err: err}
	close(t.done)
	for id, call := range t.pending {
		close(call.response)
		delete(t.pending, id)
	}
	clear(t.abandoned)
	t.mu.Unlock()
	_ = t.w.Close()
	if t.reader != nil {
		_ = t.reader.Close()
	}
}

func (t *transport) closed() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.closeErr
}

func (t *transport) shutdown() {
	t.fail(errTransportClosed)
}

func (t *transport) requireCapability(capability string) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	if !t.capabilities[capability] {
		return &Error{Code: ErrCodeCapabilityUnavailable, Message: "server does not advertise " + capability}
	}
	return nil
}

func (t *transport) call(ctx context.Context, method string, params any, result any) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	t.mu.Lock()
	if t.closeErr != nil {
		err := t.closeErr
		t.mu.Unlock()
		return err
	}
	if t.methods != nil && !t.methods[method] {
		t.mu.Unlock()
		return &Error{Code: ErrCodeUnknownMethod, Message: "server does not advertise " + method}
	}
	if len(t.pending)+len(t.abandoned) >= t.inFlightLimit && method != "server.shutdown" {
		t.mu.Unlock()
		return &Error{Code: ErrCodeServerBusy, Message: "negotiated in-flight limit reached"}
	}
	if t.nextID.Load() >= maxRequestID {
		t.mu.Unlock()
		return errors.New("eoka: safe request IDs exhausted; restart the sidecar explicitly")
	}
	id := t.nextID.Add(1)
	call := &pendingCall{response: make(chan *rpcResponse, 1)}
	t.pending[id] = call
	limit := t.messageLimit
	t.mu.Unlock()

	data, err := json.Marshal(rpcRequest{ID: id, Method: method, Params: params})
	if err == nil && len(data) > limit {
		err = &Error{Code: ErrCodeMessageTooLarge, Message: "request exceeds negotiated message limit"}
	}
	if err != nil {
		t.mu.Lock()
		delete(t.pending, id)
		t.mu.Unlock()
		return fmt.Errorf("eoka: encoding request: %w", err)
	}
	data = append(data, '\n')
	select {
	case t.writes <- queuedRequest{id: id, call: call, data: data}:
	case <-t.done:
		return t.closed()
	case <-ctx.Done():
		t.abandon(id, call)
		return ctx.Err()
	}

	select {
	case resp, ok := <-call.response:
		if !ok {
			return t.closed()
		}
		if resp.Error != nil {
			return &Error{Code: resp.Error.Code, Message: resp.Error.Message, Data: resp.Error.Data}
		}
		if result != nil {
			if err := json.Unmarshal(resp.Result, result); err != nil {
				return fmt.Errorf("eoka: decoding result: %w", err)
			}
		}
		return nil
	case <-ctx.Done():
		t.abandon(id, call)
		return ctx.Err()
	}
}

func (t *transport) abandon(id int64, call *pendingCall) {
	t.mu.Lock()
	if t.pending[id] == call {
		delete(t.pending, id)
		if call.sent {
			t.abandoned[id] = struct{}{}
		}
	}
	t.mu.Unlock()
}
