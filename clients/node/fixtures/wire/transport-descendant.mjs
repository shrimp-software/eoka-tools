process.on('SIGTERM', () => {});
setInterval(() => {}, 1000);
process.send?.({ pid: process.pid });
