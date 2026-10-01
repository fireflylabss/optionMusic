optionMusic for macOS
=====================

Contents
--------
  optionMusic.app    the GPUI desktop player (Apple Silicon, arm64)

Requirements
------------
  * macOS 12 or later, Apple Silicon (arm64)
  * libmpv — install with Homebrew:

        brew install mpv

First launch
------------
The app is not signed or notarized. On first launch macOS will warn
that it comes from an unidentified developer — either:

  * right-click the app and choose Open, or
  * clear the quarantine flag:

        xattr -dr com.apple.quarantine /path/to/optionMusic.app
