const lingerMs = Number(process.env.EOKA_TRANSPORT_TEST_LINGER_MS);
process.on('SIGTERM', () => {});
if (Number.isFinite(lingerMs)) process.once('disconnect', () => setTimeout(() => process.exit(0), lingerMs));
setInterval(() => {}, 1000);
process.send?.({ pid: process.pid });
