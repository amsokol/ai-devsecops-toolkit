# Bazel / bzlmod

How to check whether `bazel_dep` pins in `MODULE.bazel` (and related) are outdated.
Do **not** claim there is “no scanner” — use the [Bazel Central Registry](https://registry.bazel.build/).

## Detect

- `MODULE.bazel`, `*.MODULE.bazel` includes, `MODULE.bazel.lock`
- `bazel_dep(name = "…", version = "…")`
- Toolchain pins that are **not** BCR modules (e.g. `buf.toolchains(version = "v1.72.0")`) — handle separately

## Preferred scan: BCR metadata (machine-readable)

For each `bazel_dep` name, fetch module metadata from the Bazel Central Registry mirror on GitHub:

```text
https://raw.githubusercontent.com/bazelbuild/bazel-central-registry/main/modules/<name>/metadata.json
```

via `run_command`:

```text
program: curl
args: ["-fsSL", "https://raw.githubusercontent.com/bazelbuild/bazel-central-registry/main/modules/rules_rust/metadata.json"]
```

In `metadata.json`:

- `versions` — published versions (usually newest last or listed; compare carefully)
- `yanked_versions` — never propose yanked versions
- `homepage` / maintainers — for PR notes when needed

Human UI for the same data: [registry.bazel.build](https://registry.bazel.build/) → module page
(e.g. browse `rules_rust`, `bazel_lib`, `rules_buf`).

Compare **pinned version in `MODULE.bazel`** vs **newest non-yanked BCR version**.
Report: `current → available` per module.

Only use `curl` against prefixes listed in `.depbot/tools.yaml` → `curl.url_prefixes`
(typically BCR hosts under [registry.bazel.build](https://registry.bazel.build/)).

## Optional local Bazel helpers

If helpful and cheap:

- `bazel mod deps` / `bazel mod graph` — see resolved graph (does not by itself list “newer on BCR”)
- After a bump: `bazel mod tidy` if the repo uses it
- Verify with a narrow build (e.g. `bazel build //rust/ai-agent-kit:…` or documented target), not necessarily `//...` on first try

BCR `metadata.json` remains the source of truth for “is there a newer module version?”.

## Apply bumps

1. Edit `bazel_dep(…, version = "…")` in `MODULE.bazel` / included module files.
2. Respect `depbot:` comments (`dep-comments.md`).
3. Run a verification build; commit `MODULE.bazel.lock` if it changes.
4. Group policy: toolchain / rules bumps often belong in a **dedicated Bazel PR** (see `grouping.md`), separate from Cargo/npm app deps.

## Non-BCR pins (this repo)

Examples that are **not** solved by BCR module metadata alone:

| Pin | Where | How to check |
|-----|--------|--------------|
| Buf CLI `buf.toolchains(version=…)` | `MODULE.bazel` | rules_buf / Buf release notes; keep sha256 in sync when version changes |
| BSR plugins (`buf.build/anthropics/buffa:v…`) | `buf.gen.*.yaml` | BSR + match workspace `buffa` crate pin |
| Rust toolchain version | `rust.MODULE.bazel` | usually tied to workspace `rust-version`; do not bump casually |

When Cargo `buffa` and BSR `anthropics/buffa` must move together, say so explicitly and follow dependency comments.

## Reporting

In dry-run / plan output include:

```markdown
## Bazel (bzlmod) vs BCR
| Module | Pinned | BCR latest (non-yanked) | Action |
|--------|--------|-------------------------|--------|
| rules_rust | 0.71.3 | … | bump / hold / blocked by comment |
```

Cite BCR ([registry.bazel.build](https://registry.bazel.build/)) as the check source — never “no scanner available”.
