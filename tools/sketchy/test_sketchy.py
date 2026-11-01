import json
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest
from unittest.mock import patch
from sketchy import Eoka, Failure, download, video_source

LESSON = "https://app.sketchy.com/study/medical/chapter/test/lesson/requested"
OTHER = "https://app.sketchy.com/study/medical/chapter/test/lesson/other"


class Helpers(unittest.TestCase):
    def test_wrong_lesson_is_navigated_even_when_current_session_works(self):
        browser = Eoka("chosen")
        with patch.object(browser, "evaluate", side_effect=[OTHER, LESSON]), patch.object(browser, "run") as run:
            self.assertEqual(browser.navigate(LESSON), LESSON)
        self.assertEqual(run.call_args_list[0].args, ("open", LESSON))
        condition = json.loads(run.call_args_list[1].args[1])
        self.assertEqual(condition["conditions"][0]["value"], LESSON)

    def test_login_redirect_has_a_useful_error(self):
        browser = Eoka("chosen")
        with patch.object(browser, "evaluate", side_effect=[OTHER, "https://app.sketchy.com/login"]), patch.object(browser, "run"):
            with self.assertRaisesRegex(Failure, "Authentication"):
                browser.navigate(LESSON)

    def test_missing_metadata_fails_explicitly(self):
        for metadata in [{"url": LESSON, "ids": []}, {"url": LESSON, "ids": ["1"], "videos": []}]:
            with self.assertRaises(Failure):
                video_source(metadata, LESSON)

    def test_metadata_belongs_to_requested_lesson_and_video(self):
        metadata = {"url": OTHER, "ids": ["1"], "videos": [{"id": "1", "sources": [{"src": "https://example.test/video.m3u8"}]}]}
        with self.assertRaisesRegex(Failure, "different lesson"):
            video_source(metadata, LESSON)
        metadata["url"] = LESSON
        self.assertEqual(video_source(metadata, LESSON), "https://example.test/video.m3u8")
        metadata["videos"][0]["sources"][0]["key_systems"] = {"example": {}}
        with self.assertRaisesRegex(Failure, "No supported HLS"):
            video_source(metadata, LESSON)

    def test_uncertain_operations_keep_exit_code_without_replay(self):
        output = subprocess.CompletedProcess([], 4, json.dumps({"ok": False, "data": {"operation_id": "operation-1"}, "error_detail": {"code": "outcome_unknown"}}), "")
        with patch("sketchy.subprocess.run", return_value=output) as run:
            with self.assertRaises(Failure) as raised:
                Eoka("chosen").run("step", "{}")
        self.assertEqual(raised.exception.code, 4)
        self.assertIn("operation-1", str(raised.exception))
        self.assertEqual(run.call_count, 1)
        self.assertEqual(run.call_args.args[0][:4], ["eoka", "--json", "--session", "chosen"])


@unittest.skipUnless(shutil.which("ffmpeg") and shutil.which("ffprobe"), "requires ffmpeg and ffprobe")
class Media(unittest.TestCase):
    def test_hls_download_verification_publication_and_failure_cleanup(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            media = root / "media"
            media.mkdir()
            subprocess.run([
                "ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "color=c=blue:s=128x72:r=10:d=1",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-f", "hls",
                "-hls_time", "1", "-hls_list_size", "0", str(media / "video.m3u8")
            ], check=True, capture_output=True)
            server = ThreadingHTTPServer(("127.0.0.1", 0), partial(SimpleHTTPRequestHandler, directory=str(media)))
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            try:
                source = f"http://127.0.0.1:{server.server_port}/video.m3u8"
                output = root / "result" / "lesson.mp4"
                result = download(source, output)
                self.assertTrue(result["verified"])
                original = output.read_bytes()
                self.assertGreater(len(original), 0)
                with self.assertRaisesRegex(Failure, "already exists"):
                    download(source, output)
                self.assertEqual(output.read_bytes(), original)
                with self.assertRaisesRegex(Failure, "ffmpeg failed"):
                    download(source.replace("video.m3u8", "missing.m3u8"), output.with_name("failed.mp4"))
                self.assertEqual(list(output.parent.iterdir()), [output])
            finally:
                server.shutdown()
                server.server_close()
                worker.join()


if __name__ == "__main__":
    unittest.main()
