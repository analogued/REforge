# Bundled Hyllian CRT shader

`hyllian.frag` is an isolated MIT-licensed adaptation of Hyllian's CRT shader,
ported to DOSBox by Tyrells and maintained by the DOSBox Staging team. The
SPIR-V in `hyllian.spv`, Metal source in `hyllian.msl`, and Direct3D 12 DXIL in
`hyllian.dxil` are generated from it. `hyllian.hlsl` is the generated DXIL input,
retained for inspection; change `hyllian.frag` and regenerate all targets.
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

The input locations match SDL 3.4's bundled `tri_texture.vert` shader:
location 0 is `vec4` vertex colour, location 1 is `vec2` texture coordinates.
The shipped
[`tri_texture.vert.dxil.h`](https://github.com/libsdl-org/SDL/blob/release-3.4.0/src/render/gpu/shaders/tri_texture.vert.dxil.h)
outputs those as `TEXCOORD0` and `TEXCOORD1`, respectively, plus `SV_Position`.
Use those compiled semantics when linking the fragment shader: the adjacent
upstream HLSL source has different semantic names. The shipped DXIL interface is
also verified at SDL 3.4.16. Use a nearest-filtered,
clamp-to-edge input sampler; the shader performs its own reconstruction.

| Resource | Vulkan/SPIR-V | Metal | D3D12/DXIL |
| --- | --- | --- | --- |
| Source texture and sampler | Set 2, binding 0 | Texture 0, sampler 0 | `t0, space2`; `s0, space2` |
| Fragment uniform block | Set 3, binding 0 | Buffer 0 | `b0, space3` |
| Fragment entry point | `main` | `main0` | `main` (`ps_6_0`) |

Bindings follow [SDL_CreateGPUShader's format conventions](https://wiki.libsdl.org/SDL3/SDL_CreateGPUShader).

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
QSB=/usr/lib/qt6/bin/qsb DXC=/path/to/dxc sh crates/reforge_sdl/shaders/regenerate.sh
```

`QSB` and `DXC` can point to other compiler installations; their defaults are
`qsb` and `dxc` on `PATH`. The bundled assets were generated with `glslangValidator` 16.6.0,
Qt Shader Baker 6.11.2 (its bundled SPIRV-Cross translator), and `spirv-val` from
SPIRV-Tools 1.4.363.0. DXIL uses Microsoft's official
[DXC v1.8.2505.1 release](https://github.com/microsoft/DirectXShaderCompiler/releases/tag/v1.8.2505.1),
Linux x86-64 archive `linux_dxc_2025_07_14.x86_64.tar.gz`, SHA-256
`f2213da1fc99dc8778c8823078e16ba97c7f80f86a1d4520ab1adf4b462bc48c`.
Its version output is `libdxcompiler.so: 1.9(dev;4950-b106a961); libdxil.so: 1.9`.
Extract the archive into a developer-owned directory and use its `bin/dxc`;
keep the adjacent `lib/` directory. No system installation is needed.
The script's compiler commands are:

```sh
glslangValidator -V --target-env vulkan1.0 -S frag -o hyllian.spv hyllian.frag
spirv-val --target-env vulkan1.0 hyllian.spv
qsb --msl 12 --hlsl 51 -o hyllian.qsb hyllian.frag
qsb -x msl,12 -o hyllian.raw.msl hyllian.qsb
qsb -x hlsl,51 -o hyllian.raw.hlsl hyllian.qsb
dxc -T ps_6_0 -E main -HV 2021 -O3 -WX -Fo hyllian.dxil hyllian.hlsl
```

The script prefixes extracted Metal and HLSL source with the source notices;
temporary QSB containers are not shipped. Generated Metal resource bindings are
already buffer/texture/sampler 0. Qt emits HLSL resources in `space0`; the script
checks and remaps only their register spaces to SDL's fragment conventions.
HLSL inputs already match SDL's shipped DXIL vertex semantics. Shader calculations
come from the same GLSL source through Qt's translator. DXC's normal compilation
validates DXIL; validation is never disabled. The script replaces bundled assets
only after all compilers succeed.

Inspect reflection with `qsb -d hyllian.qsb`, SPIR-V with
`spirv-dis hyllian.spv`, and DXIL with `dxc -dumpbin hyllian.dxil` when changing
the interface. DXIL reflection must show `TEXCOORD0`/`TEXCOORD1`, `SV_Position`,
`SV_Target0`, the three resource bindings above, and a 32-byte constant buffer
with members at offsets 0 and 16. Regeneration with the recorded tool versions
produces identical bundled bytes across runs. Native Metal compilation requires
macOS; D3D12 rendering requires Windows. Linux validates the compiled DXIL and
SPIR-V and their interfaces, but does not validate native Metal or D3D12 rendering.

## Compiler licenses

The generated shader assets remain the compiled/translated form of this
MIT-licensed adaptation; retain `LICENSE-MIT.txt` and its notices. Compiler
licenses are separate from shader licensing. Qt Shader Baker is a developer tool
under Qt's [commercial/GPLv3 terms](https://doc.qt.io/qt-6/qtshadertools-index.html#licenses-and-attributions),
with its SPIRV-Cross translator under Apache-2.0 (and bundled MIT Khronos
definitions). The glslang compiler has
[multiple source licenses](https://github.com/KhronosGroup/glslang/blob/16.6.0/LICENSE.txt),
including BSD, MIT and Apache terms; SPIRV-Tools is
[Apache-2.0](https://github.com/KhronosGroup/SPIRV-Tools/blob/main/LICENSE).
DXC is under the
[University of Illinois/NCSA license and third-party notices](https://github.com/microsoft/DirectXShaderCompiler/blob/v1.8.2505.1/LICENSE.TXT);
the downloaded release's `ReleaseNotes.md` assigns `LICENSE-LLVM.txt` to its
compiler files. None of these compilers, their libraries or their QSB containers
are embedded or distributed with REforge, and none is needed for ordinary builds
or playback.
