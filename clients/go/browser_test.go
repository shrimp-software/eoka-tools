package eoka

import (
	"context"
	"encoding/json"
	"math"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestLaunchParamsIncludeRedactedProxyFields(t *testing.T) {
	params, err := (options{headless: true, proxy: "socks5://name%40example:pass%3Aword@127.0.0.1:1080"}).launchParams()
	if err != nil {
		t.Fatalf("launchParams: %v", err)
	}
	proxy, ok := params["proxy"].(map[string]any)
	if !ok {
		t.Fatalf("proxy params missing: %#v", params)
	}
	if got, want := proxy["server"], "socks5://127.0.0.1:1080"; got != want {
		t.Fatalf("server = %v, want %v", got, want)
	}
	if got, want := proxy["username"], "name@example"; got != want {
		t.Fatalf("username = %v, want %v", got, want)
	}
	if got, want := proxy["password"], "pass:word"; got != want {
		t.Fatalf("password = %v, want %v", got, want)
	}
}

func TestLaunchParamsIncludeAutoSolveCloudflare(t *testing.T) {
	o := options{headless: true}
	WithAutoSolveCloudflare(0, 0)(&o)
	params, err := o.launchParams()
	if err != nil {
		t.Fatal(err)
	}
	if got, ok := params["autoSolveCloudflare"].(map[string]any); !ok || len(got) != 0 {
		t.Fatalf("autoSolveCloudflare = %#v", params["autoSolveCloudflare"])
	}
	WithAutoSolveCloudflare(time.Second, 10*time.Second)(&o)
	params, err = o.launchParams()
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{"intervalMs": int64(1000), "timeoutMs": int64(10000)}
	if got := params["autoSolveCloudflare"].(map[string]any); got["intervalMs"] != want["intervalMs"] || got["timeoutMs"] != want["timeoutMs"] {
		t.Fatalf("autoSolveCloudflare = %#v", got)
	}
	WithAutoSolveCloudflare(50*time.Millisecond, 0)(&o)
	if _, err := o.launchParams(); err == nil {
		t.Fatal("launchParams accepted a 50ms poll interval")
	}
	if params, _ := (options{headless: true}).launchParams(); params["autoSolveCloudflare"] != nil {
		t.Fatalf("autoSolveCloudflare sent without the option: %#v", params)
	}
}

func TestLaunchParamsRejectInvalidProxy(t *testing.T) {
	_, err := (options{proxy: "https://127.0.0.1:1080"}).launchParams()
	if err == nil {
		t.Fatal("launchParams succeeded for an unsupported proxy")
	}
	if got := err.Error(); strings.Contains(got, "127.0.0.1") {
		t.Fatalf("proxy error leaked input: %q", got)
	}
}

func TestLaunchParamsIncludeTimezoneAndViewport(t *testing.T) {
	params, err := (options{
		headless:       true,
		timezone:       "America/Los_Angeles",
		viewportWidth:  1920,
		viewportHeight: 1080,
	}).launchParams()
	if err != nil {
		t.Fatalf("launchParams: %v", err)
	}
	if got, want := params["timezone"], "America/Los_Angeles"; got != want {
		t.Fatalf("timezone = %v, want %v", got, want)
	}
	if got, want := params["viewportWidth"], 1920; got != want {
		t.Fatalf("viewportWidth = %v, want %v", got, want)
	}
	if got, want := params["viewportHeight"], 1080; got != want {
		t.Fatalf("viewportHeight = %v, want %v", got, want)
	}
}

func TestLaunchParamsIncludeUserDataDir(t *testing.T) {
	params, err := (options{headless: true, userDataDir: "/private/profile"}).launchParams()
	if err != nil {
		t.Fatalf("launchParams: %v", err)
	}
	if got, want := params["userDataDir"], "/private/profile"; got != want {
		t.Fatalf("userDataDir = %v, want %v", got, want)
	}
}

func TestLaunchParamsRejectIncompleteViewport(t *testing.T) {
	_, err := (options{viewportWidth: 1920}).launchParams()
	if err == nil || !strings.Contains(err.Error(), "viewport") {
		t.Fatalf("launchParams error = %v, want viewport validation error", err)
	}
}

func TestBrowserCloseTabReturnsCleanupErrorAfterClose(t *testing.T) {
	b := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method != "browser.close_tab" {
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
		var p struct {
			PageID string `json:"pageId"`
		}
		_ = json.Unmarshal(params, &p)
		if p.PageID != "PAGE1" {
			return nil, &rpcError{Code: ErrCodeInvalidPage, Message: "unknown pageId"}
		}
		return map[string]any{"cleanupError": "CDP error: release failed"}, nil
	})

	err := b.CloseTab(context.Background(), "PAGE1")
	if err == nil || !strings.Contains(err.Error(), "browser closed tab") || !strings.Contains(err.Error(), "release failed") {
		t.Fatalf("CloseTab cleanup error = %v", err)
	}
}

func TestBrowserNewPageTabsCloseTab(t *testing.T) {
	var lastNewPageURL any
	haveURL := false

	b := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "browser.new_page":
			var p struct {
				URL *string `json:"url"`
			}
			_ = json.Unmarshal(params, &p)
			haveURL = true
			if p.URL != nil {
				lastNewPageURL = *p.URL
			} else {
				lastNewPageURL = nil
			}
			return map[string]any{"pageId": "PAGE1"}, nil
		case "browser.tabs":
			return map[string]any{"tabs": []TabInfo{{ID: "PAGE1", Title: "Example", URL: "https://example.com/"}}}, nil
		case "browser.close_tab":
			var p struct {
				PageID string `json:"pageId"`
			}
			_ = json.Unmarshal(params, &p)
			if p.PageID != "PAGE1" {
				return nil, &rpcError{Code: ErrCodeInvalidPage, Message: "unknown pageId"}
			}
			return map[string]any{}, nil
		}
		return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
	})

	ctx := context.Background()

	page, err := b.NewPage(ctx, "https://example.com")
	if err != nil {
		t.Fatalf("NewPage: %v", err)
	}
	if page.ID() != "PAGE1" {
		t.Fatalf("unexpected page id: %s", page.ID())
	}
	if !haveURL || lastNewPageURL != "https://example.com" {
		t.Fatalf("expected url param %q, got %v", "https://example.com", lastNewPageURL)
	}

	if _, err := b.NewPage(ctx, ""); err != nil {
		t.Fatalf("NewPage(empty): %v", err)
	}
	if lastNewPageURL != nil {
		t.Fatalf("expected null url param for empty string, got %v", lastNewPageURL)
	}

	tabs, err := b.Tabs(ctx)
	if err != nil {
		t.Fatalf("Tabs: %v", err)
	}
	if len(tabs) != 1 || tabs[0].ID != "PAGE1" || tabs[0].Title != "Example" {
		t.Fatalf("unexpected tabs: %+v", tabs)
	}

	if err := b.CloseTab(ctx, "PAGE1"); err != nil {
		t.Fatalf("CloseTab: %v", err)
	}
	if err := b.CloseTab(ctx, "NOPE"); !HasCode(err, ErrCodeInvalidPage) {
		t.Fatalf("expected InvalidPage error, got %v", err)
	}
}

func TestLaunchRejectsInvalidManagedOptionsBeforeStartingSidecar(t *testing.T) {
	cases := []struct {
		name string
		opt  Option
		want string
	}{
		{"relative executable", WithExecutablePath("chrome"), "executable path"},
		{"empty executable", WithExecutablePath(""), "executable path"},
		{"relative runtime", WithRuntimeDir("runtime"), "runtime directory"},
		{"empty runtime", WithRuntimeDir(""), "runtime directory"},
		{"empty viewport", WithViewport(0, 0), "viewport"},
		{"negative viewport", WithViewport(-1, 360), "viewport"},
		{"incomplete viewport", WithViewport(640, 0), "viewport"},
		{"zero scale", WithDeviceScaleFactor(0), "scale factor"},
		{"negative scale", WithDeviceScaleFactor(-2), "scale factor"},
		{"nan scale", WithDeviceScaleFactor(math.NaN()), "scale factor"},
		{"positive infinite scale", WithDeviceScaleFactor(math.Inf(1)), "scale factor"},
		{"negative infinite scale", WithDeviceScaleFactor(math.Inf(-1)), "scale factor"},
	}
	if maxInt := int(^uint(0) >> 1); int64(maxInt) > math.MaxInt32 {
		cases = append(cases, struct {
			name string
			opt  Option
			want string
		}{"oversized viewport", WithViewport(maxInt, 360), "viewport"})
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			b, err := Launch(context.Background(), WithServerPath(filepath.Join(t.TempDir(), "missing-sidecar")), tc.opt)
			if b != nil || err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("invalid option reached sidecar startup: browser=%v error=%v", b, err)
			}
		})
	}
}

func TestConnectRejectsLaunchOnlyOptionsBeforeStartingSidecar(t *testing.T) {
	for name, opt := range map[string]Option{
		"headless":         WithHeadless(true),
		"visible":          WithVisible(),
		"user agent":       WithUserAgent(""),
		"timezone":         WithTimezone(""),
		"viewport":         WithViewport(640, 360),
		"proxy":            WithProxy(""),
		"profile":          WithUserDataDir(""),
		"executable":       WithExecutablePath(""),
		"runtime":          WithRuntimeDir(""),
		"sandbox enabled":  WithSandbox(true),
		"sandbox disabled": WithSandbox(false),
		"scale":            WithDeviceScaleFactor(1),
	} {
		t.Run(name, func(t *testing.T) {
			b, err := Connect(context.Background(), "ws://localhost:9222/devtools/browser/id",
				WithServerPath(filepath.Join(t.TempDir(), "missing-sidecar")), opt)
			if b != nil || err == nil || !strings.Contains(err.Error(), "launch options") {
				t.Fatalf("Connect accepted launch-only option: browser=%v error=%v", b, err)
			}
		})
	}
}

func TestConnectRejectsInvalidEndpointsBeforeStartingSidecar(t *testing.T) {
	for _, endpoint := range []string{
		"", "http://localhost:9222/devtools/browser/id", "ws://localhost:9222",
		"ws://localhost:9222/devtools/page/id", "ws://localhost:9222/devtools/browser/",
		"ws://localhost:9222/devtools/browser/id/extra", "ws:///devtools/browser/id",
		"ws://user:password@localhost:9222/devtools/browser/id",
		"ws://localhost:9222/devtools/browser/id?token=secret",
		"ws://localhost:9222/devtools/browser/id#fragment",
	} {
		t.Run(endpoint, func(t *testing.T) {
			b, err := Connect(context.Background(), endpoint, WithServerPath(filepath.Join(t.TempDir(), "missing-sidecar")))
			if b != nil || err == nil || !strings.Contains(err.Error(), "DevTools endpoint") {
				t.Fatalf("invalid endpoint reached sidecar startup: browser=%v error=%v", b, err)
			}
		})
	}
}

func TestConnectRejectsUnsupportedTLSBeforeStartingSidecar(t *testing.T) {
	b, err := Connect(context.Background(), "wss://localhost:9222/devtools/browser/id",
		WithServerPath(filepath.Join(t.TempDir(), "missing-sidecar")))
	if b != nil || err == nil || !strings.Contains(err.Error(), "wss:// is not supported") {
		t.Fatalf("TLS endpoint reached sidecar startup: browser=%v error=%v", b, err)
	}
}
