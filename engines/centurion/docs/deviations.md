# Intentional deviations

These describe the current native frontend. Gameplay reconstruction follows
verified DOS rules; the presentation changes below are deliberate. Known
reconstruction and timing limits are listed in the [engine guide](../README.md#validation-and-limits).

## Menu feedback

The native menus combine DOS keyboard-style selection boxes and highlighted text
with mouse-hover selection. DOS mouse mode normally uses a decorative vertical
column without highlighting the option under the pointer. Native keyboard and
mouse input remain available together.

Tribute and Tribute Policy add a subdued band and a current-level legend for the
stored choice, separately from the brighter focus highlight. The local menu
reads that province's tribute; the global menu reads stored policy, even after
local overrides. The display changes no tribute value, income, activation rule
or random draw. `--original-tribute-menus` removes this indicator for comparison;
it does not restore all DOS mouse-mode presentation.

The save-name prompt identifies the destination slot. This is an additional
clarity aid; descriptions continue to prefill from the current campaign rather
than the selected slot's previous name.

## Enemy-fleet information

DOS enemy inspection displays a fleet-name/location notice, including when Roman
fleets are present. The native frontend shows that notice alongside an additional
detailed fleet table, with one acknowledgment dismissing both. The extra table
exposes counts, transport assignment and readiness; it can overlap a long fleet
name with the trireme count. This combined presentation remains provisional.
It does not change fleet selection precedence, ship counts or naval mechanics.

## Host presentation and sound

The software pointer uses absolute host mouse coordinates. The host cursor is
hidden over the window without relative capture or added cursor smoothing.
Window sizing preserves the 4:3 display aspect of the original 320x200 image;
maximizing fits a normal window to that image rather than leaving a wider client
area around it.

AdLib output uses software OPL2-compatible synthesis and host PCM playback.
Audio generation runs independently of window rendering. Interactive pacing
follows estimated source-sample consumption with a wall-clock fallback; late
sound requests apply at the next unrendered sample. These host scheduling choices
retain logical scene timers and deterministic headless stepping, but do not
reproduce DOS CPU/VGA or physical audio-device timing exactly.

## Bounded undefined states

The DOS program can use uninitialized entries when its arena offer roster is
exhausted. The native engine bounds that search and reports an error instead of
following an arbitrary fighter index. Ordinary cancellation/availability effects
are retained. Other unresolved DOS stack/interrupt dependencies remain explicit
fidelity limits rather than broadly corrected game rules.
