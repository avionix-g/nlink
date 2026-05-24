#!/usr/bin/env bash
# Audit: every `.rs` file under crates/nlink/examples/ MUST be
# registered as a [[example]] entry in crates/nlink/Cargo.toml.
#
# Why this exists: Cargo only auto-discovers examples at the
# top level of `examples/` (one level deep). Files in
# subdirectories like `examples/route/mpls.rs`, `examples/bridge/
# vlan.rs`, etc. are invisible to cargo unless explicitly declared
# via [[example]] name=… path=…  in Cargo.toml. Without that
# registration, `cargo build --workspace --all-targets` never
# compiles them — they bit-rot silently against API changes for
# years. This script forces every example to be visible.
#
# Per-example fix when the script flags a file:
#   - If the example is a current, working tutorial → add an
#     [[example]] block to crates/nlink/Cargo.toml.
#   - If the example is stale (references removed API) → either
#     update it to current API + register, or delete it.
#
# History: surfaced by the 0.16 audit (plans/160-example-registry-
# audit.md). 9 orphan files were found at the time the script was
# introduced; resolving them is gated separately so this script
# stayed dormant until the catalog is processed.

set -euo pipefail

CARGO="crates/nlink/Cargo.toml"
EXAMPLES_DIR="crates/nlink/examples"

if [[ ! -f "$CARGO" ]]; then
    echo "ERROR: $CARGO not found — run from repo root." >&2
    exit 2
fi

missing=0
while IFS= read -r f; do
    # Path relative to crates/nlink/ since that's what
    # `path = "examples/..."` references in Cargo.toml.
    rel="${f#crates/nlink/}"
    if ! grep -qF "path = \"$rel\"" "$CARGO"; then
        echo "::error file=$f::example $rel not registered in $CARGO"
        echo "  add this block to $CARGO:"
        # Derive a reasonable [[example]] name from the path:
        # examples/route/mpls.rs → route_mpls
        name=$(echo "$rel" | sed 's|^examples/||; s|\.rs$||; s|/|_|g')
        echo "    [[example]]"
        echo "    name = \"$name\""
        echo "    path = \"$rel\""
        echo
        missing=$((missing+1))
    fi
done < <(find "$EXAMPLES_DIR" -name '*.rs' -type f | sort)

if [[ $missing -eq 0 ]]; then
    echo "OK: every $EXAMPLES_DIR/*.rs is registered in $CARGO"
    exit 0
fi

echo
echo "Found $missing unregistered example file(s) — see plans/160-example-registry-audit.md"
echo "for per-file resolution guidance, or fix as instructed above."
exit 1
