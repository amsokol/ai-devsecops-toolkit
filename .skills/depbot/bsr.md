# Buf Schema Registry (BSR)

How to check Buf **modules** and **remote plugins** pinned in this repo.
Do **not** skip BSR when a `depbot:` comment mentions plugin/codegen alignment
(e.g. `buffa` hold until BSR `anthropics/buffa` matches).

Human UI: [buf.build](https://buf.build/) (e.g. [buf.build/anthropics/buffa](https://buf.build/anthropics/buffa)).

## Detect

| Kind          | Where                                 | Example                                   |
| ------------- | ------------------------------------- | ----------------------------------------- |
| Module dep    | `buf.yaml` → `deps:`                  | `buf.build/bufbuild/protovalidate:v1.2.2` |
| Module lock   | `buf.lock`                            | resolved commit/digest                    |
| Remote plugin | `buf.gen.*.yaml` → `plugins[].remote` | `buf.build/anthropics/buffa:v0.8.1`       |
| Coupled crate | `Cargo.toml` / other manifests        | `buffa = "=0.8.1"`                        |

Always run the **comment pass** (`dep-comments.md`) on these files first.

## Scan BSR modules (preferred: `buf` CLI)

Allowlisted program: `buf`.

```text
program: buf
args: ["registry", "module", "label", "list", "buf.build/bufbuild/protovalidate", "--format", "json", "--page-size", "50"]
```

- Compare pinned label (`v1.2.2`) to newer **version-like** labels (`v*`).
- Ignore branch-like labels (`main`, feature branches) unless the pin uses them.
- Paginate with `--page-token` if `next_page` is present.

Also useful: `buf registry module info buf.build/owner/module --format json`.

## Scan BSR remote plugins (protoc plugins)

Important limitation: `buf registry plugin …` often **rejects protoc plugins**
(e.g. `anthropics/buffa`) with *“operation only supports buf check plugins”*.
Do **not** stop there — use the fallbacks below.

### Fallback 1 — BSR plugin page / registry (preferred for tag existence)

Check that the **remote plugin tag** exists on BSR before treating a bundle as unlocked.
GitHub/crates.io are supporting evidence only when the comment requires a BSR tag.

```text
program: curl
args: ["-fsSL", "https://buf.build/anthropics/buffa"]
```

Or inspect the module/plugin page and note the published version labels.

**Ambiguity rule (mandatory):** HTML/UI pages are imperfect scanners. Unlock only when
the target tag (e.g. `v0.9.0`) is **unambiguously** present in the evidence you cite.
If the page is empty, truncated, JS-rendered without the tag text, or you cannot quote
where the tag appears → treat as **not confirmed** → bundle **blocked**. Do **not**
infer the BSR tag from GitHub/crates.io. Cite what you checked and what was missing.

### Fallback 2 — GitHub releases (supporting evidence)

Many BSR protoc plugins are released in lockstep with a public GitHub repo/tag
(same `vX.Y.Z`). Example for buffa:

```text
program: curl
args: ["-fsSL", "https://api.github.com/repos/anthropics/buffa/releases?per_page=20"]
```

Look for `tag_name` / `name` matching the candidate plugin version (`v0.9.0`).
Cite the release URL in the plan. **Does not replace** BSR tag confirmation when the bundle comment requires it.

### Fallback 3 — crates.io (when a runtime crate is coupled)

If the plugin is tied to a crate (this repo: workspace `buffa`):

```text
program: curl
args: ["-fsSL", "-A", "ai-agent-depbot", "https://crates.io/api/v1/crates/buffa"]
```

(or `…/crates/buffa/versions`)

Compare crate latest vs `Cargo.toml` pin vs `buf.gen.*.yaml` remote plugin tag.
**Unlock for a coupled bundle** only when **every** member’s evidence passes
(see `coupled-deps.md`). crates.io + GitHub **without** a confirmed BSR plugin tag
does **not** unlock a bundle that includes that remote pin.

### Fallback 4 — local pin files

Always `read_file` `buf.gen.*.yaml` and note the exact `remote:` pin and any nearby comments
(e.g. “Pin matches workspace buffa”).

## Reconcile with dependency comments

Example bundle (this repo) — see `coupled-deps.md`:

Required checks before proposing a **bundle** bump (all must pass):

1. crates.io shows target versions for **every** coupled crate member.
2. GitHub release exists for the plugin/crate project (supporting evidence only).
3. **BSR remote plugin tag** exists at the target version (e.g. `buf.build/anthropics/buffa:v0.9.0`) — confirm via BSR/registry evidence documented in `bsr.md`, not inferred from GitHub alone.
4. Plan states **all** member files that move together, then regenerate.

If any check fails → bundle stays **blocked**; do not bump individual members.

## Apply (when unlocked and requested)

1. Bump crate pin + BSR remote plugin tag to the same version family.
2. Regenerate (`bazel run //api:generate` or documented buf generate path).
3. `cargo test` / relevant bazel targets.
4. Refresh/remove stale `depbot:` comments.

## `curl` allowlist

Prefixes are configured in `.depbot/tools.yaml` (`curl.url_prefixes`), enforced by the runtime.
Typical entries for BSR work: `https://buf.build/`, `https://api.github.com/repos/`, `https://crates.io/api/`.

## Reporting

```markdown
## BSR
| Asset                           | Pinned | Available evidence                      | Action                      |
| ------------------------------- | ------ | --------------------------------------- | --------------------------- |
| module `bufbuild/protovalidate` | v1.2.2 | label list: …                           | …                           |
| plugin `anthropics/buffa`       | v0.8.1 | BSR tag v0.9.0 ✗; GitHub/crates.io 0.9.0 (supporting only) | **blocked** — bundle `buffa-codegen` |
```

Never write “no way to check BSR” when `buf` / GitHub / crates.io evidence is available.
