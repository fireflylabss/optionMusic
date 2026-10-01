optionMusic for Windows
=======================

Contents
--------
  optionmusic-gpui.exe   the GPUI desktop player (x86_64)
  mpv-2.dll              libmpv, required at runtime — keep it next to the .exe

Runtime
-------
No installer: unzip anywhere and run optionmusic-gpui.exe.
Do not move the .exe away from mpv-2.dll; the player loads libmpv
from the same directory.

The bundled mpv-2.dll comes from the mpv-dev-x86_64 package of
shinchiro/mpv-winbuild-cmake. To update MPV independently, drop a
newer mpv-2.dll build next to the .exe.
