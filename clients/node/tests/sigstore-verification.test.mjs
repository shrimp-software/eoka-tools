import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, sign } from 'node:crypto';
import { once } from 'node:events';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { verify } from 'sigstore';

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
}

const digest = bytes => createHash('sha256').update(bytes).digest('hex');

async function isolatedTrustRepository(trustedRoot) {
  const directory = await mkdtemp(join(tmpdir(), 'eoka-sigstore-contract-'));
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const key = { keytype: 'ed25519', scheme: 'ed25519', keyval: { public: publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('hex') } };
  const keyid = digest(canonical(key));
  const envelope = signed => Buffer.from(JSON.stringify({ signed, signatures: [{ keyid, sig: sign(null, Buffer.from(canonical(signed)), privateKey).toString('hex') }] }));
  const common = { spec_version: '1.0.31', version: 1, expires: '2099-01-01T00:00:00Z' };
  const roles = Object.fromEntries(['root', 'timestamp', 'snapshot', 'targets'].map(role => [role, { keyids: [keyid], threshold: 1 }]));
  const root = envelope({ ...common, _type: 'root', consistent_snapshot: false, keys: { [keyid]: key }, roles });
  const artifact = Buffer.from(JSON.stringify(trustedRoot));
  const targets = envelope({ ...common, _type: 'targets', targets: { 'trusted_root.json': { length: artifact.length, hashes: { sha256: digest(artifact) } } } });
  const snapshot = envelope({ ...common, _type: 'snapshot', meta: { 'targets.json': { version: 1, length: targets.length, hashes: { sha256: digest(targets) } } } });
  const timestamp = envelope({ ...common, _type: 'timestamp', meta: { 'snapshot.json': { version: 1, length: snapshot.length, hashes: { sha256: digest(snapshot) } } } });
  const files = new Map([['/root.json', root], ['/timestamp.json', timestamp], ['/snapshot.json', snapshot], ['/targets.json', targets], ['/targets/trusted_root.json', artifact]]);
  const requested = [];
  const server = createServer((request, response) => {
    requested.push(request.url);
    const bytes = files.get(request.url);
    if (!bytes) { response.writeHead(404).end(); return; }
    response.setHeader('Content-Type', 'application/json');
    response.end(bytes);
  });
  await writeFile(join(directory, 'root.json'), root);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    requested,
    options: {
      tufMirrorURL: `http://127.0.0.1:${server.address().port}`,
      tufRootPath: join(directory, 'root.json'),
      tufCachePath: join(directory, 'cache'),
      certificateIssuer: 'https://github.com/login/oauth',
      certificateIdentityEmail: '^brian@dehamer\\.com$',
      certificateOIDs: { '1.3.6.1.4.1.57264.1.1': 'https://github.com/login/oauth' },
      retry: 0,
      timeout: 10000,
    },
    close: async () => {
      server.closeAllConnections();
      await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
      await rm(directory, { recursive: true, force: true });
    },
  };
}

test('pinned public Sigstore verifier verifies real certificate, DSSE, CT and Rekor against isolated signed TUF metadata', async () => {
  const fixture = JSON.parse(await readFile(new URL('./fixtures/sigstore-verification.json', import.meta.url), 'utf8'));
  const repository = await isolatedTrustRepository(fixture.trustedRoot);
  try {
    const result = await verify(fixture.bundle, repository.options);
    assert.ok(result.key);
    assert.ok(repository.requested.includes('/targets/trusted_root.json'));
    await assert.rejects(verify(fixture.bundle, { ...repository.options, certificateIssuer: 'https://untrusted.example' }), /invalid certificate extension/);
    await assert.rejects(verify(fixture.bundle, { ...repository.options, certificateIdentityEmail: '^someone-else@example\\.com$' }), /certificate identity error/);
    await assert.rejects(verify(fixture.bundle, { ...repository.options, certificateOIDs: { '1.3.6.1.4.1.57264.1.1': 'not-the-issuer' } }), /invalid certificate extension/);
    await assert.rejects(verify(fixture.bundle, { ...repository.options, certificateOIDs: { '1.3.6.1.4.1.57264.1.13': Buffer.concat([Buffer.from([0x0c, 40]), Buffer.from('a'.repeat(40))]) } }), /invalid certificate extension/);
    const tampered = structuredClone(fixture.bundle);
    tampered.dsseEnvelope.payload = Buffer.from('unsigned replacement').toString('base64');
    await assert.rejects(verify(tampered, repository.options));
    const unsigned = structuredClone(fixture.bundle);
    unsigned.dsseEnvelope.signatures = [];
    await assert.rejects(verify(unsigned, repository.options));
  } finally {
    await repository.close();
  }
});
