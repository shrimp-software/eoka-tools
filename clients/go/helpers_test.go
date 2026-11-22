package eoka

import (
	"bufio"
	"context"
	"encoding/json"
	"io"
	"sync"
	"testing"
	"time"
)

type fakeHandler func(id int64, method string, params json.RawMessage) (result any, rpcErr *rpcError)

func newFakeTransport(t *testing.T, handle fakeHandler) *transport {
	t.Helper()

	reqR, reqW := io.Pipe()
	respR, respW := io.Pipe()

	tr := newTransport(reqW, respR)
	go tr.readLoop()
	go runFakeServer(reqR, respW, handle)

	t.Cleanup(func() {
		tr.shutdown()
		_ = reqR.Close()
		_ = respW.Close()
	})

	return tr
}

func newFakeBrowser(t *testing.T, handle fakeHandler) *Browser {
	t.Helper()
	b := &Browser{t: newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "server.hello":
			return fakeServerInfo(), nil
		case "server.shutdown":
			return map[string]any{}, nil
		default:
			return handle(id, method, params)
		}
	})}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	if err := b.hello(ctx); err != nil {
		t.Fatalf("fake server handshake: %v", err)
	}
	return b
}

func fakeServerInfo() ServerInfo {
	return ServerInfo{
		Protocol:      ProtocolVersion{Major: 4, Minor: 0},
		ServerVersion: "test-server",
		CoreVersion:   "test-core",
		Methods: []string{
			"server.hello", "server.shutdown", "browser.launch", "browser.connect", "browser.close",
			"browser.new_page", "browser.tabs", "browser.close_tab",
			"actions.perform", "operations.get", "operations.wait", "operations.cancel",
			"agent.observe", "agent.step", "text", "content", "info",
			"get_text", "get_attribute", "exists", "wait",
			"eval", "exec", "fetch", "page.solve_captcha", "captcha.turnstile",
			"page.capture_state", "page.restore_state", "screenshot",
			"mouse.down", "mouse.move", "mouse.up",
			"key.down", "key.up", "release_all_inputs", "emulate",
		},
		Capabilities: []string{"durableActions", "operationCancellation", "gracefulShutdown",
			"explicitExecutablePath", "browserAttachment", "runtimeIsolation", "sandboxControl", "viewportEmulation",
			"cloudflareAutoSolve"},
		Limits: TransportLimits{MaxMessageBytes: maxMessageBytes, MaxInFlight: maxInFlight},
	}
}

func runFakeServer(r io.Reader, w io.Writer, handle fakeHandler) {
	if closer, ok := w.(io.Closer); ok {
		defer closer.Close()
	}
	br := bufio.NewReaderSize(r, 64*1024)
	var writeMu sync.Mutex
	for {
		line, err := br.ReadBytes('\n')
		if err != nil {
			return
		}
		var req struct {
			ID     int64           `json:"id"`
			Method string          `json:"method"`
			Params json.RawMessage `json:"params"`
		}
		if err := json.Unmarshal(line, &req); err != nil {
			return
		}
		go func() {
			result, rpcErr := handle(req.ID, req.Method, req.Params)
			resp := rpcResponse{ID: req.ID, Error: rpcErr}
			if rpcErr == nil {
				data, err := json.Marshal(result)
				if err != nil {
					return
				}
				resp.Result = data
			}
			data, err := json.Marshal(resp)
			if err != nil {
				return
			}
			data = append(data, '\n')
			writeMu.Lock()
			_, _ = w.Write(data)
			writeMu.Unlock()
		}()
	}
}
