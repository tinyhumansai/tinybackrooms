#!/usr/bin/env bash
# The engine crate reaches models, memory and embeddings only through ports, so
# it must not link an async runtime, an HTTP client, a server, a database, or the
# OpenHuman harness. That promise is invisible in a diff, because a forbidden
# dependency arrives transitively through a feature enabled one crate away, so
# it is asserted rather than documented.
#
# The FORWARD form of `cargo tree` is required: `cargo tree -i <crate> -p ...`
# discards the `-p` scope, prints the whole-workspace inverse tree, and exits 0
# looking clean even when this crate is the one at fault.
set -euo pipefail

forbidden='^(tokio|axum|hyper|reqwest|ureq|rusqlite|git2|async-std|smol|openhuman[a-z-]*|tinybus[a-z-]*) '

found="$(cargo tree -p tinybackrooms -e normal,build --prefix none --all-features \
  | sort -u | grep -E "$forbidden" || true)"

if [ -n "$found" ]; then
  echo "tinybackrooms pulled in a dependency the engine must not link:" >&2
  echo "$found" >&2
  echo >&2
  echo "The engine owns the pipeline and nothing else. Models, memory and" >&2
  echo "embeddings arrive through its ports; transports and runtimes belong in" >&2
  echo "an adapter or the API crate." >&2
  exit 1
fi
echo "tinybackrooms links no runtime, transport, database or harness"
