# npm (Node)

## Detect

- Look for `package.json` and a lockfile: `package-lock.json`, `pnpm-lock.yaml`, or `yarn.lock`.
- Match the package manager to the lockfile (do not switch managers unless asked).

## Scan outdated

Via `run_command` in the package directory:

- npm: `npm outdated --json` (or plain `npm outdated`)
- pnpm: `pnpm outdated`
- yarn: `yarn outdated`

Also read `package.json` `dependencies` / `devDependencies` / `peerDependencies` with `read_file`.

## Apply bumps

- Prefer the repo’s package manager:
  - npm: `npm install package@version` / `npm update`
  - pnpm: `pnpm add package@version` / `pnpm update`
  - yarn: `yarn add package@version` / `yarn up`
- Keep lockfile changes in the same commit as `package.json`.
- Do not delete the lockfile to “refresh” it.

## Verify

Prefer:

1. `npm test` / `pnpm test` / `yarn test` if defined
2. `npm run build` (or equivalent) if that is the project’s gate
3. At least `npm ls` / install success when no tests exist

## npm-specific caution

- Always run the **dependency comment pass** (`dep-comments.md`). For `package.json` (no comments),
  look for sibling docs / `DEPENDENCIES.md` / nearby `depbot:` notes in the package folder.
- **Major** bumps of `react`, `next`, `vue`, `angular`, `typescript`, bundlers, and test runners → separate PR + release notes.
- Watch `peerDependencies` mismatches after bumps.
- Prefer not to bump `engines` / Node version requirements unless asked.
- Security advisories (`npm audit`) may justify a focused PR; still verify install/tests.
