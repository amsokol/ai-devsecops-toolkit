# Dependency comments (depbot advantage)

Renovate mostly follows version ranges and config DSL. **Depbot must read human comments**
near dependency declarations and treat them as first-class policy — blockers, unlock
conditions, hold reasons, and intended target versions.

This is a core product differentiator. Never skip the comment pass.

## Where to look

Before proposing or applying bumps, search and read comments in:

- Manifest lines and the few lines above/below each dependency
  (`Cargo.toml`, `go.mod`, `package.json`, `MODULE.bazel`, `buf.yaml`,
  `buf.gen.*.yaml`, `buf.lock`, lock-adjacent notes)
- Nearby `#` / `//` / `/* */` comments and TOML/JSON is awkward — for JSON, check
  sibling `*.md` / `DEPENDENCIES.md` / `docs/deps*` if present
- `rg` for: `depbot:`, `bundle`, `pin`, `hold`, `do not bump`, `until`, `when`,
  `blocked`, `lockstep`, `aligned`, `move together`, `FIXME.*dep`, `TODO.*bump`,
  package names about to be updated

## Preferred convention (`depbot:` markers)

Teams should leave machine-friendly notes next to pins:

```toml
# depbot: bundle buffa-codegen
# depbot: hold =0.8.1 — bump bundle to 0.9.x when ALL unlock (coupled-deps.md):
#   - crates.io: buffa, buffa-types @ 0.9.0
#   - GitHub: anthropics/buffa release v0.9.0
#   - BSR remote: buf.build/anthropics/buffa:v0.9.0 (tag must exist; GitHub alone is NOT enough)
buffa = { version = "=0.8.1", default-features = false, features = ["std", "fast-utf8", "json"] }
```

```go
// depbot: do not bump major until go1.23 CI image is rolled out
require github.com/example/lib v1.2.3
```

```text
// depbot: ok to patch/minor; majors only with explicit human approval
```

Marker grammar (informal, parse with judgment):

| Phrase                               | Meaning                                                                                      |
| ------------------------------------ | -------------------------------------------------------------------------------------------- |
| `hold` / `pin` / `do not bump`       | Block automatic bumps unless condition met or user overrides                                 |
| `bundle <id>`                        | Member of a coupled set — see `coupled-deps.md`; hold/unlock applies to the **whole bundle** |
| `bump to X when …` / `until …`       | Allowed target + unlock condition                                                            |
| `bump bundle to X when ALL …`        | Every listed condition must pass before **any** bundle member bumps                          |
| `ok to patch` / `patch only`         | Cap at patch (or patch+minor if said)                                                        |
| `security ok` / `security exception` | Security bumps may bypass a soft hold (still report)                                         |

Natural-language comments **without** the `depbot:` prefix still count if they clearly
refer to that dependency. Prefer adding a `depbot:` marker when you touch the line so
the next run is unambiguous.

## Workflow integration

1. **Discover** manifests.
2. **Comment pass** — collect holds / unlock conditions / intended targets, and discover
   **coupled bundles** (`rg 'depbot: bundle'|lockstep|aligned`; see `coupled-deps.md`).
3. **Scan** outdated versions with ecosystem tools.
4. **Reconcile**:
   - Identify **coupled bundles** (`coupled-deps.md`); unlock and bump decisions are **per bundle**, never per isolated line when members share a `depbot: bundle` id or lockstep comment.
   - If outdated **and** unlock condition is satisfied → candidate to bump (to the
     commented target when specified, else latest allowed by grouping policy).
   - If outdated **but** hold/condition unmet for **any bundle member** → **do not bump any member**; list under “blocked by comment” with the quote and what’s still missing.
   - If a comment names a future version that is **not yet published** → report
     “waiting on upstream”, do not invent a bump.
   - If a comment conflicts with the user request (“bump everything”) → ask or
     follow the user’s explicit override and mention the overridden comment in the PR.
5. **After a successful unlock bump** — update or remove the `depbot:` comment so it
   does not stale-block the next run (replace with a short note if a new hold applies).

## Reporting

Always surface comment-driven decisions in plans and PR bodies:

```markdown
## Dependency comments
- `buffa-codegen` bundle held at `0.8.1` — unlock: ALL conditions in comment (see Coupled bundles)

## Coupled bundles
- `buffa-codegen`: blocked — crates.io ✓, GitHub ✓, BSR plugin tag v0.9.0 not confirmed
```

If you only do a dry-run, the comment analysis section is still required.

## Anti-patterns

- Ignoring comments because `cargo update` / `npm outdated` looks clean or noisy
- Bumping past an explicit hold “to help”
- Leaving obsolete `depbot: hold … bump to 0.9.0` comments after 0.9.0 is already applied
- Bumping one member of a coupled bundle while leaving siblings on the old pin
