# Coupled dependencies (bundles)

Some pins are **not independent**. They share a version family, codegen path, or
release train. Renovate often bumps one line; depbot must treat them as a **bundle**:
scan, unlock, plan, apply, and verify **together** — never partially.

This is universal policy, not buffa-specific.

## When to use a bundle

Declare a bundle when **any** of these apply:

- Two or more manifests must stay on the **same version** (crate + plugin, app + types).
- A bump requires **regeneration** or lockfile refresh across ecosystems (proto → Rust).
- Unlock needs **multiple evidence sources** (registry A **and** registry B **and** a tag on BSR).
- A human comment says *lockstep*, *aligned*, *move together*, or names sibling pins.

If unsure, prefer a bundle over risking a half-applied bump.

## Marker convention

Use a shared bundle id on **every** member (same id on each line or in adjacent comments):

```toml
# depbot: bundle buffa-codegen
# depbot: hold =0.8.1 — bump bundle to 0.9.x when ALL unlock (see coupled-deps.md):
#   - crates.io: buffa, buffa-types @ 0.9.0
#   - GitHub: anthropics/buffa release v0.9.0
#   - BSR remote: buf.build/anthropics/buffa:v0.9.0 (tag must exist; GitHub alone is NOT enough)
buffa = { version = "=0.8.1", … }
```

```yaml
# depbot: bundle buffa-codegen — member: BSR plugin remote (must match workspace buffa family)
# depbot: hold v0.8.1 — unlock with bundle buffa-codegen (all conditions above)
- remote: buf.build/anthropics/buffa:v0.8.1
```

Grammar:

| Marker                        | Meaning                                                  |
| ----------------------------- | -------------------------------------------------------- |
| `depbot: bundle <id>`         | Pins in this id move as one unit                         |
| `member:` / list in comment   | Optional human hint of what the pin represents           |
| `hold` on any member          | Blocks the **whole bundle** until unlock is satisfied    |
| `bump bundle to X when ALL …` | Every listed condition must pass before any member bumps |

Natural-language “keep in lockstep with …” without `bundle` still implies coupling —
infer the bundle from context, then **name it** in the plan table.

## Workflow (mandatory)

1. **Discover bundles** during the comment pass (`rg 'depbot: bundle'` and lockstep language).
2. **List members** per bundle: ecosystem, file, pin, how you check “available”.
3. **Scan each member** with the right scanner (`cargo`, BCR, BSR, npm, …).
4. **Reconcile unlock** for the **bundle**, not per line:
   - If **any** unlock condition for **any** member is unmet → **entire bundle blocked**.
   - If **any** member is held and user did not override → **entire bundle blocked**.
   - Partial evidence (e.g. crates.io + GitHub but **no** BSR plugin tag) → **blocked**,
     report what is still missing. Do **not** treat one registry as a substitute for another
     unless the comment explicitly allows it.
   - Uncertain evidence counts as unmet (e.g. BSR HTML page does not clearly show the
     target tag) → **blocked**; never guess that a remote exists.
5. **Plan / PR**: one row per bundle in **Coupled bundles**; action is bump / hold / blocked
   for the **set**, not split verdicts like “crate unlocked, plugin blocked” that imply a partial bump.
6. **Apply**: in one change-set (usually one commit / one PR section):
   - bump **all** member pins to the agreed version family;
   - run required regen (`bazel run //api:generate`, `buf dep update`, `go mod tidy`, …);
   - refresh **all** affected lockfiles;
   - update or remove stale `depbot:` comments on **every** member.
7. **Verify** after the full bundle is applied (build/test/codegen), not after the first file.

## Anti-patterns

- “crates.io has 0.9.0 → unlock buffa” while BSR plugin `v0.9.0` is unconfirmed.
- Assuming a BSR remote tag exists because GitHub/crates.io shipped the same version.
- Bumping `Cargo.toml` but leaving `buf.gen.*.yaml` on the old plugin tag.
- Reporting one member as **candidate bump** and another as **hold** within the same bundle.
- Splitting a bundle across PRs unless the user explicitly asked to stage (and you document risk).

## Reporting (dry-run and PRs)

Always include when any bundle exists:

```markdown
## Coupled bundles

| Bundle          | Members                                                              | Pinned         | Target | Unlock status                  | Action                                                     |
| --------------- | -------------------------------------------------------------------- | -------------- | ------ | ------------------------------ | ---------------------------------------------------------- |
| `buffa-codegen` | `Cargo.toml` buffa+buffa-types; `buf.gen.rust.yaml` anthropics/buffa | 0.8.1 / v0.8.1 | 0.9.x  | crates.io ✓ GitHub ✓ BSR tag ✗ | **blocked** — need BSR `buf.build/anthropics/buffa:v0.9.0` |
| `example-pair`  | …                                                                    | …              | …      | all met                        | **bump bundle** in one PR                                  |

**Bundle rule:** no member of a blocked bundle may be bumped alone.
```

For unlocked bundles, list the **exact files** that will change and the **regen** step.

## Relation to `grouping.md`

- **Coupled bundle** = same version train; must land together (logical atomicity).
- **PR group** = how you split work for human review (may be one PR per bundle, or user-requested batch).

Default: **one PR per unlocked bundle** when the bundle touches multiple ecosystems or codegen.
