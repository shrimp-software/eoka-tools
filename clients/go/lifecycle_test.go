package eoka

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestMain(m *testing.M) {
	if mode := os.Getenv("EOKA_GO_FAKE_SERVER"); mode != "" {
		os.Exit(serveLifecycleFixture(mode))
	}
	os.Exit(m.Run())
}

func serveLifecycleFixture(mode string) int {
	log, err := os.OpenFile(os.Getenv("EOKA_GO_FAKE_LOG"), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0600)
	if err != nil {
		return 2
	}
	defer log.Close()
	reader := bufio.NewReader(os.Stdin)
	borrowed := false
	for {
		line, err := reader.ReadBytes('\n')
		if err != nil {
			_, _ = fmt.Fprintln(log, "eof-cleanup")
			return 0
		}
		var request rpcRequest
		if json.Unmarshal(line, &request) != nil {
			return 3
		}
		_, _ = fmt.Fprintln(log, request.Method)
		response := rpcResponse{ID: request.ID}
		result := any(map[string]any{})
		switch request.Method {
		case "server.hello":
			params, _ := json.Marshal(request.Params)
			if string(params) != `{"protocol":{"major":1,"minor":1}}` {
				response.Error = &rpcError{Code: ErrCodeInvalidParams, Message: "unexpected hello params"}
			} else if mode == "legacy" {
				response.Error = &rpcError{Code: ErrCodeUnknownMethod, Message: "server.hello"}
			} else {
				info := fakeServerInfo()
				if mode == "incompatible" {
					info.Protocol.Major = 2
				}
				if mode == "old-minor" {
					info.Protocol.Minor = 0
				}
				if mode == "no-connect" {
					info.Methods = []string{"server.hello", "server.shutdown", "browser.launch"}
				}
				if mode == "connect-only" {
					info.Methods = []string{"server.hello", "server.shutdown", "browser.connect", "browser.tabs", "page.text", "page.set_viewport", "page.reset_viewport"}
				}
				result = info
			}
		case "browser.launch":
			result = BrowserSessionInfo{Ownership: BrowserOwnershipOwned, BrowserVersion: "Chrome/123.0.0.0"}
			switch mode {
			case "borrowed-launch":
				result = BrowserSessionInfo{Ownership: BrowserOwnershipBorrowed, BrowserVersion: "Chrome/123.0.0.0"}
				borrowed = true
			case "missing-session":
				result = map[string]any{}
			case "missing-browser-version":
				result = map[string]any{"ownership": "owned"}
			case "missing-ownership":
				result = map[string]any{"browserVersion": "Chrome/123.0.0.0"}
			case "blank-browser-version":
				result = map[string]any{"ownership": "owned", "browserVersion": " "}
			case "null-session":
				result = nil
			case "invalid-ownership":
				result = map[string]any{"ownership": "unknown", "browserVersion": "Chrome/123.0.0.0"}
			case "launch-options", "unsandboxed-launch-options":
				params, _ := json.Marshal(request.Params)
				want, _ := json.Marshal(map[string]any{
					"deviceScaleFactor": 2,
					"executablePath":    os.Getenv("EOKA_GO_FAKE_EXECUTABLE"),
					"headless":          true,
					"runtimeDir":        os.Getenv("EOKA_GO_FAKE_RUNTIME"),
					"sandbox":           mode == "launch-options",
					"viewportHeight":    360,
					"viewportWidth":     640,
				})
				if string(params) != string(want) {
					response.Error = &rpcError{Code: ErrCodeInvalidParams, Message: "unexpected launch params: " + string(params)}
				}
			}
			if mode == "failed-launch" {
				response.Error = &rpcError{Code: ErrCodeInvalidParams, Message: "launch failed", Data: json.RawMessage(`{"detail":{"code":"launch_failed"}}`)}
			}
			if mode == "cancelled-launch" {
				_, _ = fmt.Fprintln(os.Stderr, "launch-started")
				continue
			}
		case "browser.connect":
			borrowed = true
			params, _ := json.Marshal(request.Params)
			if string(params) != `{"wsEndpoint":"ws://127.0.0.1:9222/devtools/browser/fixture"}` {
				response.Error = &rpcError{Code: ErrCodeInvalidParams, Message: "unexpected connect params"}
			}
			result = BrowserSessionInfo{Ownership: BrowserOwnershipBorrowed, BrowserVersion: "Chrome/124.0.0.0"}
			if mode == "owned-connect" {
				result = BrowserSessionInfo{Ownership: BrowserOwnershipOwned, BrowserVersion: "Chrome/124.0.0.0"}
			}
			if mode == "cancelled-connect" {
				_, _ = fmt.Fprintln(os.Stderr, "connect-started")
				continue
			}
		case "page.text":
			result = map[string]any{"text": "existing tab"}
		case "page.set_viewport", "page.reset_viewport":
			params, _ := json.Marshal(request.Params)
			want := `{"deviceScaleFactor":2,"height":360,"pageId":"P1","width":640}`
			if request.Method == "page.reset_viewport" {
				want = `{"pageId":"P1"}`
			}
			if string(params) != want {
				response.Error = &rpcError{Code: ErrCodeInvalidParams, Message: "unexpected viewport params: " + string(params)}
			}
		case "browser.new_page":
			result = map[string]any{"pageId": "P1"}
		case "browser.tabs":
			result = map[string]any{"tabs": []TabInfo{{ID: "P1", Title: "Fixture"}}}
		case "operations.wait":
			continue
		case "server.shutdown":
			if mode == "stuck-shutdown" {
				time.Sleep(5 * time.Second)
				continue
			}
			if mode == "legacy" {
				response.Error = &rpcError{Code: ErrCodeUnknownMethod, Message: "server.shutdown"}
			} else {
				if mode == "slow-cleanup" || mode == "cancelled-launch" || mode == "cancelled-connect" {
					time.Sleep(120 * time.Millisecond)
				}
				if borrowed {
					_, _ = fmt.Fprintln(log, "borrowed-disconnect")
				} else {
					_, _ = fmt.Fprintln(log, "owned-cleanup")
				}
			}
		default:
			response.Error = &rpcError{Code: ErrCodeUnknownMethod, Message: request.Method}
		}
		if response.Error == nil {
			response.Result, _ = json.Marshal(result)
		}
		data, err := json.Marshal(response)
		if err != nil {
			return 4
		}
		if _, err := os.Stdout.Write(append(data, '\n')); err != nil {
			return 5
		}
		if request.Method == "server.shutdown" && response.Error == nil {
			return 0
		}
	}
}

func lifecycleFixture(t *testing.T, mode string) (string, string) {
	t.Helper()
	binary, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	log := filepath.Join(t.TempDir(), "server-events")
	t.Setenv("EOKA_GO_FAKE_SERVER", mode)
	t.Setenv("EOKA_GO_FAKE_LOG", log)
	return binary, log
}

func fixtureEvents(t *testing.T, path string) []string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return strings.Fields(string(data))
}

func TestLaunchHandshakesBeforeBrowserAndCloseUsesServerShutdown(t *testing.T) {
	binary, log := lifecycleFixture(t, "normal")
	ctx := callContext(t)
	b, err := Launch(ctx, WithServerPath(binary))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = b.Close(context.Background()) })
	if b.ServerInfo().Protocol.Major != 1 || b.ServerInfo().ServerVersion != "test-server" {
		t.Fatalf("negotiated server info = %+v", b.ServerInfo())
	}
	page, err := b.NewPage(ctx, "")
	if err != nil || page.ID() != "P1" {
		t.Fatalf("NewPage = %v, %v", page, err)
	}
	tabs, err := b.Tabs(ctx)
	if err != nil || len(tabs) != 1 || tabs[0].ID != "P1" {
		t.Fatalf("Tabs = %+v, %v", tabs, err)
	}
	var wg sync.WaitGroup
	closeErrors := make(chan error, 8)
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); closeErrors <- b.Close(ctx) }()
	}
	wg.Wait()
	close(closeErrors)
	for err := range closeErrors {
		if err != nil {
			t.Fatal(err)
		}
	}
	if b.cmd.ProcessState == nil || !b.cmd.ProcessState.Exited() {
		t.Fatal("Close did not reap the sidecar")
	}
	events := fixtureEvents(t, log)
	want := "server.hello browser.launch browser.new_page browser.tabs server.shutdown owned-cleanup"
	if strings.Join(events, " ") != want {
		t.Fatalf("lifecycle requests = %v, want %s", events, want)
	}
	if _, err := b.Tabs(ctx); err == nil {
		t.Fatal("closed browser accepted a call")
	}
}

func TestLaunchRejectsLegacyOrIncompatibleBeforeBrowserCreation(t *testing.T) {
	for _, mode := range []string{"legacy", "incompatible", "old-minor"} {
		t.Run(mode, func(t *testing.T) {
			binary, log := lifecycleFixture(t, mode)
			b, err := Launch(callContext(t), WithServerPath(binary))
			if b != nil || !HasCode(err, ErrCodeProtocolMismatch) {
				t.Fatalf("incompatible launch = %v, %v", b, err)
			}
			events := fixtureEvents(t, log)
			if len(events) < 3 || events[0] != "server.hello" || events[1] != "server.shutdown" || strings.Contains(strings.Join(events, " "), "browser.launch") {
				t.Fatalf("incompatible launch created a browser or missed cleanup: %v", events)
			}
		})
	}
}

func TestFailedLaunchPreservesErrorAndCleansSidecar(t *testing.T) {
	binary, log := lifecycleFixture(t, "failed-launch")
	b, err := Launch(callContext(t), WithServerPath(binary))
	var typed *Error
	if b != nil || !errors.As(err, &typed) || typed.Code != ErrCodeInvalidParams || string(typed.Data) != `{"detail":{"code":"launch_failed"}}` {
		t.Fatalf("failed launch = %v, %v", b, err)
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.launch server.shutdown owned-cleanup" {
		t.Fatalf("failed launch cleanup = %s", got)
	}
}

type launchSignal struct {
	started chan struct{}
	once    sync.Once
}

func (signal *launchSignal) Write(data []byte) (int, error) {
	signal.once.Do(func() { close(signal.started) })
	return len(data), nil
}

func TestCancelledLaunchCompletesOwnedCleanupBeforeReturning(t *testing.T) {
	binary, log := lifecycleFixture(t, "cancelled-launch")
	ctx, cancel := context.WithCancel(callContext(t))
	defer cancel()
	signal := &launchSignal{started: make(chan struct{})}
	done := make(chan error, 1)
	go func() {
		b, err := Launch(ctx, WithServerPath(binary), WithStderr(signal))
		if b != nil {
			_ = b.Close(context.Background())
			done <- errors.New("cancelled launch returned a browser")
			return
		}
		done <- err
	}()
	select {
	case <-signal.started:
		cancel()
	case err := <-done:
		t.Fatalf("launch returned before cancellation: %v", err)
	case <-ctx.Done():
		t.Fatal("fixture never entered browser.launch")
	}
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled launch error = %v", err)
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.launch server.shutdown owned-cleanup" {
		t.Fatalf("cancelled launch cleanup = %s", got)
	}
}

func TestCloseCallerCancellationLeavesCleanupOwnedUntilCompletion(t *testing.T) {
	binary, log := lifecycleFixture(t, "slow-cleanup")
	b, err := Launch(callContext(t), WithServerPath(binary))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = b.Close(context.Background()) })
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := b.Close(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled Close error = %v", err)
	}
	if err := b.Close(callContext(t)); err != nil {
		t.Fatal(err)
	}
	if b.cmd.ProcessState == nil || !b.cmd.ProcessState.Exited() {
		t.Fatal("background cleanup did not reap process")
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.launch server.shutdown owned-cleanup" {
		t.Fatalf("cancelled Close cleanup = %s", got)
	}
}

func TestCloseInterruptsInFlightOperationWait(t *testing.T) {
	waitStarted := make(chan struct{})
	release := make(chan struct{})
	defer close(release)
	var shutdowns int
	tr := newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "server.hello":
			return fakeServerInfo(), nil
		case "operations.wait":
			close(waitStarted)
			<-release
			return OperationRecord{ID: "op1", State: "finished", Outcome: "outcome_unknown"}, nil
		case "server.shutdown":
			shutdowns++
			return map[string]any{}, nil
		default:
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
	})
	b := &Browser{t: tr}
	ctx := callContext(t)
	if err := b.hello(ctx); err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { _, err := b.WaitOperation(ctx, "op1", 30000); done <- err }()
	<-waitStarted
	if err := b.Close(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-done; err == nil || errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("shutdown did not interrupt local operation wait: %v", err)
	}
	if shutdowns != 1 {
		t.Fatalf("shutdown requests = %d", shutdowns)
	}
}

func TestTransportEOFRejectsAllFutureCalls(t *testing.T) {
	tr, requests, replies := wireTransport(t)
	ctx := callContext(t)
	done := make(chan error, 1)
	go func() { done <- tr.call(ctx, "browser.tabs", map[string]any{}, nil) }()
	_ = wireRequest(t, requests)
	_ = replies.Close()
	if err := <-done; !errors.Is(err, io.EOF) {
		t.Fatalf("EOF error = %v", err)
	}
	if err := tr.call(ctx, "browser.tabs", map[string]any{}, nil); !errors.Is(err, io.EOF) {
		t.Fatalf("future call after EOF error = %v", err)
	}
}

func TestCloseHardDeadlineKillsAndReapsUnresponsiveSidecar(t *testing.T) {
	binary, log := lifecycleFixture(t, "stuck-shutdown")
	b, err := Launch(callContext(t), WithServerPath(binary))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = b.Close(context.Background()) })
	b.closeOnce.Do(func() { go b.close(100 * time.Millisecond) })
	if err := b.Close(callContext(t)); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("forced cleanup error = %v", err)
	}
	if b.cmd.ProcessState == nil {
		t.Fatal("unresponsive sidecar was not reaped")
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.launch server.shutdown" {
		t.Fatalf("unresponsive sidecar lifecycle = %s", got)
	}
}

func TestLaunchManagedOptionsAndActualSessionMetadata(t *testing.T) {
	for _, mode := range []string{"launch-options", "unsandboxed-launch-options"} {
		t.Run(mode, func(t *testing.T) {
			binary, log := lifecycleFixture(t, mode)
			executable := filepath.Join(t.TempDir(), "chrome")
			runtime := t.TempDir()
			t.Setenv("EOKA_GO_FAKE_EXECUTABLE", executable)
			t.Setenv("EOKA_GO_FAKE_RUNTIME", runtime)
			b, err := Launch(callContext(t), WithServerPath(binary), WithExecutablePath(executable),
				WithRuntimeDir(runtime), WithSandbox(mode == "launch-options"), WithViewport(640, 360), WithDeviceScaleFactor(2))
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = b.Close(context.Background()) })
			if got := b.SessionInfo(); got != (BrowserSessionInfo{Ownership: BrowserOwnershipOwned, BrowserVersion: "Chrome/123.0.0.0"}) {
				t.Fatalf("actual launch metadata = %+v", got)
			}
			if err := b.Close(callContext(t)); err != nil {
				t.Fatal(err)
			}
			if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.launch server.shutdown owned-cleanup" {
				t.Fatalf("managed launch lifecycle = %s", got)
			}
		})
	}
}

func TestLaunchReportsBorrowedProfileReuse(t *testing.T) {
	binary, log := lifecycleFixture(t, "borrowed-launch")
	b, err := Launch(callContext(t), WithServerPath(binary))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = b.Close(context.Background()) })
	if b.SessionInfo().Ownership != BrowserOwnershipBorrowed {
		t.Fatalf("borrowed launch ownership = %+v", b.SessionInfo())
	}
	if err := b.Close(callContext(t)); err != nil {
		t.Fatal(err)
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.launch server.shutdown borrowed-disconnect" {
		t.Fatalf("borrowed launch lifecycle = %s", got)
	}
}

func TestLaunchRejectsInvalidSessionMetadataAndCleansSidecar(t *testing.T) {
	for _, mode := range []string{"missing-session", "null-session", "missing-browser-version", "blank-browser-version", "missing-ownership", "invalid-ownership"} {
		t.Run(mode, func(t *testing.T) {
			binary, log := lifecycleFixture(t, mode)
			b, err := Launch(callContext(t), WithServerPath(binary))
			if b != nil || !HasCode(err, ErrCodeProtocolMismatch) {
				t.Fatalf("invalid session result = %v, %v", b, err)
			}
			if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.launch server.shutdown owned-cleanup" {
				t.Fatalf("invalid session cleanup = %s", got)
			}
		})
	}
}

func TestConnectSharesHandshakeAndShutdownAndUsesExistingTabs(t *testing.T) {
	for _, mode := range []string{"normal", "connect-only"} {
		t.Run(mode, func(t *testing.T) {
			binary, log := lifecycleFixture(t, mode)
			ctx := callContext(t)
			b, err := Connect(ctx, "ws://127.0.0.1:9222/devtools/browser/fixture", WithServerPath(binary), WithNoAutoDownload(), WithStderr(io.Discard))
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = b.Close(context.Background()) })
			if got := b.SessionInfo(); got != (BrowserSessionInfo{Ownership: BrowserOwnershipBorrowed, BrowserVersion: "Chrome/124.0.0.0"}) {
				t.Fatalf("actual connect metadata = %+v", got)
			}
			tabs, err := b.Tabs(ctx)
			if err != nil || len(tabs) != 1 || tabs[0].ID != "P1" {
				t.Fatalf("existing tabs = %+v, %v", tabs, err)
			}
			page := b.Page(tabs[0].ID)
			if text, err := page.Text(ctx); err != nil || text != "existing tab" {
				t.Fatalf("attached page text = %q, %v", text, err)
			}
			if err := page.SetViewport(ctx, 640, 360, 2); err != nil {
				t.Fatal(err)
			}
			if err := page.ResetViewport(ctx); err != nil {
				t.Fatal(err)
			}
			for i := 0; i < 2; i++ {
				if err := b.Close(ctx); err != nil {
					t.Fatal(err)
				}
			}
			if b.cmd.ProcessState == nil || !b.cmd.ProcessState.Exited() {
				t.Fatal("Connect Close did not reap private sidecar")
			}
			want := "server.hello browser.connect browser.tabs page.text page.set_viewport page.reset_viewport server.shutdown borrowed-disconnect"
			if got := strings.Join(fixtureEvents(t, log), " "); got != want {
				t.Fatalf("connect lifecycle = %s, want %s", got, want)
			}
		})
	}
}

func TestConnectRejectsUnadvertisedMethodBeforeAttachment(t *testing.T) {
	binary, log := lifecycleFixture(t, "no-connect")
	b, err := Connect(callContext(t), "ws://127.0.0.1:9222/devtools/browser/fixture", WithServerPath(binary))
	if b != nil || !HasCode(err, ErrCodeProtocolMismatch) {
		t.Fatalf("unadvertised connect = %v, %v", b, err)
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello server.shutdown owned-cleanup" {
		t.Fatalf("unadvertised connect transmitted attachment: %s", got)
	}
}

func TestConnectRejectsOwnedMetadataAndDisconnects(t *testing.T) {
	binary, log := lifecycleFixture(t, "owned-connect")
	b, err := Connect(callContext(t), "ws://127.0.0.1:9222/devtools/browser/fixture", WithServerPath(binary))
	if b != nil || !HasCode(err, ErrCodeProtocolMismatch) {
		t.Fatalf("owned connect metadata = %v, %v", b, err)
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.connect server.shutdown borrowed-disconnect" {
		t.Fatalf("invalid connect metadata cleanup = %s", got)
	}
}

func TestCancelledConnectCompletesDisconnectBeforeReturning(t *testing.T) {
	binary, log := lifecycleFixture(t, "cancelled-connect")
	ctx, cancel := context.WithCancel(callContext(t))
	defer cancel()
	signal := &launchSignal{started: make(chan struct{})}
	done := make(chan error, 1)
	go func() {
		b, err := Connect(ctx, "ws://127.0.0.1:9222/devtools/browser/fixture", WithServerPath(binary), WithStderr(signal))
		if b != nil {
			_ = b.Close(context.Background())
			done <- errors.New("cancelled connect returned a browser")
			return
		}
		done <- err
	}()
	select {
	case <-signal.started:
		cancel()
	case err := <-done:
		t.Fatalf("connect returned before cancellation: %v", err)
	case <-ctx.Done():
		t.Fatal("fixture never entered browser.connect")
	}
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled connect error = %v", err)
	}
	if got := strings.Join(fixtureEvents(t, log), " "); got != "server.hello browser.connect server.shutdown borrowed-disconnect" {
		t.Fatalf("cancelled connect cleanup = %s", got)
	}
}
