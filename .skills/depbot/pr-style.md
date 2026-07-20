# PR style

## When to open a PR

Only when the user asked to open / create a PR (or clearly said “ship it” / “open the PR”).
For plan-only or dry-run requests: report the plan and stop.

## Git hygiene

Typical sequence via `run_command`:

1. `git status` / `git diff` — confirm only intended files changed.
2. Create a branch: `git checkout -b deps/<short-topic>` (or `depbot/<short-topic>`).
3. Stage relevant files only; commit with a clear message.
4. Push and open with `gh pr create` (if `gh` is authenticated).

Do not amend published history. Do not force-push. Do not use `git clean` / `reset --hard`.

## Branch naming

- `deps/go-<topic>` or `deps/npm-<topic>` or `deps/<ecosystem>-<date>`
- Keep it short and unique enough for concurrent bots/humans.

## Commit message

- Imperative, specific: `Bump golang.org/x/net to v0.x` or `Update npm patch dependencies`.
- Avoid vague `update deps` when the set is small enough to name.

## PR title

- One line, reviewable: what + scope.
- Examples:
  - `deps(go): bump x/crypto and x/net (patch)`
  - `deps(npm): minor updates for linting toolchain`
  - `deps(npm): major bump typescript to 5.x`

## PR body (template)

Use this structure (adapt sections as needed):

```markdown
## Summary
- What was updated and why (security / routine / requested major / comment unlock).

## Dependency comments
- Quote relevant `depbot:` / human holds; note which unlocks were satisfied or still blocking.

## Coupled bundles
- Table per `coupled-deps.md`: bundle id, all members, unlock status, single action (bump bundle / blocked).

## Changes
- List packages: `name` `old -> new` (group if long).

## Risk
- Low / medium / high and why.
- Link release notes or migration guides for majors.

## Test plan
- [ ] Commands you ran (tests/build) and results
- [ ] Anything not run (and why)

## Notes
- Follow-ups / leftover outdated packages / unmet comment conditions
```

## After opening

Paste the PR URL in the final answer. If `gh` fails (auth/network), leave the branch committed locally and explain how to finish.
