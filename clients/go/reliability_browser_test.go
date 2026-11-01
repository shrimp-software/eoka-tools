package eoka

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
)

func TestReliabilityRealBrowser(t *testing.T) {
	serverPath := os.Getenv("EOKA_RELIABILITY_SERVER")
	if serverPath == "" {
		t.Skip("set EOKA_RELIABILITY_SERVER and EOKA_RELIABILITY_CHROME")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	fixture := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html")
		_, _ = w.Write([]byte(`<label>Name<input id=name oninput="events++"></label><script>window.events=0</script>`))
	}))
	defer fixture.Close()
	browser, err := Launch(ctx, WithServerPath(serverPath), WithExecutablePath(os.Getenv("EOKA_RELIABILITY_CHROME")), WithHeadless(true))
	if err != nil {
		t.Fatal(err)
	}
	defer browser.Close(context.Background())
	page, err := browser.NewPage(ctx, fixture.URL)
	if err != nil {
		t.Fatal(err)
	}
	if err := page.Fill(ctx, "#name", "aé😀"); err != nil {
		t.Fatal(err)
	}
	count, err := EvaluateAs[int](ctx, page, "()=>window.events")
	if err != nil || count != 3 {
		t.Fatalf("sequential input: %d %v", count, err)
	}
	if err := page.FillWithMode(ctx, "#name", "bulk", "bulk"); err != nil {
		t.Fatal(err)
	}
	count, err = EvaluateAs[int](ctx, page, "()=>window.events")
	if err != nil || count != 4 {
		t.Fatalf("bulk replacement must emit one input event: %d %v", count, err)
	}
	for i := 0; i < 2; i++ {
		value, err := page.Evaluate(ctx, "value=>{const local=value;return local}", map[string]any{"quoted": "é😀\""})
		if err != nil {
			t.Fatal(err)
		}
		var result map[string]string
		if err := json.Unmarshal(value, &result); err != nil || result["quoted"] != "é😀\"" {
			t.Fatalf("function args: %s %v", value, err)
		}
	}
	if err := page.Execute(ctx, "async()=>{throw new Error('fixture-exception')}"); err == nil || !strings.Contains(err.Error(), "fixture-exception") {
		t.Fatalf("exception: %v", err)
	}
	view, err := page.AgentObserve(ctx, AgentObserveOptions{})
	if err != nil || len(view.Controls) != 1 || !view.Controls[0].Editable {
		t.Fatalf("view: %#v %v", view, err)
	}
}
