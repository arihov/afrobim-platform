#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
# Copyright (c) 2026 AfroBIM. All rights reserved.
#
# Regenerates THIRD-PARTY-NOTICES.md (every Rust dependency, its SPDX licence and licence text,
# grouped by licence) from afrobim/Cargo.lock. Run it before every release and commit the result.
#
# Usage: scripts/gen-notices.sh [--check]
#   --check  fail if THIRD-PARTY-NOTICES.md is out of date (for CI)
#
# Uses `cargo metadata` and a small script, so cargo-about is not needed. It reads the crates'
# own licence files from the cargo registry, so the sources must be fetched (done here).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$root/afrobim/Cargo.toml"
cargo fetch --locked --manifest-path "$manifest" >/dev/null
python3 -I "$root/scripts/gen_notices.py" "$manifest" "$root/THIRD-PARTY-NOTICES.md" "$@"
