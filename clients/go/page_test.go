package eoka

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"math"
	"sync/atomic"
	"testing"
	"time"
)

func newFakePage(t *testing.T, handle fakeHandler) *Page {
	t.Helper()
	b := newFakeBrowser(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var p map[string]any
		_ = json.Unmarshal(params, &p)
		if p["pageId"] != "P1" {
			return nil, &rpcError{Code: ErrCodeInvalidPage, Message: "bad pageId"}
		}
		return handle(id, method, params)
	})
	return &Page{id: "P1", b: b}
}

func TestPageNoResultMethods(t *testing.T) {
	seen := map[string]bool{}
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var request map[string]any
		_ = json.Unmarshal(params, &request)
		if method == "actions.perform" {
			a := request["action"].(map[string]any)
			switch a["kind"] {
			case "navigate":
				method = "page.goto"
			case "click":
				method = "page.click"
				if a["human"] == true {
					method = "page.human_click"
				}
			case "fill":
				method = "page.fill"
				if a["human"] == true {
					method = "page.human_fill"
				}
			case "select_option":
				method = "page.select"
			case "type":
				method = "page.type_into"
			default:
				method = "page." + a["kind"].(string)
			}
		}
		seen[method] = true
		if operationID, ok := request["operation_id"]; ok {
			return map[string]any{"id": operationID, "state": "finished", "outcome": "verified_complete"}, nil
		}
		return map[string]any{}, nil
	})

	ctx := context.Background()
	checks := []struct {
		name string
		call func() error
	}{
		{"page.goto", func() error { return page.Goto(ctx, "https://example.com") }},
		{"page.click", func() error { return page.Click(ctx, "#go") }},
		{"page.human_click", func() error { return page.HumanClick(ctx, "#go") }},
		{"page.fill", func() error { return page.Fill(ctx, "#user", "bob") }},
		{"page.human_fill", func() error { return page.HumanFill(ctx, "#user", "bob") }},
		{"page.type_into", func() error { return page.TypeInto(ctx, "#user", "bob") }},
		{"page.wait_for", func() error { return page.WaitFor(ctx, "#go", time.Second) }},
		{"page.wait_for_visible", func() error { return page.WaitForVisible(ctx, "#go", time.Second) }},
		{"page.wait_for_text", func() error { return page.WaitForText(ctx, "hello", time.Second) }},
		{"page.execute", func() error { return page.Execute(ctx, "1+1") }},
		{"page.select", func() error { return page.Select(ctx, "#opt", "a") }},
		{"page.hover", func() error { return page.Hover(ctx, "#go") }},
		{"page.press_key", func() error { return page.PressKey(ctx, "Enter") }},
		{"page.mouse_down", func() error { return page.MouseDown(ctx, 10, 20, MouseButtonLeft) }},
		{"page.mouse_move", func() error { return page.MouseMove(ctx, 20, 30) }},
		{"page.mouse_up", func() error { return page.MouseUp(ctx, 20, 30, MouseButtonLeft) }},
		{"page.key_down", func() error { return page.KeyDown(ctx, "Shift") }},
		{"page.key_up", func() error { return page.KeyUp(ctx, "Shift") }},
		{"page.release_all_inputs", func() error { return page.ReleaseAllInputs(ctx) }},
		{"page.close", func() error { return page.Close(ctx) }},
	}

	for _, c := range checks {
		if err := c.call(); err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		if !seen[c.name] {
			t.Fatalf("%s: server never saw the expected method", c.name)
		}
	}
}

func TestPageGetters(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "page.text":
			return map[string]any{"text": "hello world"}, nil
		case "page.content":
			return map[string]any{"html": "<html></html>"}, nil
		case "page.title":
			return map[string]any{"title": "Example Domain"}, nil
		case "page.url":
			return map[string]any{"url": "https://example.com/"}, nil
		case "page.get_text":
			return map[string]any{"text": "Click me"}, nil
		case "page.exists":
			return map[string]any{"exists": true}, nil
		}
		return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
	})

	ctx := context.Background()

	if v, err := page.Text(ctx); err != nil || v != "hello world" {
		t.Fatalf("Text: %q, %v", v, err)
	}
	if v, err := page.Content(ctx); err != nil || v != "<html></html>" {
		t.Fatalf("Content: %q, %v", v, err)
	}
	if v, err := page.Title(ctx); err != nil || v != "Example Domain" {
		t.Fatalf("Title: %q, %v", v, err)
	}
	if v, err := page.URL(ctx); err != nil || v != "https://example.com/" {
		t.Fatalf("URL: %q, %v", v, err)
	}
	if v, err := page.GetText(ctx, "a"); err != nil || v != "Click me" {
		t.Fatalf("GetText: %q, %v", v, err)
	}
	if v, err := page.Exists(ctx, "a"); err != nil || !v {
		t.Fatalf("Exists: %v, %v", v, err)
	}
}

func TestPageGetAttribute(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var p struct {
			Name string `json:"name"`
		}
		_ = json.Unmarshal(params, &p)
		if p.Name == "href" {
			return map[string]any{"value": "https://example.com/more"}, nil
		}
		return map[string]any{"value": nil}, nil
	})

	ctx := context.Background()

	v, ok, err := page.GetAttribute(ctx, "a", "href")
	if err != nil || !ok || v != "https://example.com/more" {
		t.Fatalf("GetAttribute(href): %q, %v, %v", v, ok, err)
	}

	v, ok, err = page.GetAttribute(ctx, "a", "data-missing")
	if err != nil || ok || v != "" {
		t.Fatalf("GetAttribute(data-missing): %q, %v, %v", v, ok, err)
	}
}

func TestPageEvaluate(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var p struct {
			JS string `json:"js"`
		}
		_ = json.Unmarshal(params, &p)
		switch p.JS {
		case "1+1":
			return map[string]any{"result": 2}, nil
		case "document.title":
			return map[string]any{"result": "Example Domain"}, nil
		}
		return map[string]any{"result": nil}, nil
	})

	ctx := context.Background()

	raw, err := page.Evaluate(ctx, "1+1")
	if err != nil || string(raw) != "2" {
		t.Fatalf("Evaluate: %s, %v", raw, err)
	}

	title, err := EvaluateAs[string](ctx, page, "document.title")
	if err != nil || title != "Example Domain" {
		t.Fatalf("EvaluateAs[string]: %q, %v", title, err)
	}

	n, err := EvaluateAs[int](ctx, page, "1+1")
	if err != nil || n != 2 {
		t.Fatalf("EvaluateAs[int]: %d, %v", n, err)
	}
}

func TestPageFetch(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method != "page.fetch" {
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
		var request struct {
			URL     string            `json:"url"`
			Method  string            `json:"method"`
			Headers map[string]string `json:"headers"`
			Body    string            `json:"body"`
		}
		if err := json.Unmarshal(params, &request); err != nil {
			t.Fatal(err)
		}
		if request.URL != "https://example.com/api" || request.Method != "POST" || request.Headers["Accept"] != "application/json" || request.Body != `{"ok":true}` {
			t.Fatalf("unexpected fetch request: %+v", request)
		}
		return map[string]any{
			"url": "https://example.com/api", "status": 201, "ok": true,
			"headers": map[string]string{"content-type": "application/json"}, "body": `{"id":1}`,
		}, nil
	})

	response, err := page.Fetch(context.Background(), "https://example.com/api", FetchOptions{
		Method: "POST", Headers: map[string]string{"Accept": "application/json"}, Body: `{"ok":true}`,
	})
	if err != nil {
		t.Fatal(err)
	}
	if !response.OK || response.Status != 201 || response.Body != `{"id":1}` {
		t.Fatalf("unexpected fetch response: %+v", response)
	}
}

func TestPageSolveCaptchaReturnsSolverUserAgent(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method != "page.solve_captcha" {
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
		return map[string]any{"injection": map[string]any{
			"kind":            "recaptcha",
			"updated_count":   1,
			"solverUserAgent": "Mozilla/5.0 solver",
		}}, nil
	})

	injection, err := page.SolveCaptcha(context.Background(), CaptchaOptions{
		APIKey:     "test-key",
		Mode:       "anti_captcha_proxyless",
		Type:       "recaptcha_v2_enterprise",
		WebsiteURL: "https://example.com/login",
		WebsiteKey: "site-key",
	})
	if err != nil {
		t.Fatal(err)
	}
	if injection.SolverUserAgent != "Mozilla/5.0 solver" {
		t.Fatalf("solver UA = %q", injection.SolverUserAgent)
	}
}

func TestPageState(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "page.capture_state":
			return map[string]any{"state": map[string]any{
				"cookies": []map[string]any{{
					"name": "session", "value": "secret", "domain": ".example.com", "path": "/", "secure": true, "http_only": true,
				}},
				"localStorage":   map[string]string{"token": "value"},
				"sessionStorage": map[string]string{"tab": "value"},
				"userAgent":      "Mozilla/5.0",
				"url":            "https://example.com/account",
			}}, nil
		case "page.restore_state":
			var p struct {
				State BrowserState `json:"state"`
			}
			if err := json.Unmarshal(params, &p); err != nil {
				return nil, &rpcError{Code: ErrCodeInvalidParams, Message: err.Error()}
			}
			if len(p.State.Cookies) != 1 || !p.State.Cookies[0].HTTPOnly || p.State.URL != "https://example.com/account" {
				return nil, &rpcError{Code: ErrCodeInvalidParams, Message: "invalid state"}
			}
			return map[string]any{}, nil
		}
		return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
	})

	state, err := page.CaptureState(context.Background())
	if err != nil || !state.Cookies[0].HTTPOnly || state.LocalStorage["token"] != "value" {
		t.Fatalf("CaptureState: %#v, %v", state, err)
	}
	if err := page.RestoreState(context.Background(), state); err != nil {
		t.Fatalf("RestoreState: %v", err)
	}
}

func TestPageScreenshot(t *testing.T) {
	want := []byte("\x89PNGfake-image-bytes")
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return map[string]any{"dataBase64": base64.StdEncoding.EncodeToString(want)}, nil
	})

	got, err := page.Screenshot(context.Background())
	if err != nil {
		t.Fatalf("Screenshot: %v", err)
	}
	if string(got) != string(want) {
		t.Fatalf("Screenshot bytes mismatch: got %q, want %q", got, want)
	}
}

func TestPageErrorPropagation(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		return nil, &rpcError{Code: ErrCodeElementNotVisible, Message: "not visible"}
	})

	err := page.Click(context.Background(), "#hidden")
	if !HasCode(err, ErrCodeElementNotVisible) {
		t.Fatalf("expected ElementNotVisible, got %v", err)
	}
}

func TestPageViewportSendsCSSDimensionsAndScaleAndReset(t *testing.T) {
	var sets, resets atomic.Int32
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		switch method {
		case "page.set_viewport":
			sets.Add(1)
			var request struct {
				PageID string  `json:"pageId"`
				Width  int     `json:"width"`
				Height int     `json:"height"`
				Scale  float64 `json:"deviceScaleFactor"`
			}
			if err := json.Unmarshal(params, &request); err != nil || request.PageID != "P1" || request.Width != 640 || request.Height != 360 || request.Scale != 2 {
				t.Errorf("viewport wire = %s, %v", params, err)
			}
		case "page.reset_viewport":
			resets.Add(1)
			if string(params) != `{"pageId":"P1"}` {
				t.Errorf("reset wire = %s", params)
			}
		default:
			return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: method}
		}
		return map[string]any{}, nil
	})
	ctx := callContext(t)
	if err := page.SetViewport(ctx, 640, 360, 2); err != nil {
		t.Fatal(err)
	}
	if err := page.ResetViewport(ctx); err != nil {
		t.Fatal(err)
	}
	if sets.Load() != 1 || resets.Load() != 1 {
		t.Fatalf("viewport dispatches = %d sets, %d resets", sets.Load(), resets.Load())
	}
}

func TestPageViewportValidationPreventsDispatch(t *testing.T) {
	var dispatched atomic.Int32
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		dispatched.Add(1)
		return map[string]any{}, nil
	})
	for _, dimensions := range [][2]int{{0, 360}, {640, 0}, {-1, 360}, {640, -1}} {
		if err := page.SetViewport(callContext(t), dimensions[0], dimensions[1], 1); err == nil {
			t.Fatalf("invalid dimensions accepted: %v", dimensions)
		}
	}
	if maxInt := int(^uint(0) >> 1); int64(maxInt) > math.MaxInt32 {
		if err := page.SetViewport(callContext(t), maxInt, 360, 1); err == nil {
			t.Fatal("oversized width accepted")
		}
		if err := page.SetViewport(callContext(t), 640, maxInt, 1); err == nil {
			t.Fatal("oversized height accepted")
		}
	}
	for _, scale := range []float64{0, -1, math.NaN(), math.Inf(1), math.Inf(-1)} {
		if err := page.SetViewport(callContext(t), 640, 360, scale); err == nil {
			t.Fatalf("invalid scale accepted: %v", scale)
		}
	}
	if dispatched.Load() != 0 {
		t.Fatalf("invalid viewport dispatched %d requests", dispatched.Load())
	}
}

func TestPageViewportRequiresAdvertisedMethods(t *testing.T) {
	info := fakeServerInfo()
	info.Methods = []string{"server.hello", "server.shutdown", "browser.launch"}
	var dispatched atomic.Int32
	b := &Browser{t: newFakeTransport(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method == "server.hello" {
			return info, nil
		}
		dispatched.Add(1)
		return map[string]any{}, nil
	})}
	ctx := callContext(t)
	if err := b.hello(ctx); err != nil {
		t.Fatal(err)
	}
	page := b.Page("P1")
	if err := page.SetViewport(ctx, 640, 360, 2); !HasCode(err, ErrCodeUnknownMethod) {
		t.Fatalf("unadvertised set viewport = %v", err)
	}
	if err := page.ResetViewport(ctx); !HasCode(err, ErrCodeUnknownMethod) {
		t.Fatalf("unadvertised reset viewport = %v", err)
	}
	if dispatched.Load() != 0 {
		t.Fatalf("unadvertised viewport dispatched %d requests", dispatched.Load())
	}
}
