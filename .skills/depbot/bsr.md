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

### Fallback 1 — GitHub releases (version existence)

Many BSR protoc plugins are released in lockstep with a public GitHub repo/tag
(same `vX.Y.Z`). Example for buffa:

```text
program: curl
args: ["-fsSL", "https://api.github.com/repos/anthropics/buffa/releases?per_page=20"]
```

Look for `tag_name` / `name` matching the candidate plugin version (`v0.9.0`).
Cite the release URL in the plan.

### Fallback 2 — crates.io (when a runtime crate is coupled)

If the plugin is tied to a crate (this repo: workspace `buffa`):

```text
program: curl
args: ["-fsSL", "-A", "ai-agent-depbot", "https://crates.io/api/v1/crates/buffa"]
```

(or `…/crates/buffa/versions`)

Compare crate latest vs `Cargo.toml` pin vs `buf.gen.*.yaml` remote plugin tag.
**Unlock for coordinated bumps** only when crate **and** plugin version family align
(or the `depbot:` comment’s condition is otherwise satisfied).

### Fallback 3 — local pin files

Always `read_file` `buf.gen.*.yaml` and note the exact `remote:` pin and any nearby comments
(e.g. “Pin matches workspace buffa”).

## Reconcile with dependency comments

Example (this repo):

```toml
# depbot: hold =0.8.1 — bump to =0.9.0 when buffa 0.9 is released and the BSR
# anthropics/buffa plugin / codegen in this repo are aligned …
```

Required checks before proposing the bump:

1. crates.io (or `cargo update --dry-run -v`) shows `0.9.0` available.
2. GitHub release `v0.9.0` (or equivalent) exists for the plugin/crate project.
3. Plan states that `Cargo.toml` **and** `buf.gen.rust.yaml` must move together, then regenerate.

If crate `0.9.0` exists but you cannot confirm a matching plugin release → keep **blocked**
and say what evidence is missing. Do not claim “BSR unknown, bump anyway”.

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
| plugin `anthropics/buffa`       | v0.8.1 | GitHub release v0.9.0 / crates.io 0.9.0 | blocked by comment / unlock |
```

Never write “no way to check BSR” when `buf` / GitHub / crates.io evidence is available.
