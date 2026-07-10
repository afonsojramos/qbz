# Contributing to QBZ

This project is actively evolving. Contributions are welcome, but we have a few rules to keep releases stable and avoid regressions (especially around audio output).

## Where the code lives

The live app is the Rust workspace under `crates/` — a single native process
with a Slint UI. UI code is in `crates/qbz-ui` (`.slint` + generated bindings)
and `crates/qbz` (the binary). The old Svelte `src/` and Tauri `src-tauri/`
trees were **deleted in 2.0.2**; they survive only at the git tag
`legacy-tauri-svelte` for reference. PRs against those paths cannot be merged —
port the change to `crates/` instead.

## Quick rules

- Write clear, concise English (no emojis in code, comments, or commit messages).
- Keep PRs focused and small when possible.
- Do not change app branding or legal disclaimers without discussing it first.
- Do not modify protected audio-backend behavior unless explicitly requested by the maintainer.

## Branch naming

We use a consistent branch naming scheme:

`<type>/<origin>/<branch_name>`

- `type`: `feature` | `bugfix` | `hotfix` | `refactor` | `release` | `chore` | `docs`
- `origin`:
  - `internal`: created/owned by maintainers
  - `external`: branches/commits authored by third-party contributors (PRs)

Examples:

- `feature/internal/offline-cache-encryption`
- `bugfix/internal/login-footer-alignment`
- `docs/internal/contributing-process`
- `feature/external/add-album-to-playlist`

## Branch workflow

We use a **pre-release integration branch** to keep `main` stable and release-ready at all times.

```
feature/xyz ──┐
bugfix/abc  ──┼──> pre-release ──> main (tagged release)
hotfix/123  ──┘
```

### Branch hierarchy

1. **`main`** - Releases ONLY. Protected branch. Merging here triggers a tagged release.
2. **`pre-release`** - Integration branch. All features and fixes merge here first.
3. **`feature/*`, `bugfix/*`, etc.** - Individual work branches.

### For contributors

**All PRs must target `pre-release`, not `main`.**

PRs targeting `main` will be closed and asked to retarget to `pre-release`.

### Procedure (maintainer)

1. **Triage**
   - Confirm scope and that it does not touch protected areas (audio routing/backends, credential storage, etc.) unless requested.
   - Verify PR targets `pre-release` (not `main`).
2. **Check out the PR**
   - `gh pr checkout <PR_NUMBER>`
3. **Rename the checked-out branch (local)**
   - Use an `external` branch name so it's obvious these commits are third-party authored:
   - `git branch -m <type>/external/<topic>`
4. **Merge to pre-release**
   - `git checkout pre-release`
   - `git merge --no-ff <type>/external/<topic>`
5. **Run checks**
   - Build/validate a touched core crate: `cargo check -p <crate>` (run from
     `crates/`). The full UI (`qbz`/`qbz-ui`) is a ~20–30 GB compile — see the
     README "Building from Source" section before attempting it.
6. **Push pre-release**
   - `git push origin pre-release`
7. **Close the PR with a comment** explaining it was merged to `pre-release`.

### Releasing to main

When ready to release:

```bash
git checkout main
git merge pre-release
git push origin main
git tag vX.Y.Z
git push origin vX.Y.Z
```

This is done exclusively by maintainers.

### Merge strategy note (to preserve “external” authorship)

If you want the git history to clearly show third-party authored commits, avoid “squash merge”.
Prefer:

- **Create a merge commit**, or
- **Rebase and merge** (preserves individual commits/authors)

## What to include in PRs

- A short description of the problem and solution.
- Screenshots for UI changes when possible.
- Notes about any breaking changes or migrations.

## What not to include

- Large refactors mixed with feature work.
- Changes that reintroduce removed UI/UX patterns (for example, exporting offline cache files).

---

## Internationalization (i18n)

QBZ ships 8 locales (`en es de fr pt ru ja nl`) as gettext `.po` files, bundled
via the `qbz-i18n` crate. Rules:

- **No hardcoded UI strings in `.slint`** — every string goes through
  `@tr("...")`.
- Adding or changing a string means updating **all** locale `.po` files, not
  just English.
- `@tr` property defaults are not reactive — re-seed from Rust on language
  change (see `crates/qbz-i18n` and `select_bundled_translation()`, called
  after `AppWindow::new()`).

### Checklist for PRs with UI Text

- [ ] No hardcoded strings in `.slint` — all text via `@tr`
- [ ] Every new/changed string updated across all 8 `.po` locales
- [ ] Reused an existing string where one already fit
