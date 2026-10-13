import hashlib
import html
import json
import re
import threading
from collections import defaultdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

REPORT = b"name,total\nAva Park,29\nMira Chen,43\n"
TASKS = {
    "pricing": "Find the monthly price of the Pro plan. Report the amount.",
    "form": "Submit the contact form once with name Ava Park, email ava@example.test, and role Analyst. Confirm the result.",
    "tabs": "Open the shipment tracker and report the tracking code. It may open another tab.",
    "download": "Download the CSV report exactly once into {downloads}/report.csv and confirm the saved file.",
    "dynamic": "Prepare the report, then generate it once when the control becomes available. Confirm completion.",
    "pagination": "Find Mira Chen in the directory and report her email address.",
}


class Fixture:
    def __init__(self):
        self.events = defaultdict(list)
        self.visits = defaultdict(set)
        self.non_browser_requests = defaultdict(list)
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def send(self, body, content_type="text/html", attachment=False):
                body = body if isinstance(body, bytes) else body.encode()
                self.send_response(200)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(body)))
                if attachment:
                    self.send_header("Content-Disposition", 'attachment; filename="report.csv"')
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                run = self.path.strip("/").split("/")[0]
                self.audit(run)
                body = self.rfile.read(int(self.headers.get("Content-Length", "0"))).decode()
                fixture.events[run].append({"path": self.path, "body": body})
                self.send(json.dumps({"status": "complete"}), "application/json")

            def do_GET(self):
                parts = urlparse(self.path).path.strip("/").split("/")
                if len(parts) < 2:
                    self.send("Missing fixture")
                    return
                run, task, *rest = parts
                self.audit(run)
                fixture.visits[run].add(urlparse(self.path).path)
                base = f"/{run}/{task}"
                if task == "pricing":
                    body = '<h1>Plans</h1><h2>Basic</h2><p>$12 per month</p><h2>Pro</h2><p>$29 per month</p><h2>Team</h2><p>$79 per month</p>' if rest else f'<h1>Acme workspace</h1><a href="{base}/plans">Plans and pricing</a>'
                elif task == "form":
                    body = '<h1>Contact us</h1><form id=f><label>Name<input name=name required></label><label>Email<input name=email type=email required></label><label>Role<select name=role><option value=engineer>Engineer</option><option value=analyst>Analyst</option></select></label><button>Send request</button></form><p id=status role=status></p><script>f.onsubmit=async e=>{e.preventDefault();await fetch(location.pathname,{method:"POST",body:new URLSearchParams(new FormData(f))});document.querySelector("#status").textContent="Request received for Ava Park"}</script>'
                elif task == "tabs":
                    body = '<h1>Shipment tracker</h1><p>Tracking code: ZX-4821</p>' if rest else f'<h1>Shipments</h1><a target="_blank" href="{base}/tracker">Shipment tracker</a>'
                elif task == "download":
                    if rest:
                        fixture.events[run].append({"path": self.path, "download": True})
                        self.send(REPORT, "text/csv", attachment=True)
                        return
                    body = f'<h1>Reports</h1><a href="{base}/report.csv">Download CSV report</a>'
                elif task == "dynamic":
                    body = '<h1>Report generator</h1><button id=prepare>Prepare report</button><section id=slot></section><p id=status role=status>Not prepared</p><script>prepare.onclick=()=>{prepare.disabled=true;slot.innerHTML="<button disabled>Generate report</button>";document.querySelector("#status").textContent="Preparing";setTimeout(()=>{slot.innerHTML="<button id=generate>Generate report</button>";generate.onclick=async()=>{await fetch(location.pathname,{method:"POST",body:"generate"});document.querySelector("#status").textContent="Report generated"}},1200)}</script>'
                elif task == "pagination":
                    page = int(rest[0]) if rest else 1
                    name, email = [("Alex Kim", "alex@example.test"), ("Leo Patel", "leo@example.test"), ("Mira Chen", "mira@example.test")][min(page, 3) - 1]
                    body = f'<h1>Staff directory</h1><p>Page {page} of 3</p><h2>{name}</h2><p>{email}</p>'
                    if page < 3:
                        body += f'<a href="{base}/{page + 1}">Next page</a>'
                else:
                    body = "Unknown task"
                self.send(f'<!doctype html><title>{html.escape(task)}</title><style>body{{font:18px sans-serif;padding:32px}}input,select,button,a{{display:block;margin:14px 0;padding:10px}}</style>{body}')

            def audit(self, run):
                user_agent = self.headers.get("User-Agent", "")
                if "Chrome/" not in user_agent:
                    fixture.non_browser_requests[run].append({"path": self.path, "user_agent": user_agent})

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def start(self):
        self.thread.start()
        return self

    def url(self, run, task):
        return f"http://127.0.0.1:{self.server.server_port}/{run}/{task}"

    def score(self, run, task, answer, downloads):
        events = self.events[run]
        effects = len(events)
        if task == "pricing":
            success = bool(re.search(r"(?<!\d)29(?:\.00)?(?!\d)", answer)) and f"/{run}/pricing/plans" in self.visits[run]
        elif task == "tabs":
            success = "ZX-4821" in answer and f"/{run}/tabs/tracker" in self.visits[run]
        elif task == "pagination":
            success = "mira@example.test" in answer and f"/{run}/pagination/3" in self.visits[run]
        elif task == "form":
            success = effects == 1 and parse_qs(events[0]["body"]) == {"name": ["Ava Park"], "email": ["ava@example.test"], "role": ["analyst"]}
        elif task == "dynamic":
            success = effects == 1 and events[0]["body"] == "generate"
        else:
            path = downloads / "report.csv"
            success = effects == 1 and path.is_file() and path.read_bytes() == REPORT
        return {"success": success, "valid": not self.non_browser_requests[run], "non_browser_requests": self.non_browser_requests[run], "side_effects": effects, "duplicate_effects": max(0, effects - 1), "events": events,
                "expected_download_sha256": hashlib.sha256(REPORT).hexdigest() if task == "download" else None}
