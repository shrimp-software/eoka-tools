import { mkdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { Browser, RecordingError } from '@cbxss/eoka';

if (process.argv[2] === undefined) throw new Error('Usage: node capture.mjs OUTPUT_DIRECTORY [BROWSER_CACHE_DIRECTORY]');
const directory = resolve(process.argv[2]);
await mkdir(directory, { recursive: true });
const browser = await Browser.launch({
  ...(process.argv[3] === undefined ? {} : { browserCacheDir: process.argv[3] }),
  download: 'never',
  headless: true,
  sandbox: true,
  viewport: { width: 800, height: 600, deviceScaleFactor: 1 },
  timeoutMs: 30000,
});
try {
  const scene = '<!doctype html><title>Native capture</title><style>body{margin:0;background:#123;color:white;height:1200px}h1{padding:24px}.box{width:100px;height:100px;background:orange;animation:move 1s alternate infinite linear}@keyframes move{from{transform:translateX(0)}to{transform:translateX(600px)}}</style><h1>Native animated capture</h1><div class="box"></div>';
  const page = await browser.newPage(`data:text/html,${encodeURIComponent(scene)}`);
  const image = await page.screenshot({ format: 'png', clip: { x: 0, y: 0, width: 400, height: 300 } });
  const screenshots = [];
  for (const format of ['png', 'jpeg', 'webp']) {
    screenshots.push(await page.screenshotToFile({ path: join(directory, `page.${format}`), format, fullPage: true }));
  }
  const recording = await page.record({ path: join(directory, 'animation.mp4'), maxWidth: 800, maxHeight: 600, fps: 30 });
  await setTimeout(2000);
  const artifact = await recording.stop({ timeoutMs: 60000 });
  console.log(JSON.stringify({ imageBytes: image.length, screenshots, recordingId: recording.id, artifact }, null, 2));
} catch (error) {
  if (error instanceof RecordingError) {
    console.error(JSON.stringify({ recordingId: error.recordingId, code: error.code, path: error.path, record: error.record, failure: error.failure, partialPath: error.partialPath, cleanupUncertain: error.cleanupUncertain }, null, 2));
  }
  throw error;
} finally {
  await browser.close();
}
