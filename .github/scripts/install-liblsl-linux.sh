#!/usr/bin/env bash
set -euo pipefail

LSL_VERSION="1.17.4"

if apt-cache show liblsl-dev >/dev/null 2>&1; then
  sudo apt-get update
  sudo apt-get install -y liblsl-dev
  exit 0
fi

codename="$(. /etc/os-release && echo "${VERSION_CODENAME}")"
case "${codename}" in
  noble | jammy) target="${codename}" ;;
  *)
    echo "Unsupported Ubuntu codename for liblsl install: ${codename}" >&2
    exit 1
    ;;
esac

deb="liblsl-${LSL_VERSION}-${target}_amd64.deb"
url="https://github.com/sccn/liblsl/releases/download/v${LSL_VERSION}/${deb}"

curl -fsSL "${url}" -o "/tmp/${deb}"
sudo apt-get update
sudo apt-get install -y "/tmp/${deb}"
