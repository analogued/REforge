# Deliberate deviations from the original

This records intentional corrections and presentation or usability changes to
the supported DOS game. The [player guide](../../../docs/games/mach3.md) explains
player options. Supported releases and hardware are covered by its
[current limits](../../../docs/games/mach3.md#current-limits); comparison coverage
and timing uncertainties are described in the
[validation scope](dos-gameplay.md#validation-scope),
[timer reference](dos-gameplay.md#timer-pacing) and
[sound execution reference](sound-assets.md#reconstructed-execution-interface).

## Visual motion

The original publishes discrete sprite placements at its native update and
drawing sites. Reforged, the interactive default, constructs smoother visual
trajectories for the craft, shadow and world objects using finer projection and
bounded prediction. It composes fresh scenes from complete sprites and
procedural gate geometry, removing the original drawing history's retained
fragments. Deactivated gates disappear at the observed event. Mountains keep
their native artwork and updates.

Enhanced smooths only the craft and shadow while retaining native world drawing
history. Original uses native placements and publications. These presentation
choices leave reconstructed input polls, collisions, random consumption,
PIT/sound execution and the gameplay corrections below in force. Recording replay
and headless exports retain native presentation. Predicted visual positions can
differ from collision positions, and host stalls can cause catch-up corrections.

See [motion options](../../../docs/games/mach3.md#motion) and the
[presenter reference](../README.md#visual-motion-presenter) for selection,
prediction bounds, composition and lifecycle rules.

## Enhanced PCM

The optional Enhanced PCM renderer uses the stored eight-bit amplitudes during
recognized sample playback, preserving more source precision than the original
speaker conversion. Procedural effects retain the speaker signal. The same
reconstructed execution supplies source visits and timestamps to both renderers;
selection, interruption, timer writes and gameplay effects remain unchanged.

Sample setup/return use explicit midpoint and residual-pulse suppression policies.
This is an enhanced interpretation of the assets rather than a Sound Blaster
driver or an acoustically validated model. Speaker remains the default renderer.
See the [player option](../../../docs/games/mach3.md#game-specific-options) and
[rendering policy](sound-assets.md#enhanced-pcm-playback).

## F3 and host controls

The original toggles colour/monochrome on every active F3 poll. Interactive play
defaults to one toggle per press, requiring release before another toggle.
`--f3-mode original` restores the per-poll rule. Original-game recording replay
and headless execution retain that rule unless a caller explicitly selects the
frontend policy. F1/F2 processing and effective F3 palette changes retain their
native rules. See [session controls](dos-gameplay.md#session-and-gameplay-controls).

P, Pause/Break and controller Start/Menu provide a host pause that freezes the
simulation and queued playback together. Settings also uses host pause. Native
F1 retains its separate handshake and continuing device execution. SDL controller
input maps to digital steering and Fire; it adds no analog movement model.
See [controller mappings](../../../docs/configuration.md#controllers) and
[input boundaries](input-tracing.md#history-boundaries) for frontend handling.

## Splash startup and skip

The original splash advances automatically after its DOS-clock wait; Space does
not shorten it. The reconstruction fixes the initial BIOS tick count at zero
for deterministic startup, giving 219 accelerated interrupts (about 1.9823
seconds), and excludes incidental loading and instruction costs. Other original
clock phases can produce shorter waits.

A fresh Space press can skip this wait. That press is consumed until release
so it cannot also advance the intro. See
[startup timing](cga-graphics.md#intro-and-gameplay-transition).

## Expanding scene transitions

The original seven-region reveal uses bounded CGA retrace polling with interrupts
disabled. Its duration depends on CPU/rendering cost and display phase. Live
playback assigns each region `floor(PIT_HZ / 60)` clocks, about 16.7 ms, for a
total near 116.7 ms. These are fixed simulation intervals, independent of monitor
refresh rate. Device and keyboard interrupts continue while the foreground game
waits for the reveal to finish. Host stalls can prevent intermediate images from
appearing onscreen while their simulation intervals still execute.

This preserves the visual sequence without reproducing CPU-dependent polling
cost, interrupt masking or timeout behavior. See
[transition execution](dos-gameplay.md#expanding-scene-transitions).

## Original bug corrections

- **Falling-craft explosion argument:** the original wrapper omits a slot argument,
  leaving the handler to read stale stack data. The reconstruction passes the
  current craft explicitly, so ground contact starts and advances its explosion.
  See the [gameplay reference](dos-gameplay.md#rust-gameplay-simulation).
- **High-difficulty artwork:** the original indexes beyond its ten visual families
  and can stall HUD selection at difficulty 10. Runtime HUD marker and patrol
  artwork selection reuses family 9 at higher difficulties, including marker
  placement.
- **Command-delay overflow:** the original delay expression wraps back to a large
  positive wait at difficulty 4,099. The reconstruction caps its difficulty operand
  at 3, keeping immediate dispatch above difficulty 2.
- **Patrol-population overflow:** the original signed comparison disables patrols
  at difficulty 32,768 and above. An unsigned comparison retains one craft at
  zero, two at one and up to three thereafter.

The latter three corrections are detailed in
[shooting and progression](dos-gameplay.md#shooting-and-progression). The stored
difficulty remains a wrapping 16-bit word, and the external rule decoder retains
raw table access for inspection. Corrected original-code comparisons are kept
separate from unchanged-original comparisons; see
[validation scope](dos-gameplay.md#validation-scope).
