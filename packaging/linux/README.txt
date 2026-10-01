optionMusic for Linux
=====================

Contents
--------
  optionmusic-gpui     the GPUI desktop player binary (x86_64)
  optionmusic.desktop  freedesktop launcher entry
  icons/               hicolor icon theme set
  install.sh           installs binary + launcher + icons into ~/.local

Runtime dependencies
--------------------
  * libmpv (the player links against it at runtime):

        Debian/Ubuntu:  sudo apt install mpv
        Arch:           sudo pacman -S mpv
        Fedora:         sudo dnf install mpv       # RPM Fusion

  * A working GPU stack (Vulkan/GL); runs on Wayland or X11.

Install
-------
  ./install.sh                 # installs into ~/.local
  PREFIX=/usr/local sudo ./install.sh
  ./install.sh --uninstall     # remove

Then run `optionmusic-gpui` from a terminal or the "optionMusic" launcher.
