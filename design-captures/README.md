# UI captures

`capture.sh <out.png> [wait_secs]` runs the app and captures only its window
with `xdotool` and ImageMagick's `import`.

**Sandbox caveat:** the default sandbox cannot host X11 windows (winit
panics on `XMapRaised: BadDrawable`, and Xvfb segfaults). Run the capture on a
workstation display to produce usable before/after evidence.
