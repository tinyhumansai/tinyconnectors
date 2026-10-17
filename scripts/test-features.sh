#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
python3 scripts/test-e2e-version.py
python3 scripts/test-script-contracts.py
cargo test --locked --workspace
cargo test --locked --workspace --all-features

cargo run -p tinyconnectors --example basic

set -euo pipefail
forbidden="$(cargo tree -p tinyconnectors-bus -e normal,build --prefix none \
  | grep -Ei 'tinybus|tokio|reqwest|ureq|hyper|rusqlite|git2' || true)"
if [ -n "$forbidden" ]; then
  echo "tinyconnectors-bus pulled in a dependency its manifest forbids:" >&2
  echo "$forbidden" >&2
  echo >&2
  echo "The contract is what a host compiles against. It must stay free" >&2
  echo "of transports, async runtimes, HTTP clients and native libraries." >&2
  exit 1
fi
bash scripts/test-storage-needs-consumer.sh
case "${RUNNER_OS:-$(uname -s)}" in
  Windows|MINGW*|MSYS*|CYGWIN*) ;;
  *) python3 scripts/test-package-layout.py ;;
esac
