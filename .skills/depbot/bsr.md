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

Important limitation: `buf registry plugin info|commit|…` often **rejects protoc plugins**
(e.g. `anthropics/buffa`) with *“operation only supports buf check plugins”*.
`buf registry plugin label info …:vX.Y.Z` also **false-negatives** on protoc plugins
(even a pin that already works in this repo, e.g. `v0.8.1`, reports “label does not exist”).
Do **not** use those commands as unlock evidence for protoc remotes — use the probe below.

### Preferred — resolve probe via `buf generate` (temp dir)

This is the reliable machine-check for **protoc** remote plugin tags. Run in a **throwaway
directory under the workspace** (shell `cwd` must stay in-workspace), e.g. `.tmp/depbot-bsr-probe/`.
Never point `out:` at the real `rust/api/gen/…` during a dry-run scan. Add `.tmp/` to
`.gitignore` if missing; do not commit probe outputs.

```text
# Create /tmp/depbot-bsr-probe (or workspace .tmp/…) with:
#   buf.yaml (v2 module), one tiny .proto, buf.gen.yaml with the candidate remote
program: buf
args: ["generate"]
# cwd: that throwaway directory
```

Example `buf.gen.yaml` in the probe dir:

```yaml
version: v2
plugins:
  - remote: buf.build/anthropics/buffa:v0.9.0
    out: out
    opt: [file_per_package=true]
```

Interpret stderr/exit:

| Result | Meaning |
|--------|---------|
| exit 0, files under `out/` | **Tag exists** — BSR unlock condition for that remote is met |
| `not_found: plugin version "vX.Y.Z" was not found … with latest version "vA.B.C"` | **Tag missing** — cite the error; `latest version` is useful for the plan |
| other errors (auth, network, timeout) | **Not confirmed** → treat as unmet / blocked; do not guess |

Verified on this stack (buf 1.72): `v0.8.1` resolves; `v0.9.0` and fake tags return
`not_found` with `latest version "v0.8.1"` (GitHub/crates.io can be ahead of BSR).

Clean up the throwaway dir after the probe. Do not commit probe outputs.

### Supporting — GitHub releases

Many BSR protoc plugins are released in lockstep with a public GitHub repo/tag
(same `vX.Y.Z`). Example for buffa:

```text
program: curl
args: ["-fsSL", "https://api.github.com/repos/anthropics/buffa/releases?per_page=20"]
```

Look for `tag_name` / `name` matching the candidate plugin version (`v0.9.0`).
Cite the release URL in the plan. **Does not replace** the generate resolve probe when
the bundle comment requires a BSR tag.

### Supporting — crates.io (when a runtime crate is coupled)

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

### Do not rely on — HTML `curl` of buf.build pages

`https://buf.build/…` is a JS SPA shell; version labels are not in the HTML.
Treat that as **not confirmed**, never as unlock. Prefer the generate probe.

### Local pin files

Always `read_file` `buf.gen.*.yaml` and note the exact `remote:` pin and any nearby comments
(e.g. “Pin matches workspace buffa”).

## Reconcile with dependency comments

Example bundle (this repo) — see `coupled-deps.md`:

Required checks before proposing a **bundle** bump (all must pass):

1. crates.io shows target versions for **every** coupled crate member.
2. GitHub release exists for the plugin/crate project (supporting evidence only).
3. **BSR remote plugin tag** exists at the target version (e.g. `buf.build/anthropics/buffa:v0.9.0`) —
   confirm with the **`buf generate` resolve probe** in `bsr.md` (temp dir), not GitHub/HTML alone.
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
| plugin `anthropics/buffa`       | v0.8.1 | `buf generate` probe: `v0.9.0` not_found (latest v0.8.1); crates.io/GitHub 0.9.0 supporting only | **blocked** — bundle `buffa-codegen` |
```

Never write “no way to check BSR” when `buf` / GitHub / crates.io evidence is available.
