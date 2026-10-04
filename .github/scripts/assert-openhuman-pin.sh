#!/bin/sh
# Assert the `rev` of each git dependency on OpenHuman in Cargo.toml equals the
# commit recorded for the submodule its `[patch]` redirects onto. The `[patch]`
# builds this workspace against the submodule; a consumer without that patch
# builds against the rev. If they drift, the two build different OpenHuman trees
# and nothing else would say so.
set -eu
cd "$(dirname "$0")/../.."

pin() {
  sed -n "s/^$1 = { git = \"https:\/\/github.com\/tinyhumansai\/$2\", rev = \"\([0-9a-f]*\)\".*/\1/p" Cargo.toml
}

check() {
  crate="$1" rev="$2" tree="$3" sub="$4"
  if [ -z "$rev" ] || [ -z "$sub" ]; then
    echo "assert-openhuman-pin: could not read the rev for $crate ($rev) or the submodule pointer for $tree ($sub)" >&2
    exit 1
  fi
  if [ "$rev" != "$sub" ]; then
    echo "assert-openhuman-pin: Cargo.toml pins $crate at $rev but $tree records $sub" >&2
    exit 1
  fi
  echo "assert-openhuman-pin: $crate $rev"
}

# `openhuman-embed` and `openhuman` come from the OpenHuman submodule.
openhuman_sub=$(git ls-files -s vendor/openhuman | awk '{print $2}')
check openhuman-embed "$(pin openhuman-embed openhuman)" vendor/openhuman "$openhuman_sub"
check openhuman "$(pin openhuman openhuman)" vendor/openhuman "$openhuman_sub"

# `tinytools` comes from the copy nested inside OpenHuman's tinyagents, at
# whatever commit OpenHuman's own tree vendors, since that is the copy the patch
# resolves it onto.
tinytools_sub=$(git -C vendor/openhuman/vendor/tinyagents ls-tree HEAD vendor/tinytools | awk '{print $3}')
check tinytools "$(pin tinytools tinytools)" vendor/openhuman/vendor/tinyagents/vendor/tinytools "$tinytools_sub"
