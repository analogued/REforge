# Bundled Hyllian CRT shader

`hyllian.frag` is an isolated MIT-licensed adaptation of Hyllian's CRT shader,
ported to DOSBox by Tyrells and maintained by the DOSBox Staging team. The
SPIR-V in `hyllian.spv` and Metal source in `hyllian.msl` are generated from it.
They are embedded by REforge; building and running REforge needs no shader
compiler, Qt, DOSBox Staging installation or shader files alongside the executable.
Retain [the MIT license and attribution](LICENSE-MIT.txt) when redistributing
these shaders. MIT permits use with the project's candidate permissive or GPL
licenses; it does not select a license for the rest of REforge.

## Provenance and adaptations

The source is
[`resources/shaders/crt/crt-hyllian.glsl`](https://github.com/dosbox-staging/dosbox-staging/blob/7b40053b7ac580843d0461eba8c36a47a990e66c/resources/shaders/crt/crt-hyllian.glsl)
at DOSBox Staging v0.83.0, immutable commit
`7b40053b7ac580843d0461eba8c36a47a990e66c`.
It matches the installed v0.83.0 shader and release archive byte for byte.
Source SHA-256:
`148fd9f65321bc7ef63ebff4d7e283ad0128bf584debd6ce5cc4bd3fa5233c72`.
The initial settings derive from the same revision's
[`cga-1080p.preset`](https://github.com/dosbox-staging/dosbox-staging/blob/7b40053b7ac580843d0461eba8c36a47a990e66c/resources/shader-presets/crt/crt-hyllian/cga-1080p.preset),
SHA-256 `25ef3b7ad4bca83f30eeca07f3aa4375be7f03e319d974fcc5b2079a7fc5c0ee`.
The upstream notices, including their unusual year ranges, are preserved.

Local adaptations replace the OpenGL/DOSBox vertex interface and parameter
uniforms with the SDL GPU Renderer fragment interface, a size/viewport uniform
block, and fixed parameters. The horizontal cubic filter, anti-ringing,
brightness-dependent Gaussian beams and gamma calculations preserve the upstream
algorithm. Only the selected aperture mask branch is retained; the other mask
layouts and the DOSBox vertex stage are omitted. The selected mask is hunterk's
public-domain RGB aperture implementation, included by the upstream shader.
Mask coordinates use physical viewport-local pixels. SDL vertex colour modulates
the result. These shaders do not apply DOSBox Staging's separate colour profiles.

The fixed parameters are beam profile 0, Hermite horizontal filter 0, beam widths
0.70/1.00, scanline strength 0.65, colour boost 2.00, sharpness 1.00, mask intensity
0.35, anti-ringing 1.00, input/output gamma 2.40/2.40, and horizontal scanlines.
Phosphor layout **1** selects the RGB aperture mask; the Staging CGA preset uses
layout 4 for RBG panels. Equal input/output gamma retains Hyllian's beam response
without introducing a separate colour-profile conversion. The source texture
retains its native height (200 rows for current games).

## SDL interface

The input locations match SDL 3.4's bundled
[`tri_texture.vert` shader](https://github.com/libsdl-org/SDL/blob/release-3.4.0/src/render/gpu/shaders/tri_texture.vert.hlsl)
and its SPIR-V output decorations: location 0 is `vec4` vertex colour, location 1
is `vec2` texture coordinates. Use a nearest-filtered, clamp-to-edge input sampler;
the shader performs its own reconstruction.

| Resource | Vulkan/SPIR-V | Metal |
| --- | --- | --- |
| Source texture and sampler | Set 2, binding 0 | Texture 0, sampler 0 |
| Fragment uniform block | Set 3, binding 0 | Buffer 0 |
| Fragment entry point | `main` | `main0` |

The uniform block is 32 bytes, in two four-component 32-bit float vectors:

| Offset | Member | Values |
| --- | --- | --- |
| 0 | `source_size` | Native texture width, height, 0, 0 |
| 16 | `viewport` | Drawable-pixel x, y, width, height |

The viewport is the actual game image rectangle after resizing, aspect correction,
letterboxing and high-DPI scaling. Its width/height are reserved in the interface;
the current shader uses x/y to anchor the mask to that rectangle.

## Offline regeneration

From the repository root:

```sh
QSB=/usr/lib/qt6/bin/qsb sh crates/reforge_sdl/shaders/regenerate.sh
```

`QSB` can point to another Qt Shader Baker installation; its default is `qsb`
on `PATH`. The bundled assets were generated with `glslangValidator` 16.6.0,
Qt Shader Baker 6.11.2 (its bundled SPIRV-Cross translator), and `spirv-val` from
SPIRV-Tools 1.4.363.0. The script's exact compiler commands are:

```sh
glslangValidator -V --target-env vulkan1.0 -S frag -o hyllian.spv hyllian.frag
spirv-val --target-env vulkan1.0 hyllian.spv
qsb --msl 12 -o hyllian.qsb hyllian.frag
qsb -x msl,12 -o hyllian.raw.msl hyllian.qsb
```

The script prefixes the extracted Metal source with the source notices; temporary
QSB containers are not shipped. Generated Metal resource bindings are already
buffer/texture/sampler 0 and require no manual remapping. Inspect reflection with
`qsb -d hyllian.qsb` and SPIR-V with `spirv-dis hyllian.spv` when changing the
interface. Regenerate both assets together after source changes. Native Metal
compilation requires macOS; Linux can validate SPIR-V and inspect generated Metal
bindings but cannot validate an Apple driver.
