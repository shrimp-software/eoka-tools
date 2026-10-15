import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, sign, verify } from 'node:crypto';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { test } from 'node:test';
import { resumePublication, verifyExistingPackage } from '../scripts/publication-recovery.mjs';

const sdkName = '@cbxss/eoka';
const identity = { repository: 'shrimp-software/eoka-tools', workflowPath: '.github/workflows/publish-node.yml', sourceSha: 'a'.repeat(40), runId: '12345' };
const names = [sdkName, ...['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64'].map(id => `@cbxss/eoka-server-${id}`)];
const packages = names.map(name => {
  const bytes = Buffer.from(`qualified immutable tarball for ${name}`);
  return { name, version: '0.1.0-beta.1', filename: `${name.replace(/[@/]/g, '-')}.tgz`, bytes, integrity: `sha512-${createHash('sha512').update(bytes).digest('base64')}` };
});
const ordered = [...packages.slice(1).sort((a, b) => a.name.localeCompare(b.name)), packages[0]];
const { publicKey, privateKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
const provenanceType = 'https://slsa.dev/provenance/v1';
const payloadType = 'application/vnd.in-toto+json';
const certificate = 'isolated-test-certificate';
const workflowUri = `https://github.com/${identity.repository}/${identity.workflowPath}@refs/heads/main`;

function pae(payload) {
  return Buffer.concat([Buffer.from(`DSSEv1 ${Buffer.byteLength(payloadType)} ${payloadType} ${payload.length} `), payload]);
}

function statement(item) {
  return {
    _type: 'https://in-toto.io/Statement/v1',
    predicateType: provenanceType,
    subject: [{ name: `pkg:npm/${item.name.replace(/^@/, '%40')}@${item.version}`, digest: { sha512: Buffer.from(item.integrity.slice(7), 'base64').toString('hex') } }],
    predicate: {
      buildDefinition: {
        buildType: 'https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1',
        externalParameters: { workflow: { ref: 'refs/heads/main', repository: `https://github.com/${identity.repository}`, path: identity.workflowPath } },
        internalParameters: { github: { event_name: 'workflow_dispatch' } },
        resolvedDependencies: [{ uri: `git+https://github.com/${identity.repository}@refs/heads/main`, digest: { gitCommit: identity.sourceSha } }],
      },
      runDetails: { builder: { id: 'https://github.com/actions/runner/github-hosted' }, metadata: { invocationId: `https://github.com/${identity.repository}/actions/runs/${identity.runId}/attempts/1` } },
    },
  };
}

function provenanceBundle(item, change = value => value) {
  const value = statement(item);
  change(value);
  const payload = Buffer.from(JSON.stringify(value));
  return {
    dsseEnvelope: { payloadType, payload: payload.toString('base64'), signatures: [{ keyid: '', sig: sign('sha256', pae(payload), privateKey).toString('base64') }] },
    verificationMaterial: { certificate: { rawBytes: certificate } },
  };
}

async function isolatedRegistry() {
  const versions = new Map();
  const attestations = new Map();
  const next = new Map();
  const writes = [];
  const checks = [];
  const requests = [];
  let failure;
  let registry;
  const seed = (item, change) => {
    const metadata = {
      name: item.name,
      version: item.version,
      dist: {
        integrity: item.integrity,
        signatures: [{ keyid: 'fixture-registry-key', sig: sign('sha256', Buffer.from(`${item.name}@${item.version}:${item.integrity}`), privateKey).toString('base64') }],
        attestations: { url: `${registry}attestations/${encodeURIComponent(item.name)}`, provenance: { predicateType: provenanceType } },
      },
    };
    versions.set(item.name, metadata);
    attestations.set(item.name, { attestations: [{ predicateType: provenanceType, bundle: provenanceBundle(item, change) }] });
    next.set(item.name, item.version);
  };
  const server = createServer(async (request, response) => {
    requests.push(`${request.method} ${request.url}`);
    const path = request.url.slice(1);
    if (request.method === 'POST' && path.startsWith('publish/')) {
      const name = decodeURIComponent(path.slice('publish/'.length));
      const item = packages.find(item => item.name === name);
      if (failure?.name === name && failure.mode === 'before') {
        response.writeHead(503).end('isolated publication failure');
        return;
      }
      if (versions.has(name)) {
        response.writeHead(409).end('immutable existing version');
        return;
      }
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const bytes = Buffer.concat(chunks);
      if (!item || !bytes.equals(item.bytes)) {
        response.writeHead(400).end('not the exact qualified bytes');
        return;
      }
      seed(item);
      writes.push(name);
      if (failure?.name === name && failure.mode === 'after') {
        response.destroy();
        return;
      }
      response.writeHead(201).end('{}');
      return;
    }
    if (request.method !== 'GET') {
      response.writeHead(405).end();
      return;
    }
    let document;
    if (path.startsWith('attestations/')) document = attestations.get(decodeURIComponent(path.slice('attestations/'.length)));
    else {
      const name = decodeURIComponent(path);
      if (packages.some(item => item.name === name)) document = { name, versions: versions.has(name) ? { [versions.get(name).version]: versions.get(name) } : {}, 'dist-tags': next.has(name) ? { next: next.get(name) } : {} };
    }
    if (!document) {
      response.writeHead(404).end('{}');
      return;
    }
    response.setHeader('Content-Type', 'application/json');
    response.end(JSON.stringify(document));
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  registry = `http://127.0.0.1:${server.address().port}/`;
  const readJson = async url => {
    const response = await fetch(url);
    assert.equal(response.status, 200);
    return response.json();
  };
  const readPackage = name => readJson(`${registry}${encodeURIComponent(name)}`);
  const options = { registry, tufCache: 'isolated-trust-boundary' };
  const verifySignatures = async item => {
    const metadata = (await readPackage(item.name)).versions[item.version];
    assert.ok(metadata.dist.signatures?.length > 0, 'Missing registry signature');
    for (const signature of metadata.dist.signatures) {
      assert.equal(signature.keyid, 'fixture-registry-key');
      assert.ok(verify('sha256', Buffer.from(`${item.name}@${item.version}:${metadata.dist.integrity}`), publicKey, Buffer.from(signature.sig, 'base64')), 'Invalid registry signature');
    }
  };
  const sigstore = {
    verify: async (bundle, policy) => {
      assert.equal(bundle.verificationMaterial?.certificate?.rawBytes, certificate, 'Untrusted isolated certificate');
      const payload = Buffer.from(bundle.dsseEnvelope.payload, 'base64');
      assert.ok(verify('sha256', pae(payload), publicKey, Buffer.from(bundle.dsseEnvelope.signatures[0].sig, 'base64')), 'Invalid provenance signature');
      assert.equal(policy.certificateIssuer, 'https://token.actions.githubusercontent.com');
      assert.ok(new RegExp(policy.certificateIdentityURI).test(workflowUri));
      assert.ok(!new RegExp(policy.certificateIdentityURI).test(`${workflowUri}-untrusted`));
      const values = {
        '1.3.6.1.4.1.57264.1.12': `https://github.com/${identity.repository}`,
        '1.3.6.1.4.1.57264.1.13': identity.sourceSha,
        '1.3.6.1.4.1.57264.1.14': 'refs/heads/main',
        '1.3.6.1.4.1.57264.1.20': 'workflow_dispatch',
      };
      for (const [oid, value] of Object.entries(values)) assert.deepEqual(policy.certificateOIDs[oid], Buffer.concat([Buffer.from([0x0c, Buffer.byteLength(value)]), Buffer.from(value)]));
      checks.push(JSON.parse(payload).subject[0].name);
    },
  };
  const boundary = {
    readPackage,
    verifyExisting: (item, metadata) => verifyExistingPackage(item, metadata, { verifySignatures, sigstore, options, readJson, identity }),
    publish: async item => {
      const response = await fetch(`${registry}publish/${encodeURIComponent(item.name)}`, { method: 'POST', body: item.bytes });
      assert.equal(response.status, 201, 'Publication failed');
    },
  };
  return {
    versions, attestations, next, writes, checks, requests, seed,
    fail: (name, mode) => { failure = name ? { name, mode } : undefined; },
    resume: beforePublish => resumePublication({ packages, sdkName, boundary, beforePublish: beforePublish ?? (async () => {}) }),
    close: async () => { server.closeAllConnections(); await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); },
  };
}

async function withRegistry(run) {
  const registry = await isolatedRegistry();
  try { await run(registry); } finally { await registry.close(); }
}

test('partial six-package publication resumes exact artifacts without reuploading completed packages', async () => {
  await withRegistry(async registry => {
    registry.fail(ordered[2].name, 'before');
    await assert.rejects(registry.resume(), /Publication failed/);
    assert.deepEqual(registry.writes, ordered.slice(0, 2).map(item => item.name));
    registry.fail();
    const result = await registry.resume();
    assert.deepEqual(result.skipped, ordered.slice(0, 2).map(item => item.name));
    assert.deepEqual(result.published, ordered.slice(2).map(item => item.name));
    assert.deepEqual(registry.writes, ordered.map(item => item.name));
    assert.equal(registry.checks.length, 2);
  });
});

test('lost publish response is fatal immediately and recoverable on the next invocation', async () => {
  await withRegistry(async registry => {
    registry.fail(ordered[2].name, 'after');
    await assert.rejects(registry.resume());
    assert.deepEqual(registry.writes, ordered.slice(0, 3).map(item => item.name));
    assert.equal(registry.requests.filter(request => request.startsWith('POST ')).length, 3);
    registry.fail();
    const result = await registry.resume();
    assert.deepEqual(result.skipped, ordered.slice(0, 3).map(item => item.name));
    assert.deepEqual(registry.writes, ordered.map(item => item.name));
  });
});

test('all existing verified packages are a no-op without source-write gates or retagging', async () => {
  await withRegistry(async registry => {
    for (const item of packages) registry.seed(item);
    assert.deepEqual(await registry.resume(async () => { assert.fail('No write gate should run'); }), { skipped: ordered.map(item => item.name), published: [] });
    assert.equal(registry.checks.length, 6);
    assert.deepEqual(registry.writes, []);
    assert.equal(registry.requests.some(request => request.startsWith('POST ')), false);
  });
});

for (const [name, alter, message] of [
  ['integrity mismatch', registry => { registry.versions.get(sdkName).dist.integrity = packages[1].integrity; }, /Existing tarball differs/],
  ['missing provenance', registry => { delete registry.versions.get(sdkName).dist.attestations; }, /Missing npm provenance/],
  ['missing registry signature', registry => { delete registry.versions.get(sdkName).dist.signatures; }, /Missing registry signature/],
  ['invalid registry signature', registry => { registry.versions.get(sdkName).dist.signatures[0].sig = 'AA=='; }, /Invalid registry signature/],
  ['invalid provenance signature', registry => { registry.attestations.get(sdkName).attestations[0].bundle.dsseEnvelope.signatures[0].sig = 'AA=='; }, /Invalid provenance signature/],
  ['unsigned provenance', registry => { registry.attestations.get(sdkName).attestations[0].bundle.dsseEnvelope.signatures = []; }, undefined],
  ['no provenance bundle', registry => { registry.attestations.get(sdkName).attestations = []; }, /Exactly one verified/],
  ['untrusted certificate', registry => { registry.attestations.get(sdkName).attestations[0].bundle.verificationMaterial.certificate.rawBytes = 'untrusted'; }, /Untrusted isolated certificate/],
  ['next tag mismatch', registry => { registry.next.set(sdkName, '0.1.0-beta.2'); }, /refusing silent retag/],
]) {
  test(`${name} in the final package aborts before any missing native package is published`, async () => {
    await withRegistry(async registry => {
      registry.seed(packages[0]);
      alter(registry);
      await assert.rejects(registry.resume(), message);
      assert.deepEqual(registry.writes, []);
      assert.equal(registry.requests.some(request => request.startsWith('POST ')), false);
    });
  });
}

for (const [name, change] of [
  ['different source commit', value => { value.predicate.buildDefinition.resolvedDependencies[0].digest.gitCommit = 'b'.repeat(40); }],
  ['different source repository', value => { value.predicate.buildDefinition.externalParameters.workflow.repository = 'https://github.com/untrusted/project'; }],
  ['different workflow', value => { value.predicate.buildDefinition.externalParameters.workflow.path = '.github/workflows/untrusted.yml'; }],
  ['different event', value => { value.predicate.buildDefinition.internalParameters.github.event_name = 'push'; }],
  ['different run', value => { value.predicate.runDetails.metadata.invocationId = `https://github.com/${identity.repository}/actions/runs/999/attempts/1`; }],
  ['different artifact', value => { value.subject[0].digest.sha512 = '0'.repeat(128); }],
]) {
  test(`cryptographically signed provenance for ${name} aborts before writes`, async () => {
    await withRegistry(async registry => {
      registry.seed(packages[0], change);
      await assert.rejects(registry.resume());
      assert.deepEqual(registry.writes, []);
      assert.equal(registry.checks.length, 1);
    });
  });
}

test('native packages always precede SDK and each missing write rechecks the source gate', async () => {
  await withRegistry(async registry => {
    const gates = [];
    const result = await registry.resume(async item => {
      assert.deepEqual(registry.writes, gates);
      gates.push(item.name);
    });
    assert.deepEqual(gates, ordered.map(item => item.name));
    assert.deepEqual(result.published, gates);
    assert.equal(registry.writes.at(-1), sdkName);
  });
});

test('source gate failure preserves packages already uploaded and never starts the SDK', async () => {
  await withRegistry(async registry => {
    await assert.rejects(registry.resume(async item => {
      if (item.name === ordered[3].name) throw new Error('Protected main changed');
    }), /Protected main changed/);
    assert.deepEqual(registry.writes, ordered.slice(0, 3).map(item => item.name));
    assert.equal(registry.versions.has(sdkName), false);
  });
});

test('ambiguous registry reads and missing scoped-package bootstrap are fail closed', async () => {
  for (const failure of [new Error('Network unavailable'), { name: ordered[5].name, 'dist-tags': { next: packages[0].version } }]) {
    const writes = [];
    await assert.rejects(resumePublication({
      packages, sdkName, beforePublish: async () => {},
      boundary: {
        readPackage: async name => {
          if (failure instanceof Error) throw failure;
          return { ...failure, name };
        },
        verifyExisting: async () => { assert.fail('No version is present'); },
        publish: async item => { writes.push(item.name); },
      },
    }));
    assert.deepEqual(writes, []);
  }
});
