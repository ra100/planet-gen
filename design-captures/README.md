# UI captures

`capture.sh <out.png> [wait_secs]` runs the app and grabs a screenshot.

**Sandbox caveat:** this agent's sandbox cannot host X11 windows (winit
panics on `XMapRaised: BadDrawable`, and Xvfb segfaults), so captures taken
here show only the desktop — they are not usable for visual review. Run
`./design-captures/capture.sh design-captures/ui_v2.png 20` on a real
workstation display to produce before/after evidence.
