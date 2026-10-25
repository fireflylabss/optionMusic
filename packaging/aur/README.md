# still music on AUR

Two packages install the same `still` and `msc` commands:

- **stillmusic** builds the pinned GitHub release from source with Cargo.
- **stillmusic-bin** installs the prebuilt x86_64 Linux release asset.

The GitHub repository remains `fireflylabss/optionMusic`. Both AUR packages
conflict with the previous `optionmusic` and `optmusic` packages so an AUR helper can remove
an older package during installation. The old AUR package is preserved; these
recipes do not request its deletion or merge.

```bash
yay -S stillmusic
# or, to install without compiling:
yay -S stillmusic-bin
```

The runtime dependencies are MPV, SQLite, glibc and libgcc. `cava` adds spectrum
bars; `yt-dlp` adds downloads; `ffmpeg` adds audio extraction and conversion;
`tar` enables backup export and import. The source package also needs Cargo
and pkgconf during its build. The binary package does not invoke Cargo.

## Release metadata

Publish the GitHub tag and its Linux binary archive first. Then, on Arch Linux:

Recipes live on the default branch and are refreshed after the release exists;
the release source archive cannot contain its own final checksum. For the first
still release, the new recipes are added after the tag and binary are published.

```bash
./packaging/aur/bump.sh v0.3.0-beta.1
```

This downloads both the tag archive and
`stillmusic-v0.3.0-beta.1-x86_64-unknown-linux-gnu.tar.gz`, checks their expected
contents, calculates real SHA-256 checksums and regenerates both `.SRCINFO`
files with `makepkg --printsrcinfo`. A `v0.3.0-beta.1` tag uses Arch version
`0.3.0beta1`, which sorts before the eventual `0.3.0` stable package. Neither
recipe is updated if one download or metadata generation fails. Release archives must remain unchanged afterward.

## Publish

The [AUR workflow](../../.github/workflows/publish-aur.yml) updates both packages
after the release assets are ready. It requires the `AUR_SSH_PRIVATE_KEY` secret,
whose public key is registered on the maintainer's AUR account.

For a local publish:

```bash
./packaging/aur/publish.sh v0.3.0-beta.1  # refresh metadata and publish both
./packaging/aur/publish.sh              # publish already refreshed metadata
```

The helper verifies both source checksums before pushing, maintains separate
repositories at `~/aur/stillmusic` and `~/aur/stillmusic-bin`, and verifies each
remote commit after pushing. It uses `~/.ssh/aur_synara`; set `AUR_SSH_KEY` to
override the key or `AUR_DIR` to override the parent of the two AUR checkouts.
It refuses dirty checkouts and propagates fetch/rebase errors.

Publishing a package repository and seeing it indexed in the AUR web interface
are separate checks. Verify both package pages after publication:

- https://aur.archlinux.org/packages/stillmusic
- https://aur.archlinux.org/packages/stillmusic-bin
