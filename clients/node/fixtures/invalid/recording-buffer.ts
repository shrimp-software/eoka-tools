import type { Recording } from "../../dist/index.js";
declare const recording: Recording;
const video: Promise<Buffer> = recording.stop();
void video;
