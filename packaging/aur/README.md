# AUR packaging (`optmusic`)

Published: https://aur.archlinux.org/packages/optmusic

## Install (users)

```bash
yay -S optmusic
# or
paru -S optmusic
```

## Automatic publish (recommended)

Every **GitHub Release** runs [`.github/workflows/publish-aur.yml`](../../.github/workflows/publish-aur.yml):

1. Bumps `packaging/aur/PKGBUILD` + `.SRCINFO` for the release tag
2. Commits that bump back to `master`
3. Pushes the package to the AUR

### One-time setup

Add the AUR SSH **private** key as a repo secret:

1. GitHub → **Settings → Secrets and variables → Actions**
2. New secret name: `AUR_SSH_PRIVATE_KEY`
3. Value: contents of `~/.ssh/aur_synara` (the private key, not `.pub`)

Or from the CLI:

```bash
gh secret set AUR_SSH_PRIVATE_KEY < ~/.ssh/aur_synara
```

The public key must already be on your AUR account (it is, if you published 0.2.4).

### Day-to-day

```bash
# bump code, commit, then:
git tag -a v0.2.5 -m "optMusic 0.2.5"
git push origin v0.2.5
gh release create v0.2.5 --title "optMusic 0.2.5" --generate-notes
# → Actions publishes AUR automatically
```

Manual re-run: **Actions → Publish AUR → Run workflow**.

## Local publish (fallback)

```bash
./packaging/aur/publish.sh           # push current packaging/
./packaging/aur/publish.sh 0.2.5     # bump + push
```

Uses `~/aur/optmusic` and `~/.ssh/aur_synara` (override with `AUR_SSH_KEY=` / `AUR_DIR=`).
