import assert from 'node:assert/strict';
import { access, mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { Browser, RecordingError, releaseManifest } from '@cbxss/eoka';

const directory = resolve('captures');
await mkdir(directory);
const browser = await Browser.launch({
  download: 'never',
  browserCacheDir: process.env.EOKA_BROWSER_CACHE_DIR,
  viewport: { width: 640, height: 480, deviceScaleFactor: 2 },
  timeoutMs: 90000,
});
try {
  const html = '<!doctype html><title>offline-staged</title><style>body{margin:0;background:#123;height:1200px}.box{width:80px;height:80px;background:orange;animation:move 1s alternate infinite linear;animation-play-state:paused}@keyframes move{from{transform:translateX(0)}to{transform:translateX(500px)}}</style><div class="box"></div>';
  const page = await browser.newPage(`data:text/html,${encodeURIComponent(html)}`);
  const title = await page.title();
  const clip = { x: 0, y: 0, width: 128, height: 64 };
  const png = await page.screenshot({ clip, signal: new AbortController().signal, timeoutMs: 60000 });
  assert.equal(png.readUInt32BE(16), 256);
  assert.equal(png.readUInt32BE(20), 128);
  const image = await page.screenshotToFile({ path: 'captures/crop.png', clip });
  assert.equal(image.path, `${directory}/crop.png`);
  assert.deepEqual(await readFile(image.path), png);
  assert.equal(image.bytes, png.length);
  for (const format of ['jpeg', 'webp']) {
    const bytes = await page.screenshot({ format, quality: 80, clip });
    if (format === 'jpeg') assert.equal(bytes.readUInt16BE(0), 0xffd8);
    else assert.equal(bytes.toString('ascii', 8, 12), 'WEBP');
  }
  const fullPage = await page.screenshotToFile({ path: `${directory}/full.png`, fullPage: true });
  const fullImage = await readFile(fullPage.path);
  assert.equal(fullImage.readUInt32BE(16), 1280);
  assert.equal(fullImage.readUInt32BE(20), 2400);
  const existing = `${directory}/existing.png`;
  await writeFile(existing, 'do-not-replace');
  await assert.rejects(page.screenshotToFile({ path: existing }), error => error.code === 'FileOutput');
  assert.equal(await readFile(existing, 'utf8'), 'do-not-replace');
  await page.execute("document.querySelector('.box').style.animationPlayState='running'");
  const recording = await page.record({ path: `${directory}/animation.mp4`, maxWidth: 640, maxHeight: 480, fps: 30 });
  await setTimeout(1000);
  const video = await recording.stop({ timeoutMs: 60000 });
  assert.equal(video.container, 'mp4');
  assert.equal(video.mimeType, 'video/mp4');
  const videoBytes = await readFile(video.path);
  assert.equal(video.bytes, videoBytes.length);
  assert.equal(videoBytes.toString('ascii', 4, 8), 'ftyp');
  assert.deepEqual(await recording.stop(), video);
  const recovered = browser.recording(recording.id);
  assert.equal((await recovered.status()).state, 'completed');
  assert.deepEqual(await recovered.stop(), video);
  const cancelled = await page.record({ path: `${directory}/cancelled.mp4` });
  assert.equal((await cancelled.cancel()).state, 'cancelled');
  await assert.rejects(cancelled.stop(), error => error instanceof RecordingError && error.record.state === 'cancelled');
  await assert.rejects(access(`${directory}/cancelled.mp4`), { code: 'ENOENT' });
  await page.setViewport({ width: 2500, height: 2500, deviceScaleFactor: 1 });
  await page.execute(`(() => {
    document.body.replaceChildren();
    document.body.style.height = '2500px';
    const canvas = document.createElement('canvas');
    canvas.width = 2500;
    canvas.height = 2500;
    canvas.style.display = 'block';
    document.body.appendChild(canvas);
    const context = canvas.getContext('2d');
    const image = context.createImageData(2500, 2500);
    let noise = 0x12345678;
    for (let offset = 0; offset < image.data.length; offset += 4) {
      noise ^= noise << 13;
      noise ^= noise >>> 17;
      noise ^= noise << 5;
      image.data[offset] = noise & 255;
      image.data[offset + 1] = (noise >>> 8) & 255;
      image.data[offset + 2] = (noise >>> 16) & 255;
      image.data[offset + 3] = 255;
    }
    context.putImageData(image, 0, 0);
  })()`);
  await assert.rejects(page.screenshot({ format: 'png', timeoutMs: 30000 }), error => {
    assert.equal(error.code, 'ScreenshotTooLarge');
    assert.match(error.message, /screenshot_to_file/);
    return true;
  });
  assert.equal(await page.title(), title);
  const oversizedImage = await page.screenshotToFile({ path: `${directory}/noise.png`, format: 'png', timeoutMs: 30000 });
  const oversizedBytes = await readFile(oversizedImage.path);
  assert.ok(oversizedBytes.length > 16 * 1024 * 1024);
  assert.equal(oversizedImage.bytes, oversizedBytes.length);
  assert.equal(oversizedImage.mimeType, 'image/png');
  assert.deepEqual(oversizedBytes.subarray(0, 8), Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
  assert.equal(oversizedBytes.readUInt32BE(16), 2500);
  assert.equal(oversizedBytes.readUInt32BE(20), 2500);
  assert.equal(await page.evaluate("document.querySelector('canvas').width"), 2500);
  console.log(JSON.stringify({ title, ownership: browser.ownership, browserVersion: browser.browserVersion, buildId: releaseManifest.chrome.buildId, image, fullPage, oversizedImage, video, recordingId: recording.id }));
} finally {
  await browser.close();
}
