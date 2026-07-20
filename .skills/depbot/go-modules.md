# Go modules

## Detect

- Look for `go.mod` (and `go.sum`) at the workspace root or in modules under the tree.
- Multi-module repos: identify which module the user meant; do not silently bump every module.

## Scan outdated

Typical commands via `run_command` (cwd = module directory):

- `go list -m -u all` — see available updates (noisy; filter mentally).
- `go list -m -f '{{if .Update}}{{.Path}} {{.Version}} -> {{.Update.Version}}{{end}}' all` — outdated only when supported.
- Prefer official tooling already in the repo (Makefiles, scripts) if present.

Do not run network-heavy loops forever; one coherent scan pass is enough for a plan.

## Apply bumps

- Edit `go.mod` carefully or use `go get package@version`.
- Always follow with `go mod tidy` in that module.
- Ensure `go.sum` stays consistent; do not hand-edit `go.sum` unless you must fix a conflict and understand it.

## Verify

Prefer, in order, whatever exists and is cheap enough:

1. `go test ./...` (or a narrower package set if the module is huge)
2. `go build ./...`
3. Repo-documented `make test` / CI script for Go

If tests are unavailable, say so and at least ensure `go build` / `go list` succeeds.

## Go-specific caution

- Always run the **dependency comment pass** (`dep-comments.md`) on `go.mod` before bumps.
- Treat `golang.org/x/...`, database drivers, and major framework modules as **high risk** when major/minor jumps look large.
- Respect `go` directive in `go.mod`; do not bump the language version unless asked.
- Avoid `replace` directives unless the repo already uses them and the user wants that path.
