# Project goals

## Agreed direction

The initial goal is an accurate recreation of Alley Cat. Consider improvements
after the original behavior has been established and reproduced. The game
implementation uses Rust; supporting analysis tools can use other languages.
Linux is the initial implementation target. Keep platform interfaces portable
so another desktop port does not require rewriting game behavior.

The fidelity target includes the game's CGA and PCjr hardware paths. Establish
separate references for their palettes, input handling, timing, and audio where
they differ. Reproduce what the selected executable actually does on each
machine rather than assuming that every available hardware feature is used.
The supplied executable is the current investigation input; its precise release
identity and whether it will be the final target remain to be confirmed.

Accuracy includes game rules, movement, collisions, progression, graphics,
controls, music, and sound effects. Establish expected behavior and tolerances
from evidence. Do not silently normalize quirks or make gameplay depend on the
host's display refresh rate. Exact CPU-cycle equivalence and full PC emulation
have not been made requirements.

## Dependency preferences

Use as few dependencies as practical while providing portable windowing, input,
graphics presentation, and audio output. Evaluate build-time dependencies,
runtime libraries, and Rust transitive dependencies separately. No graphics or
audio library has been selected yet.

SDL3 through Rust bindings is an initial candidate, not an adopted dependency.
Its [2D renderer](https://wiki.libsdl.org/SDL3/CategoryRender) can present a pixel
buffer, and its [audio streams](https://wiki.libsdl.org/SDL3/CategoryAudio) accept
generated PCM samples. These facilities suggest one host library could cover
the presentation and output needs. Its
[platform support](https://wiki.libsdl.org/SDL3/FrontPage) includes Linux,
Windows, and macOS. Its
[Linux dependencies](https://wiki.libsdl.org/SDL3/README-linux) depend on build
configuration and which desktop/audio backends are used at runtime.

A candidate graphics design is a logical pixel buffer with original palette
behavior, presented through an SDL texture. Verify display aspect ratio and
scaling against the reference; preserving 320x200 source pixels does not by
itself establish the correct displayed proportions. SDL offers
[logical presentation](https://wiki.libsdl.org/SDL3/SDL_SetRenderLogicalPresentation)
and [texture filtering](https://wiki.libsdl.org/SDL3/SDL_SetTextureScaleMode).
An engine, image loader, font library, or custom GPU shader stack has not been
shown necessary for this design.

For audio, distinguish generating the original sound from sending samples to a
modern audio device. SDL supplies the latter. A speaker/timer model and, if the
executable uses it, a PCjr sound-chip model must supply the former. The
[DOSBox-X hardware reference](https://github.com/joncampbell123/dosbox-x/wiki/Guide%3ASound-card-support-in-DOSBox%E2%80%90X)
identifies the PCjr's SN76496 tone/noise generator. It does not establish which
features this game executable uses. Decide whether to use a separate emulator
library after identifying those features and reviewing accuracy, dependencies,
and license compatibility. Generic retro-sound resemblance is insufficient.

The [Rust SDL3 bindings](https://docs.rs/sdl3/0.20.0/sdl3/) expose optional image,
font, mixer, and network extensions; these are not initial requirements. Assess
the binding's needed APIs and dependency graph before adopting it.

## Next evidence to establish

1. Import the MZ executable into Ghidra with a recorded segment/load model and
   check startup, data references, and relocation handling against the survey.
   Tool use does not select the eventual reconstruction method.
2. Record repeatable CGA and PCjr reference configurations and captures. Include
   executable hash, emulator version, CPU/cycle settings, video and audio
   settings, input sequence, and capture format. An emulator is a recorded
   reference, not proof that every hardware detail is exact.
3. Resolve sound-device usage and timing sources in this executable before
   choosing a synthesis implementation or external sound-chip dependency.
4. Choose the RE approach and a small first reconstruction milestone from the
   resulting evidence. Public name and license remain separate open decisions.
