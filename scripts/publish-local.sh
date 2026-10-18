#!/usr/bin/env bash
# Local equivalent of .github/workflows/publish.yml - builds and pushes the
# same container images through the same nx `container` targets, for when
# the GitHub Actions minutes are exhausted. Must run from a real Linux (or
# WSL2) shell for the 3 Rust apps (edge_shield, tunnel-server, tunnel-client)
# - see rust.Dockerfile: their images copy an already-built binary rather
# than compiling it in-container, and `nx build`/`build-arm64` only produce
# a Linux ELF from a Linux host. On Windows, this script relaunches itself
# inside WSL2 automatically (see below) - just run `npm run publish:local`
# from PowerShell/cmd/Git Bash, nothing else needed.
#
# Usage:
#   npm run publish:local                 # only nx-affected images (vs. origin/main)
#   npm run publish:local -- --all        # every image, like a release build
#   npm run publish:local -- backend frontend  # only the named projects
#
# Requires: `docker login ghcr.io` beforehand (PAT with write:packages), and
# `docker login` (Docker Hub) too if tunnel-client is in scope, since that
# one also pushes to docker.io/eversit/eversitd. Run the login once, inside
# WSL2 if you're on Windows - that's the Docker CLI this script actually
# uses there.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Git Bash/MSYS/Cygwin can run this script but can't build the Rust apps'
# Linux ELF binaries - relaunch under WSL2 instead of just warning, so a
# plain `npm run publish:local` on Windows does the right thing by itself.
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    if ! command -v wsl.exe >/dev/null 2>&1; then
      echo "This is a Windows shell (not WSL2) and wsl.exe wasn't found -" >&2
      echo "install WSL2 (https://aka.ms/wsl) or run this script from inside" >&2
      echo "a WSL2 shell directly." >&2
      exit 1
    fi
    echo "Windows shell detected - relaunching inside WSL2..."
    # wsl.exe auto-translates the caller's current directory (repo root, set
    # above) into its /mnt/<drive>/... equivalent, so the relative path
    # below resolves the same script inside WSL2.
    exec wsl.exe bash scripts/publish-local.sh "$@"
    ;;
esac

# --- resolve EVERSIT_* image refs, mirroring publish.yml's "Resolve image refs" step ---
remote_url=$(git remote get-url origin)
owner=$(echo "$remote_url" | sed -E 's#.*[:/]([^/]+)/[^/]+\.git#\1#' | tr '[:upper:]' '[:lower:]')
base="ghcr.io/${owner}"

export EVERSIT_BACKEND="${base}/backend"
export EVERSIT_FRONTEND="${base}/frontend"
export EVERSIT_DOCS="${base}/docs"
export EVERSIT_EMAIL_SERVICE="${base}/email-service"
export EVERSIT_DNS_SERVICE="${base}/dns-service"
export EVERSIT_DNS_VERIFICATION_POLLER="${base}/dns-verification-poller"
export EVERSIT_DOMAINS_SERVICE="${base}/domains-service"
export EVERSIT_BILLING_SERVICE="${base}/billing-service"
export EVERSIT_BANKING_POLLER_SERVICE="${base}/banking-poller-service"
export EVERSIT_CRON_DOMAIN_PRICE_SYNC="${base}/cron-domain-price-sync"
export EVERSIT_CRON_DOMAIN_STATUS_SYNC="${base}/cron-domain-status-sync"
export EVERSIT_CRON_DOMAIN_RENEWAL="${base}/cron-domain-renewal"
export EVERSIT_TUNNELS_SERVICE="${base}/tunnels-service"
export EVERSIT_SUPPORT_SERVICE="${base}/support-service"
export EVERSIT_TUNNEL_SERVER="${base}/tunnel-server"
export EVERSIT_TUNNEL_CLIENT="${base}/tunnel-client"
export EVERSIT_EDGE_SHIELD="${base}/edge-shield"

echo "Registry namespace: ${base}"

# --- sanity checks ---
if ! docker system info >/dev/null 2>&1; then
  echo "Docker daemon not reachable - start Docker (Desktop/WSL) first." >&2
  exit 1
fi

if ! grep -q "ghcr.io" ~/.docker/config.json 2>/dev/null; then
  echo "Warning: no ghcr.io login found in ~/.docker/config.json - run" >&2
  echo "  docker login ghcr.io -u <github-user>   (PAT scope: write:packages)" >&2
  echo "first, otherwise the push will fail with 'unauthorized'." >&2
fi

RUST_APPS=(edge_shield tunnel-server tunnel-client)
if [ "$(uname -s)" != "Linux" ]; then
  echo "Warning: not running on Linux - ${RUST_APPS[*]} will fail to build" >&2
  echo "(rust.Dockerfile needs a Linux ELF binary, which only a Linux host" >&2
  echo "can produce). On Windows this script already relaunches itself" >&2
  echo "under WSL2 for you - if you're seeing this warning, you're on" >&2
  echo "macOS or a similar non-Linux Unix, which has no WSL2 equivalent." >&2
fi

# --- decide scope ---
if [ "${1:-}" = "--all" ]; then
  echo "Building & pushing ALL images..."
  npx nx run-many -t container --parallel=3 -- --push=true
elif [ "$#" -gt 0 ]; then
  echo "Building & pushing: $*"
  npx nx run-many -t container --projects="$(IFS=,; echo "$*")" --parallel=3 -- --push=true
else
  echo "Building & pushing nx-affected images (vs. origin/main)..."
  git fetch origin main:refs/remotes/origin/main >/dev/null 2>&1 || true
  npx nx affected -t container --base=origin/main --parallel=3 -- --push=true
fi
