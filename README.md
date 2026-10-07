# Alley Cat

An early Rust reconstruction of **Alley Cat** for modern desktop systems,
initially targeting Linux. The current program supports alley movement,
alley and room actors, damage, window entry, the fishbowl, bookcase, cheese, birdcage and milk rooms,
and the hearts bonus scene.
It is not a complete game.
A public recreation name has not been chosen.

## Original game required

The program requires your original `CAT.EXE` at runtime. It reads embedded
sprites, scenery, animation tables, and platform data from that file. Rust
implements the reconstructed behavior; it does not execute the DOS program.
Original game data is not included in this repository or embedded in builds.

Currently one executable is supported, identified by its exact bytes rather
than an unverified historical release name:

```text
Size:    55067 bytes
SHA-256: 4979c8867826e08881d1d4bec95c95d6bc1ac70fd21050ccdb4f733a50d7bfdd
```

The loader opens the file read-only and rejects other versions. Keep original
files and any generated captures outside this repository.

## Build and run

Install Rust 1.85 or newer with Cargo, and an SDL3 development package providing
the native `SDL3` library. The window backend was checked against SDL 3.4.16 on
Linux. It uses a small authored C interface; no Rust SDL binding crate or SDL
image/font/mixer extension is required. Cargo fetches `sha2` and its locked
dependencies for executable verification.

From this repository:

```sh
cargo run --locked --release -- --original /path/to/CAT.EXE
```

Launch starts with the original animated splash: a cat enters and walks along
the fence, and the copyright artwork alternates between SynSoft and IBM.
Press a key to continue, or wait for the original first-entry timeout of about
47 seconds. Desktop shortcuts, modifier keys alone and held-key repeats do not
dismiss it. The following menu omits the original joystick question. Select
K (Kitten), H (House Cat), T (Tomcat), or A (Alley Cat); the controls then appear
below the question. Press another key to start. These choices select initial
internal difficulties 0 through 3; progression can reach 7.
Ctrl+Q or window close quits during startup as well.

Use the arrow keys to move and jump. Esc, P and Pause/Break toggle pause;
other keys, including arrows, leave it paused. Pause shortcuts are ignored while
Alt, Ctrl or Super is held, and held-key repeats do not toggle pause. Losing or
regaining window focus does not change the pause state. Ctrl+Q or closing the
window exits, including while paused. Ctrl+R starts a new run, and Ctrl+M returns
to the skill menu to select a new run. Both also work while paused. After losing
all lives, press R to start a new run with the initial difficulty and seed;
the high score is retained.
In the cheese room, hold Alt at a hole to travel to its paired hole.
In the hearts scene, gifts are picked up automatically on contact; hold Alt
to place a carried gift near the cat's current position.

Pause preserves the game state and virtual clock without catching up on resume.
The overlay reads the original pause title from the executable and uses the
original placement and palette color, with an updated hint for the toggle keys.
Its lettering uses SDL's built-in 8×8 font. Scoring uses an attributed adaptation
of the same glyphs, including in headless builds.

`--machine pcjr` selects the PCjr palette and player countdown; the default is
`cga`. Both modes use the same packed 320×200, four-colour framebuffer. Pixels
have a 5:6 width-to-height ratio: the initial 1280×960 window displays them at
4:3, with nearest-neighbor sampling and no CRT shader. `--scale N` selects an
initial window size of `320*N` × `240*N`, with a default multiplier of 4 and
accepted values from 1 through 16. Use `--scale 3` for 960×720.
Resizing or maximizing preserves 4:3 with black bars as needed.
Host display vsync is requested when
supported; game timing still follows the virtual PIT/video clocks. Composite
monitor simulation is not implemented. Raw framebuffer and PPM exports remain
320×200 for data comparisons.

`--difficulty` accepts 0 through 7 and sets the initial difficulty, skipping the
skill question while retaining the controls and key-to-start wait. Normal builds
accept
`--original`, `--machine`, `--scale`, `--difficulty` and `--help`.

## Development tools

Testing options are compiled only with the `dev-tools` Cargo feature:

```sh
cargo run --locked --release --features dev-tools -- \
  --original /path/to/CAT.EXE --scene underwater
```

Without that feature, their parser branches, option fields, diagnostic runners
and export helpers are excluded; passing a testing flag reports an unknown
option. `cargo run --locked --features dev-tools -- --help` lists the development
options alongside the player options.

An explicit `--scene` bypasses the splash and menu; use `--scene alley` to start
gameplay immediately. `--scene splash` previews only the title screen. In that preview,
`--steps N` limits virtual BIOS ticks instead of accepted player updates, and
`--dump-cga`/`--dump-ppm` export the final title frame. Gameplay scripts, traces
and `--step-hz` are not available for splash previews. Headless gameplay runs
continue to bypass the splash by default.

`--seed` selects a nonzero 16-bit random seed, in decimal or `0xHEX` notation.
The internal default remains `0xfa59` in both builds.

`--original-window-timing` restores the original one-tick window hold at
difficulty 6. Both builds default to the corrected 18-tick hold. See
[deliberate deviations](docs/deviations.md) for this fix and the pause-control
usability change.

`--scene fishbowl`, `--scene bookcase`, `--scene cheese` and `--scene birdcage`
start directly in those rooms for testing; `--scene milk` starts the milk-bowl
room. The default is `--scene alley`. Direct room starts bypass the ordinary
selector and do not add that starting room to its repeat-prevention history.
Room actors and transitions use the default timed runner. `--scene hearts`
starts directly in the hearts scene with one gift. `--scene underwater` starts
inside the fishbowl. Fishbowl, underwater, bookcase, cheese, milk and hearts require
default windowed timing or `--headless --timed`.
`--scene completion` previews the birdcage scoring presentation and returns to
the alley, without playing the objective. It also requires default windowed
timing or `--headless --timed`.

`--disable-actors NAMES` removes selected actor sprites and skips their updates
and interactions. Accepts `all`, `none`, or a comma-separated list:
`alley-dog`, `room-dog`, `window-projectiles`, `milk-dogs`, `spider`, `eels`,
`bin-cat`, `alley-mice`, `broom`, `hearts-arrows`, `hearts-rivals`.
Fish remain enabled when eels are disabled, and room objectives remain playable.
`hearts-rivals` excludes the goal cat. Overrides are configured at startup;
initial placement may still consume randomness and shared schedulers retain
skipped opportunities. Later timing and random consumption can differ from an
unmodified run.

`--disable-damage SOURCES` accepts `all`, `none`, or a comma-separated list:
`alley-dog`, `room-dog`, `window-projectiles`, `milk-dogs`, `spider`, `eels`, `air`.
Actors still move and render when their damage is disabled. Contact does not
start their hit, pursuit, forced-fall or death response; disabling `air` retains
low-air warnings while suppressing sinking and fatal timeout. Actor removal and
damage suppression are independent. `--infinite-lives` preserves the life count
while retaining hit effects, death animations and returns to the alley.

Repeated disable flags replace the previous selection, so the last value wins.
Whitespace around list entries is accepted; unknown names and mixed `all`/`none`
lists are rejected. With no development switches, gameplay follows the same
harmful defaults as a normal build.

## Current behavior

The implemented slice includes seeded alley initialization, CGA bank addressing,
sprite composition and background restoration, and player movement on the floor,
dumpsters, and occupied upper platforms. Ground decorations use the original's
weighted random selection and external sprites. Current score, high score and
three starting lives use the original digit glyphs. Mouse catches and ordinary-room
completion award points. Hearts completion also awards points and an extra
life, capped at nine. Dogs, projectiles, the spider, eels and air expiry can
remove lives. Falling off scenery, broom contact, bin-cat pushes and clothesline
mouse knockdowns do not directly remove lives.

In the default timing mode, the clotheslines scroll right/left/right, generate
new garments, update landing supports, and carry an attached cat. Their original
call countdown, BIOS-tick and retrace gates control motion; scrolling pauses when
the cat is airborne in the relevant row. Releasing a cat at a clothesline edge
starts a fall without losing a life. The renderer composes complete scenery and
cat images rather than simulating partially updated display memory.

The bin cat emerges from randomly selected bins and pushes a supported player
off on contact. It also alerts the alley dog. The dog enters from a screen edge,
moves along the ground and starts the original pursuit on contact. Its final
exit removes a life and begins alley re-entry. Ordinary furnished rooms use the
same roaming-dog machinery, followed by the shared room-failure presentation.
Three clothesline mice move and animate using the shared random generator and
their own BIOS-tick gate. Contact knocks an attached cat off its line, with the
original eight-tick pause (about 0.44 seconds). An airborne cat instead catches
the mouse for 90, 70 or 50 points on the top, middle or bottom line. Caught mice
respawn after 54 ticks (about three seconds). Neither interaction removes a life.
Actor sprites and motion tables come from the required original executable.

Windows open, hold and close with the original invocation-count, retrace and
BIOS-tick gates. New openings require the cat to be above the fence, as in the
original. A descending cat touching an eligible open window enters a room.
The original weighted selector chooses fishbowl, bookcase, cheese, birdcage or
milk, using the shared random generator and two-entry history to avoid three
identical ordinary destinations in a row. Completing any ordinary room makes
the next eligible window display the female cat and lead to the hearts scene.
Windows can throw projectiles aimed toward the player. A hit forces a fall;
the first hit pauses game work for ten BIOS ticks and removes one life. Further
contact with that projectile can force another fall without another life charge.

In the fishbowl room, climb the furniture and jump into the bowl. Contact during
its six-tick animation gate enters the underwater section without requiring Alt.
Arrow keys then control swimming with acceleration, inertia and upward drift
after releasing vertical input. Eat all twelve fish to complete the room.
Fish and eels use the original slot scheduler, motion bounds, animation tables
and shared random generator; collecting fish reactivates disabled eels.
The water surface animates separately. Surface breathing resets the air timer
through the original sprite-dependent test, and palette changes warn of low air.
Eel contact triggers a thirteen-tick hit effect and room failure. Air expiry
forces the cat to sink with bubbles, then fails the room at its later fatal
threshold. Success and sinking follow the original eel-immunity predicates.
Completion uses the shared scoring presentation and returns directly to the alley
at the saved window position. Leaving the furnished room through its window
returns without an award.

In the bookcase room, climb the shelves and collect the three potted plants.
Books use the original seeded layout; shelf support uses the original geometric
rule rather than individual book outlines. The spider follows the original
pursuit, descent and ascent logic, including broom interference. Contact starts
a nine-tick effect, then the shared room-failure presentation and life loss.
Collecting the third plant starts the shared scoring presentation, then returns
to the alley with the next eligible window leading to hearts. Leaving through
the room window returns without an award.

In the cheese room, collect four mice as they emerge from the holes. Their
visibility, avoidance and relocation use the original shared random generator
and tick gates. The sixteen holes are paired by difficulty; hold Alt while
standing at one to travel to its partner. The cat disappears and emerges there
through a timed animation while the mice and broom continue updating. Cheese
supports, floor tracks and the broom use the ordinary-room rules. Collecting
the fourth mouse starts the shared scoring presentation and alley return.

In the birdcage room, push the cage off its shelf and catch the released bird.
Furniture supports, cage pushing/falling, bird movement, floor tracks and the
broom use the original tables and rules. The broom pushes the cat without
removing a life. Catching the bird starts the shared ordinary-room scoring
presentation: an expanding black wipe, cat-face silhouettes, a stepped reveal,
the four-digit award, a green/black background flash and an expanding color wipe
before returning to the alley. The reveal waits two BIOS ticks after each row;
the final flash lasts 30 ticks. The award includes the external room base score
and a time bonus sampled after the initial drawing. Lives are preserved, and
returning through the room window leaves the score unchanged. Alley return
restores the saved window position and starts a downward fall. Direct scene
previews instead save the initial ground-level alley position.

In the milk room, hold Alt near an unfinished bowl while standing on its level.
The cat aligns toward the bowl and plays the drinking animation, holding normal
movement until drinking stops. Empty all twelve bowls to start the shared
scoring presentation and alley return; this room uses its original slower
time-bonus decay. Higher difficulties increase the milk in each bowl and the
number of sleeping dogs. Nearby dogs open their eyes and become alert, with
probabilistic calming when the cat moves away. Fully alert contact begins a
pursuit from that dog's position, relocates the cat and ends in room failure.
The `milk-dogs` damage control covers both attention and this pursuit; `room-dog`
controls roaming dogs in the other furnished rooms. `--disable-actors milk-dogs`
removes them while keeping all bowls. Furniture supports, the room window,
floor tracks and the broom remain active.

In the hearts scene, jump through seven rows of whole and broken hearts to
reach the female cat at the top. A rival on the starting row is initially hidden
and appears after a difficulty-dependent delay. Other cats and arrows knock the player down
without removing lives. Arrows toggle crossed cells between whole and broken;
the original support geometry, including edge cells, is retained. Placed gifts
temporarily distract other cats. Each completed ordinary room supplies another
gift, up to eight, and unsuccessful hearts attempts preserve that count.

Success plays the original approach and pairing animations, score/multiplier
display and return presentation. Its award combines the external base and a
time bonus with a difficulty multiplier; arriving with a carried gift doubles
the multiplier. It then adds a life, increases difficulty up to seven, resets
the ordinary-room completion count and returns to the alley. The updated
difficulty selects new bin and ground-object tables. Falling out of the hearts
scene instead plays its neutral return animation, preserving score, lives,
difficulty and the gift count. Both paths return at the saved window position.

Scoring cat faces come from the executable. The original award lettering uses
a BIOS ROM font absent from that file. The recreation uses SDL's 8×8 glyphs for
award digits and multiplier text; all characters used there match the captured
DOSBox-X CGA/PCjr BIOS reference. This does not establish equality with every
historical BIOS font. Font attribution is in [THIRD_PARTY.md](THIRD_PARTY.md).
The black and color wipes follow the original expanding geometry; their execution
costs are provisional. Scene-specific CGA/PCjr palette tables,
carried-gift background changes and completion flashes are applied separately
from packed framebuffer pixels. Audio remains silent.

The player starts settled on the floor, bypassing the original entry animation.
Falling behind the fence hides the cat and starts a clipped re-entry from the
nearer alley edge. Its speaker effect is silent. The timed runner blocks game
work until seven BIOS ticks have elapsed and advances the effect's shared random
state through its original call-count algorithm, using approximate loop costs.

The splash uses a separate copy of the executable data. Watching it does not
advance the fixed gameplay seed; shared startup/menu state integration remains
deferred. Startup menus and audio output are absent. Windowed execution uses a
virtual BIOS/PIT clock, two-tick control
polling, the original 32/16-call player countdown, and a retrace gate for idle
movement. The reference is DOSBox-X normal CPU core at fixed 240 cycles. Loop
costs are fitted approximations; original-PC timing fidelity is not established.
Room and hearts execution costs are provisional and have not been independently
calibrated against the fixed-240 reference.
Rendering refresh does not determine game ticks. See
[timing contracts](docs/timing.md) for the schedule and its limits.

With `dev-tools`, `--step-hz 36.413` explicitly selects the earlier evenly spaced diagnostic
accepted-update mode. It bypasses the original player timing gates.

## Headless development use

Headless diagnostics require `dev-tools` and need no SDL library:

```sh
cargo run --locked --no-default-features --features dev-tools -- \
  --original /path/to/CAT.EXE --headless --steps 60
```

`--script /path/to/controls.csv` supplies one accepted update per row, with
columns `x,y` and an optional third `action` column. Directions are -1, 0, or 1:
negative is left/up, positive is right/down. Action is 0 (released) or 1 (Alt
held); omitting it selects 0. Blank lines, `#` comments, and an optional
`x,y` or `x,y,action` header are accepted.
Without `--steps`, a script runs for its row count; additional requested updates
use neutral controls.

Add `--timed` to execute virtual loop attempts, input polls and rejected player
calls without wall-clock waits. Timed headless runs stop early at game over. Script rows still select held directions by
accepted-update index; they are not timestamped hardware input recordings.
`--trace-timing /path/to/timing.csv` records guest times, BIOS/PIT samples,
retrace, polling, acceptance, actor-divider events, scene changes and blocking waits.
It also records post-pass ordinary-room history, random state, difficulty and pending
hearts entry; collecting these values does not alter selection or consume randomness.
In headless runs it requires `--timed`; in windowed runs it uses default timing.

`--trace`, `--dump-cga`, and `--dump-ppm` write player-state CSV including the
original scene number (0 for alley, 1 for fishbowl, 2 for underwater, 3 for bookcase,
4 for cheese, 5 for birdcage, 6 for milk, 7 for hearts), the final
16384-byte packed framebuffer, and a 320×200 PPM image respectively. Use paths
outside the repository: these outputs contain original graphics or reference
evidence. Output files must not already exist. See `--help` for all options.

## Development and licensing

See [CONTRIBUTING.md](CONTRIBUTING.md) for validation, formatting, and commit
conventions, and [the analysis tool](tools/README.md) for read-only executable
reconnaissance. Development does not require a particular parent workspace.

The project's license has not been selected; no project license is granted yet.
Dependency attribution and license notices are in
[THIRD_PARTY.md](THIRD_PARTY.md). Those notices do not grant rights to the
original game data.
