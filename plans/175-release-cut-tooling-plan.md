---
to: nlink maintainers
from: 0.16 cut activation (2026-05-25)
subject: `scripts/cut-release.sh` + dry-run-inversion docs + GitHub release length workaround
status: proposed for 0.17 — applies to the 0.17 cut
target version: 0.17.0
parent: 177-0.17-master-plan.md
source: Plan 167 execution friction points
created: 2026-05-25
---

# Plan 175 — release-cut tooling

## 1. Why this plan exists

Plan 167 documented the 0.16 cut sequence as a runbook. Cutting
0.16 worked, but three friction points surfaced:

1. **`cargo publish --dry-run` for `nlink`** fails because the
   corresponding `nlink-macros` version isn't on crates.io yet.
   The publish ordering is "macros first, then nlink", but
   `dry-run` checks against the live registry. No end-to-end
   pre-flight validation.
2. **CHANGELOG `[Unreleased]` → `[X.Y.Z]` promotion** is
   manual. Cut-time edit, prone to forgetting.
3. **GitHub release body** has a 125000-character limit. The
   nlink CHANGELOG is bigger than that. The first attempt
   failed with `body is too long`; we had to truncate to a
   shorter highlight body.

A `scripts/cut-release.sh` wrapping the Plan 167 sequence makes
these issues concrete (script catches them) and reduces the
cognitive load of the cut.

## 2. The script's shape

```bash
#!/usr/bin/env bash
# scripts/cut-release.sh — orchestrate an nlink release cut.
#
# Walks the Plan 167 sequence end-to-end with confirmations at
# irreversible steps. Run from a clean working tree on the
# X.Y.Z release branch.
#
# Usage:
#   ./scripts/cut-release.sh 0.17.0

set -euo pipefail

VERSION="${1:?usage: $0 <X.Y.Z>}"

# --- Phase 1: pre-flight ---
check_clean_tree
check_on_release_branch "$VERSION"
check_cargo_login

# --- Phase 2: CHANGELOG promotion ---
promote_changelog "$VERSION"
confirm "CHANGELOG promoted; review the diff above"

# --- Phase 3: CI green-gate ---
push_branch
wait_for_ci_green

# --- Phase 4: dry-runs (with the inversion workaround) ---
cargo publish -p nlink-macros --dry-run
# Skip nlink dry-run — it fails because macros version isn't
# yet on crates.io; document this explicitly.
echo "NOTE: skipping 'cargo publish -p nlink --dry-run' — known"
echo "      false negative because nlink-macros $VERSION isn't"
echo "      on crates.io yet. Real publish below handles ordering."
confirm "dry-run clean — ready to commit + merge"

# --- Phase 5: commit + merge ---
git add CHANGELOG.md
git commit -m "chore(release): promote [Unreleased] → [$VERSION]"
git push
gh pr ready "$PR_NUMBER" || true
gh pr merge "$PR_NUMBER" --merge --subject "$VERSION cut"
git checkout master && git pull

# --- Phase 6: tag ---
tag_release "$VERSION"
confirm "tag created locally; ready to publish (THIS IS IRREVERSIBLE)"

# --- Phase 7: publish ---
cargo publish -p nlink-macros
echo "Waiting 30s for crates.io index propagation..."
sleep 30
cargo publish -p nlink

# --- Phase 8: tag push + GitHub release ---
git push origin "v$VERSION"
create_github_release "$VERSION"   # with length-aware body builder

# --- Phase 9: post-cut housekeeping ---
open_next_branch "$VERSION"
echo "Cut complete. Next: $NEXT_VERSION branch open at origin/$NEXT."
```

Each step has confirmations at the irreversible points.

## 3. The three workarounds the script bakes in

### 3.1 `cargo publish --dry-run` inversion

The script SKIPS the `nlink` dry-run by design, with a comment
explaining why. Optionally: write a local fake-registry
workaround (`cargo-publish-test` or similar) for a truly
end-to-end dry-run. **Out of scope for 175** — the skip-with-
explanation is adequate.

### 3.2 CHANGELOG promotion

`promote_changelog "$VERSION"` is a 5-line sed:

```bash
promote_changelog() {
    local version=$1
    local date=$(date +%Y-%m-%d)
    sed -i "s/^## \[Unreleased\]$/## [Unreleased]\n\n## [$version] - $date/" CHANGELOG.md
}
```

### 3.3 GitHub release length

`create_github_release "$VERSION"` builds a body that:
1. Pulls the version's CHANGELOG section between `## [X.Y.Z]`
   and the next `## [...]` (sed/awk).
2. Checks the length. If > 125000 chars, truncates to a
   "highlights" template + a link to the full CHANGELOG on
   GitHub.

```bash
create_github_release() {
    local version=$1
    local body=$(extract_changelog_section "$version")
    local max_len=125000

    if [[ ${#body} -gt $max_len ]]; then
        body=$(build_highlights_body "$version")
    fi

    gh release create "v$version" \
        --title "nlink $version" \
        --notes "$body" \
        --verify-tag
}
```

## 4. Acceptance criteria

- [ ] `scripts/cut-release.sh` exists + executable.
- [ ] The 0.17 cut runs through it end-to-end.
- [ ] Three friction points (§3) explicitly handled in the
      script with comments documenting why.
- [ ] CLAUDE.md "Publishing" section points at the script.
- [ ] CHANGELOG entry under `### Added`.

## 5. Effort estimate

| Phase | Effort |
|---|---|
| 1 script scaffold + helpers | ~1 h |
| 2 §3.3 body length + highlights builder | ~30 min |
| 3 dry-run with a non-cut version (test the script's pre-flight checks) | ~30 min |
| 4 CHANGELOG + CLAUDE.md updates | ~15 min |
| **Total** | **~2 h** |

## 6. Risks

- **The script becomes outdated as the cut sequence evolves**:
  CHANGELOG format, branch convention, etc. may drift.
  Mitigation: keep the script's individual functions small +
  named so the maintainer can edit one without breaking others.
- **`sleep 30` after `cargo publish -p nlink-macros` is
  flaky**: crates.io index propagation can take longer (or
  return cached "not found" briefly). Mitigation: loop with
  `cargo search nlink-macros` until the new version appears,
  with a 5-min cap.
- **`gh` auth not available**: script bails early with a clear
  message. Pre-flight check.

## 7. Out-of-scope follow-ups

- **Pre-cut auto-PR for the CHANGELOG promotion** (open the
  promotion PR, let CI run, then auto-merge): adds GH App
  surface, not worth the complexity for a manual cut cadence.
- **Auto-detect the version from `Cargo.toml`** instead of
  taking it as an arg: nice but the version was already bumped
  manually mid-cycle (per 0.16's `7b70850`), so the script can
  validate `cargo metadata` matches the arg.
- **A `cut-prerelease.sh` for `-rc.N` tags**: future, when
  there's a real audience for RCs.

End of plan.
