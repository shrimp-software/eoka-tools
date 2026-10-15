import { chmod, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const tokenVariable = /^(?:NODE_AUTH_TOKEN|NPM_TOKEN|NPM_AUTH_TOKEN|NPM_BOOTSTRAP_TOKEN)$/i;
const publicationOverride = /^(?:EOKA_(?:CORE_(?:REF|PATH)|SERVER_BIN|CHROME_PATH)|CARGO_PATCH_|CARGO_SOURCE_|CARGO_BUILD_RUSTFLAGS$|RUSTFLAGS$)/;

export function validatePublicationAuth(environment, command) {
  const setting = environment.NPM_BOOTSTRAP;
  if (setting !== undefined && setting !== '' && setting !== 'false' && setting !== 'true') throw new Error('NPM_BOOTSTRAP must be true or false');
  const bootstrap = setting === 'true';
  for (const [name, value] of Object.entries(environment)) {
    if (value && (publicationOverride.test(name) || /^npm_config_.*auth/i.test(name))) throw new Error(`Publication override or token is forbidden: ${name}`);
    if (value !== undefined && tokenVariable.test(name) && (name !== 'NODE_AUTH_TOKEN' || !bootstrap || command !== 'publish' || environment.GITHUB_JOB !== 'publish')) throw new Error(`Publication token is forbidden: ${name}`);
  }
  if (bootstrap && command === 'publish') {
    if (environment.GITHUB_JOB !== 'publish') throw new Error('GITHUB_JOB must be publish for NPM_BOOTSTRAP publication');
    if (typeof environment.NODE_AUTH_TOKEN !== 'string' || !environment.NODE_AUTH_TOKEN.trim()) throw new Error('NODE_AUTH_TOKEN is required for NPM_BOOTSTRAP publication');
  }
  return bootstrap;
}

export function validatePublicationReview(reviewers, bootstrap = false) {
  if (typeof bootstrap !== 'boolean') throw new Error('NPM_BOOTSTRAP review opt-in must be boolean');
  if (reviewers?.type !== 'required_reviewers' || !Array.isArray(reviewers.reviewers) || reviewers.reviewers.length === 0) throw new Error('npm-prerelease must require reviewer approval');
  if (typeof reviewers.prevent_self_review !== 'boolean') throw new Error('npm-prerelease prevent_self_review must be boolean');
  if (!bootstrap && !reviewers.prevent_self_review) throw new Error('npm-prerelease must prevent self-review');
}

export async function npmEnvironment(directory, { environment = process.env, authenticated = false } = {}) {
  if (authenticated === true && !validatePublicationAuth(environment, 'publish')) throw new Error('NPM_BOOTSTRAP must be true for authenticated npm publication');
  const userconfig = join(directory, 'npmrc');
  const globalconfig = join(directory, 'global-npmrc');
  await writeFile(userconfig, authenticated === true ? '//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}\n' : '', { mode: 0o600 });
  await chmod(userconfig, 0o600);
  await writeFile(globalconfig, '', { mode: 0o600 });
  await chmod(globalconfig, 0o600);
  const env = { ...environment };
  for (const key of Object.keys(env)) if (/^npm_config_/i.test(key) || tokenVariable.test(key)) delete env[key];
  if (authenticated === true) env.NODE_AUTH_TOKEN = environment.NODE_AUTH_TOKEN;
  return { ...env, NPM_CONFIG_USERCONFIG: userconfig, NPM_CONFIG_GLOBALCONFIG: globalconfig, NPM_CONFIG_REGISTRY: 'https://registry.npmjs.org/', NPM_CONFIG_CACHE: join(directory, 'npm-cache'), NPM_CONFIG_FETCH_RETRIES: '0' };
}
