# AUR packaging (`optmusic`)

Published: https://aur.archlinux.org/packages/optmusic

## Install (users)

```bash
yay -S optmusic
# or
paru -S optmusic
```

## Maintain (you)

Local working copy: `~/aur/optmusic` (tracks `ssh://aur@aur.archlinux.org/optmusic.git`).

SSH key used: `~/.ssh/aur_synara` (override with `AUR_SSH_KEY=`).

### After a new GitHub tag/release

```bash
# from the optMusic repo root
./packaging/aur/publish.sh 0.2.5
```

That script bumps `pkgver` + `sha256sums`, regenerates `.SRCINFO`, commits, and pushes to the AUR.

### Same version (PKGBUILD-only tweak)

```bash
./packaging/aur/publish.sh
```
