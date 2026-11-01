# Sketchy helpers

Reconstructed from the investigated scripts. All three commands share `sketchy.py`; install the whole directory. Python 3.10+, protocol 2 Eoka, ffmpeg and ffprobe are required. No account credentials or playback policy keys are embedded. The former `sketchy-dl-eoka` implementation is replaced by `sketchy-dl`.

Open the intended session explicitly and authenticate in that browser:

```sh
eoka --session sketchy --headed open https://app.sketchy.com/login
eoka --session sketchy view
```

Native fill uses sequential character input by default. Email validation on this form requires a native Tab blur; filling a value alone does not prove validation. Fill the password after it appears, and verify that the submit control becomes enabled. Keep passwords out of shell history and saved automation transcripts. These helpers reuse an authenticated session; they never select a session from a launch-mode suffix or restart a browser.

```sh
./sketchy-list-lessons --session sketchy
./sketchy-dl --session sketchy 'https://app.sketchy.com/study/medical/chapter/.../lesson/...' --out lesson.mp4
./sketchy-m3u8dl 'https://authorized-media.example/video.m3u8' --out lesson.mp4
python3 -m unittest discover -s . -p 'test_*.py'
```

`sketchy-list-lessons` reads every lesson link currently loaded in the selected page. It does not claim an exhaustive course inventory beyond that document. `sketchy-dl` navigates to the requested lesson even if another page is already open, checks the URL again during capture, and reads matching authorized Video.js player metadata. A missing player, multiple video IDs, expired source, or unsupported protected source fails explicitly. The helpers do not fetch or embed playback policy keys.

Downloads go to a unique temporary directory beside the destination, pass ffprobe validation, then publish without overwriting an existing file. Failure removes only that temporary directory. Operation errors retain CLI exits 3 (running) and 4 (unknown), with no automatic replay. Automated fixtures cover session routing, wrong-page selection, login redirects, missing metadata and uncertain operations. The authenticated macOS workflow remains a separate release gate.
