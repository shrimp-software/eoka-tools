import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit, urlunsplit


class Failure(Exception):
    def __init__(self, message, code=1):
        super().__init__(message)
        self.code = code


def lesson_url(value):
    parsed = urlsplit(value)
    if parsed.scheme != "https" or parsed.netloc != "app.sketchy.com" or "/lesson/" not in parsed.path:
        raise Failure("Expected an https://app.sketchy.com/.../lesson/... URL", 2)
    return urlunsplit((parsed.scheme, parsed.netloc, parsed.path.rstrip("/"), parsed.query, ""))


class Eoka:
    def __init__(self, session, binary="eoka"):
        self.session = session
        self.binary = binary

    def run(self, *args):
        try:
            result = subprocess.run([self.binary, "--json", "--session", self.session, *args], text=True, capture_output=True, timeout=90)
        except subprocess.TimeoutExpired as error:
            raise Failure("Browser response timed out; inspect the existing session before retrying", 4) from error
        except OSError as error:
            raise Failure("Could not launch eoka; install the protocol 2 CLI", 2) from error
        try:
            envelope = json.loads(result.stdout)
        except ValueError as error:
            raise Failure("CLI did not return structured JSON; upgrade eoka", 2) from error
        if result.returncode or not envelope.get("ok"):
            data = envelope.get("data") or {}
            identity = data.get("operation_id") or data.get("id")
            code = (envelope.get("error_detail") or {}).get("code", "browser_error")
            hint = f"; inspect operation {identity}, do not replay" if identity else ""
            raise Failure(f"Browser command failed: {code}{hint}", result.returncode or 1)
        return envelope.get("data")

    def evaluate(self, function, *arguments):
        return self.run("eval", function, "--args", json.dumps(arguments))

    def require_session(self):
        doctor = self.run("doctor")
        if not doctor.get("daemon_running"):
            raise Failure(f"Session {self.session!r} is not running. Open and authenticate it first")
        if (doctor.get("daemon") or {}).get("protocol_major") != 2:
            raise Failure("This helper requires protocol 2 on both the CLI and the running daemon", 2)

    def navigate(self, requested):
        requested = lesson_url(requested)
        current = self.evaluate("()=>location.href")
        if current != requested:
            self.run("open", requested)
        actual = self.evaluate("()=>location.href")
        if "/login" in urlsplit(actual).path:
            raise Failure("Authentication is required in the selected session")
        if lesson_url(actual) != requested:
            raise Failure("Navigation reached a different lesson; refusing its video metadata")
        condition = {"kind": "all", "conditions": [
            {"kind": "url", "value": requested},
            {"kind": "visible", "target": {"query": {"kind": "css", "selector": "[data-video-id]"}}},
        ]}
        self.run("--expect", json.dumps(condition), "wait", "--timeout", "30000")
        return requested


VIDEO = """()=>{
  const nodes=[...document.querySelectorAll('[data-video-id]')];
  const ids=[...new Set(nodes.map(n=>n.getAttribute('data-video-id')).filter(Boolean))];
  const players=Object.values(window.videojs?.getPlayers?.()||{});
  const videos=players.map(p=>p.mediainfo).filter(v=>v&&ids.includes(String(v.id)));
  return {url:location.href,ids,videos:videos.map(v=>({id:String(v.id),name:v.name,sources:v.sources||[]}))};
}"""


def video_source(metadata, requested):
    if lesson_url(metadata.get("url", "")) != requested:
        raise Failure("Page changed during metadata capture; refusing a different lesson")
    ids = metadata.get("ids", [])
    if len(ids) != 1:
        raise Failure("Expected exactly one lesson video ID; no download was started")
    matches = [video for video in metadata.get("videos", []) if video.get("id") == ids[0]]
    if len(matches) != 1:
        raise Failure("Authorized player metadata is unavailable. Load the lesson player in the selected session first")
    sources = matches[0].get("sources", [])
    candidates = []
    for source in sources:
        url = source.get("src", "")
        if source.get("key_systems") or source.get("drm"):
            continue
        if urlsplit(url).scheme not in ("https", "http"):
            continue
        if "mpegurl" in source.get("type", "").lower() or urlsplit(url).path.endswith(".m3u8"):
            candidates.append(url)
    if not candidates:
        raise Failure("No supported HLS source is available in the authorized player metadata")
    return candidates[0]


def download(source, output):
    if urlsplit(source).scheme not in ("https", "http"):
        raise Failure("Expected an HTTP(S) HLS URL", 2)
    output = Path(output).expanduser().absolute()
    if output.exists():
        raise Failure("Output already exists; choose a new path", 2)
    if output.suffix.lower() != ".mp4":
        raise Failure("Output must use the .mp4 extension", 2)
    if not shutil.which("ffmpeg") or not shutil.which("ffprobe"):
        raise Failure("Install ffmpeg and ffprobe before downloading", 2)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".sketchy-", dir=output.parent) as temporary:
        partial = Path(temporary) / "download.mp4"
        result = subprocess.run(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-n", "-i", source, "-c", "copy", "-movflags", "+faststart", str(partial)], capture_output=True)
        if result.returncode:
            raise Failure(f"ffmpeg failed (exit {result.returncode}); no output was published")
        probe = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "json", str(partial)], capture_output=True, text=True)
        try:
            valid = probe.returncode == 0 and partial.stat().st_size > 0 and float(json.loads(probe.stdout)["format"]["duration"]) > 0
        except (ValueError, KeyError, OSError):
            valid = False
        if not valid:
            raise Failure("Downloaded media did not pass ffprobe verification")
        try:
            os.link(partial, output)
        except FileExistsError as error:
            raise Failure("Output appeared during download; it was preserved") from error
    return {"output": str(output), "verified": True}


def main(program=None):
    program = program or Path(sys.argv[0]).name
    parser = argparse.ArgumentParser(prog=program)
    parser.add_argument("--session", default=os.environ.get("EOKA_SESSION"))
    parser.add_argument("--eoka", default="eoka")
    if program != "sketchy-list-lessons":
        parser.add_argument("url")
        parser.add_argument("--out", required=True)
    args = parser.parse_args()
    try:
        if program == "sketchy-m3u8dl":
            result = download(args.url, args.out)
        else:
            if not args.session:
                raise Failure("Specify --session or EOKA_SESSION; helpers never choose another browser session", 2)
            browser = Eoka(args.session, args.eoka)
            browser.require_session()
            if program == "sketchy-list-lessons":
                result = browser.evaluate("()=>({url:location.href,lessons:[...new Map([...document.querySelectorAll('a[href*=\"/lesson/\"]')].map(a=>[a.href,{url:a.href,title:a.textContent.trim()}])).values()]})")
                if urlsplit(result["url"]).netloc != "app.sketchy.com" or not result["lessons"]:
                    raise Failure("No lesson links found. Open an authenticated Sketchy lesson list in this session")
            else:
                requested = browser.navigate(args.url)
                source = video_source(browser.evaluate(VIDEO), requested)
                result = download(source, args.out)
        print(json.dumps(result))
    except Failure as error:
        print(str(error), file=sys.stderr)
        return error.code
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f"Helper failed ({type(error).__name__}); no completion was established", file=sys.stderr)
        return 1
    return 0
