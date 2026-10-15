import assert from 'node:assert/strict';

const predicateType = 'https://slsa.dev/provenance/v1';
const statementType = 'https://in-toto.io/Statement/v1';
const buildType = 'https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1';

function utf8Extension(value) {
  const bytes = Buffer.from(value);
  assert.ok(bytes.length < 128, 'Certificate identity extension is unexpectedly long');
  return Buffer.concat([Buffer.from([0x0c, bytes.length]), bytes]);
}

export async function verifyExistingPackage(item, metadata, { verifySignatures, sigstore, options, readJson, identity }) {
  assert.equal(metadata.name, item.name);
  assert.equal(metadata.version, item.version);
  assert.equal(metadata.dist?.integrity, item.integrity);
  assert.ok(metadata.dist.signatures?.length > 0, `Missing registry signature: ${item.name}`);
  assert.equal(metadata.dist.attestations?.provenance?.predicateType, predicateType);
  await verifySignatures(item, metadata);
  const url = new URL(metadata.dist.attestations.url);
  assert.equal(url.origin, new URL(options.registry).origin, 'Attestations must come from the configured registry');
  const document = await readJson(url.href);
  assert.ok(Array.isArray(document.attestations), 'Invalid registry attestation response');
  const provenance = document.attestations.filter(item => item.predicateType === predicateType);
  assert.equal(provenance.length, 1, 'Exactly one verified npm provenance attestation is required');
  const { bundle } = provenance[0];
  assert.equal(bundle?.dsseEnvelope?.payloadType, 'application/vnd.in-toto+json');
  assert.ok(bundle.verificationMaterial?.certificate || bundle.verificationMaterial?.x509CertificateChain, 'Keyless certificate provenance is required');
  const workflowUri = `https://github.com/${identity.repository}/${identity.workflowPath}@refs/heads/main`;
  await sigstore.verify(bundle, {
    tufCachePath: options.tufCache,
    retry: 0,
    timeout: 120000,
    certificateIssuer: 'https://token.actions.githubusercontent.com',
    certificateIdentityURI: `^${workflowUri.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`,
    certificateOIDs: {
      '1.3.6.1.4.1.57264.1.12': utf8Extension(`https://github.com/${identity.repository}`),
      '1.3.6.1.4.1.57264.1.13': utf8Extension(identity.sourceSha),
      '1.3.6.1.4.1.57264.1.14': utf8Extension('refs/heads/main'),
      '1.3.6.1.4.1.57264.1.20': utf8Extension('workflow_dispatch'),
    },
  });
  const statement = JSON.parse(Buffer.from(bundle.dsseEnvelope.payload, 'base64').toString('utf8'));
  assert.equal(statement._type, statementType);
  assert.equal(statement.predicateType, predicateType);
  assert.deepEqual(statement.subject, [{
    name: `pkg:npm/${item.name.replace(/^@/, '%40')}@${item.version}`,
    digest: { sha512: Buffer.from(item.integrity.slice('sha512-'.length), 'base64').toString('hex') },
  }]);
  const definition = statement.predicate?.buildDefinition;
  assert.equal(definition?.buildType, buildType);
  assert.deepEqual(definition.externalParameters?.workflow, {
    ref: 'refs/heads/main',
    repository: `https://github.com/${identity.repository}`,
    path: identity.workflowPath,
  });
  assert.equal(definition.internalParameters?.github?.event_name, 'workflow_dispatch');
  assert.deepEqual(definition.resolvedDependencies, [{
    uri: `git+https://github.com/${identity.repository}@refs/heads/main`,
    digest: { gitCommit: identity.sourceSha },
  }]);
  assert.equal(statement.predicate.runDetails?.builder?.id, 'https://github.com/actions/runner/github-hosted');
  const invocation = statement.predicate.runDetails?.metadata?.invocationId;
  assert.match(invocation ?? '', new RegExp(`^https://github\\.com/${identity.repository}/actions/runs/${identity.runId}/attempts/[1-9][0-9]*$`));
}

export async function resumePublication({ packages, sdkName, boundary, beforePublish, allowMissingPackages = false }) {
  assert.ok(Array.isArray(packages) && packages.length === 6, 'All six qualified packages are required');
  assert.equal(new Set(packages.map(item => item.name)).size, packages.length);
  const sdk = packages.find(item => item.name === sdkName);
  assert.ok(sdk, 'Qualified SDK package is required');
  const ordered = [...packages.filter(item => item !== sdk).sort((a, b) => a.name.localeCompare(b.name)), sdk];
  const missing = [];
  const skipped = [];
  for (const item of ordered) {
    assert.match(item.integrity, /^sha512-[A-Za-z0-9+/]{86}==$/);
    const packument = await boundary.readPackage(item.name);
    if (packument === null) {
      assert.equal(allowMissingPackages, true, `Registry package is missing; explicit bootstrap is required: ${item.name}`);
      missing.push(item);
      continue;
    }
    assert.equal(packument.name, item.name);
    assert.ok(packument.versions && typeof packument.versions === 'object' && !Array.isArray(packument.versions), `Invalid registry versions: ${item.name}`);
    assert.ok(packument['dist-tags'] && typeof packument['dist-tags'] === 'object', `Invalid registry tags: ${item.name}`);
    if (!Object.hasOwn(packument.versions, item.version)) {
      assert.notEqual(packument['dist-tags'].next, item.version, `Registry tag points to a missing version: ${item.name}`);
      missing.push(item);
      continue;
    }
    const metadata = packument.versions[item.version];
    assert.equal(metadata?.name, item.name);
    assert.equal(metadata?.version, item.version);
    assert.equal(metadata?.dist?.integrity, item.integrity, `Existing tarball differs from qualification: ${item.name}`);
    assert.equal(packument['dist-tags'].next, item.version, `Existing version must already be tagged next; refusing silent retag: ${item.name}`);
    assert.equal(metadata.dist.attestations?.provenance?.predicateType, predicateType, `Missing npm provenance: ${item.name}`);
    assert.ok(metadata.dist.attestations.url, `Missing npm attestation URL: ${item.name}`);
    await boundary.verifyExisting(item, metadata);
    skipped.push(item.name);
  }
  const published = [];
  for (const item of missing) {
    await beforePublish(item);
    await boundary.publish(item);
    published.push(item.name);
  }
  return { skipped, published };
}
