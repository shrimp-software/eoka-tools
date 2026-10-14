#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 0 ]]; then
  echo 'Usage: EOKA_CORE_PATH=/absolute/path/to/eoka bash scripts/with-local-core.sh COMMAND [ARGS...]' >&2
  exit 2
fi

core_root="${EOKA_CORE_PATH:?Set EOKA_CORE_PATH to the matching core checkout}"
core_root="$(cd "${core_root}" && pwd -P)"
if [[ ! -f "${core_root}/Cargo.toml" ]]; then
  echo "No Cargo.toml in core checkout: ${core_root}" >&2
  exit 2
fi

base_cargo_home="${CARGO_HOME:-${HOME}/.cargo}"
base_cargo_home="$(cd "${base_cargo_home}" && pwd -P)"
local_cargo_home="$(mktemp -d)"
cleanup() {
  rm -rf -- "${local_cargo_home}"
}
trap cleanup EXIT

for directory in registry git; do
  if [[ -d "${base_cargo_home}/${directory}" ]]; then
    ln -s "${base_cargo_home}/${directory}" "${local_cargo_home}/${directory}"
  fi
done
for file in config.toml credentials.toml; do
  if [[ -f "${base_cargo_home}/${file}" ]]; then
    cp "${base_cargo_home}/${file}" "${local_cargo_home}/${file}"
  fi
done

escaped_root="${core_root//\\/\\\\}"
escaped_root="${escaped_root//\"/\\\"}"
printf '\n[patch.crates-io.eoka]\npath = "%s"\n' "${escaped_root}" >> "${local_cargo_home}/config.toml"
export CARGO_HOME="${local_cargo_home}"
"$@"
