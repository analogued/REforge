#version 450

// SPDX-FileCopyrightText:  2020-2026 The DOSBox Staging Team
// SPDX-FileCopyrightText:  2021-2020 Hyllian <sergiogdb@gmail.com>
// SPDX-FileCopyrightText:  2020-2020 Tyrells
// SPDX-License-Identifier: MIT
//
// Hyllian's CRT Shader, ported from Libretro by Tyrells, then adapted
// from DOSBox Staging v0.83.0 for SDL 3.4's GPU Renderer in REforge.
// See README.md for provenance, adaptations, parameters and regeneration.
// The original beam, cubic reconstruction, anti-ringing and mask code follows.

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec2 v_texCoord;
layout(location = 0) out vec4 FragColor;

layout(set = 2, binding = 0) uniform sampler2D INPUT_TEXTURE_0;
layout(std140, set = 3, binding = 0) uniform HyllianUniforms {
    vec4 source_size; // Native texture width/height; remaining components unused.
    vec4 viewport;    // Physical drawable x/y/width/height, before this draw.
    vec4 beam;        // Strength, minimum/maximum width, colour boost.
    vec4 filter_mask; // Sharpness, mask intensity, anti-ringing, horizontal filter.
    vec4 gamma;       // Input/output gamma, reserved, reserved.
};

#define INPUT_SIZE_0 source_size.xy
#define BEAM_PROFILE 0.0
#define HFILTER_PROFILE filter_mask.w
#define BEAM_MIN_WIDTH beam.y
#define BEAM_MAX_WIDTH beam.z
#define SCANLINES_STRENGTH beam.x
#define COLOR_BOOST beam.w
#define SHARPNESS_HACK filter_mask.x
#define PHOSPHOR_LAYOUT 1.0
#define MASK_INTENSITY filter_mask.y
#define CRT_ANTI_RINGING filter_mask.z
#define INPUT_GAMMA gamma.x
#define OUTPUT_GAMMA gamma.y
#define VSCANLINES 0.0

#define GAMMA_IN(color)   pow(color, vec4(INPUT_GAMMA, INPUT_GAMMA, INPUT_GAMMA, INPUT_GAMMA))
#define GAMMA_OUT(color)  pow(color, vec4(1.0 / OUTPUT_GAMMA, 1.0 / OUTPUT_GAMMA, 1.0 / OUTPUT_GAMMA, 1.0 / OUTPUT_GAMMA))

// Aperture mask by hunterk, public domain, as included in the MIT upstream.
// Fixed PHOSPHOR_LAYOUT=1: retain its RGB aperture math; omit unused layouts.
vec3 mask_weights(vec2 coord, float mask_intensity, int phosphor_layout)
{
    float on = 1.;
    float off = 1.-mask_intensity;

    vec3 green   = vec3(off, on,  off);
    vec3 magenta = vec3(on,  off, on );

    // Classic aperture for RGB panels; good for 1080p, too small for 4K+.
    // aka aperture_1_2_bgr
    return mix(magenta, green, floor(mod(coord.x, 2.0)));
}

// Horizontal cubic filter.
//
// Some known filters use these values:
//
//   B = 0.0, C = 0.0  =>  Hermite cubic filter.
//   B = 1.0, C = 0.0  =>  Cubic B-Spline filter.
//   B = 0.0, C = 0.5  =>  Catmull-Rom Spline filter. This is the default used in this shader.
//   B = C = 1.0/3.0   =>  Mitchell-Netravali cubic filter.
//   B = 0.3782, C = 0.3109  =>  Robidoux filter.
//   B = 0.2620, C = 0.3690  =>  Robidoux Sharp filter.

// Using only Hermite and Catmull-Rom, as the others aren't useful for crt shader.
// For more info, see: http://www.imagemagick.org/Usage/img_diagrams/cubic_survey.gif
mat4x4 get_hfilter_profile()
{
    float bf = 0.0;
    float cf = 0.0;

    if (HFILTER_PROFILE == 1) {
        bf = 0.0;
        cf = 0.5;
    }

    return mat4(
                   (-bf - 6.0*cf)/6.0,          (3.0*bf + 12.0*cf)/6.0, (-3.0*bf - 6.0*cf)/6.0,             bf/6.0,
         (12.0 - 9.0*bf - 6.0*cf)/6.0, (-18.0 + 12.0*bf  + 6.0*cf)/6.0,                    0.0, (6.0 - 2.0*bf)/6.0,
        -(12.0 - 9.0*bf - 6.0*cf)/6.0,  (18.0 - 15.0*bf - 12.0*cf)/6.0,  (3.0*bf + 6.0*cf)/6.0,             bf/6.0,
                    (bf + 6.0*cf)/6.0,                        -cf,                         0.0,                0.0
    );
}

#define scanlines_strength (4.0 * profile.x)
#define beam_min_width     profile.y
#define beam_max_width     profile.z
#define color_boost        profile.w

vec4 get_beam_profile()
{
	vec4 bp = vec4(SCANLINES_STRENGTH, BEAM_MIN_WIDTH, BEAM_MAX_WIDTH, COLOR_BOOST);

	if (BEAM_PROFILE == 1.0) {
		bp = vec4(0.62, 1.00, 1.00, 1.40); // Catmull-rom
	}
	if (BEAM_PROFILE == 2.0) {
		bp = vec4(0.72, 1.00, 1.00, 1.20); // Catmull-rom
	}

	return bp;
}

void main()
{
	vec4 profile = get_beam_profile();

	vec2 TextureSize =
	        mix(vec2(INPUT_SIZE_0.x * SHARPNESS_HACK, INPUT_SIZE_0.y),
	            vec2(INPUT_SIZE_0.x, INPUT_SIZE_0.y * SHARPNESS_HACK),
	            VSCANLINES);

	vec2 dx = mix(vec2(1.0 / TextureSize.x, 0.0),
	              vec2(0.0, 1.0 / TextureSize.y),
	              VSCANLINES);

	vec2 dy = mix(vec2(0.0, 1.0 / TextureSize.y),
	              vec2(1.0 / TextureSize.x, 0.0),
	              VSCANLINES);

	vec2 pix_coord = v_texCoord.xy * TextureSize + vec2(-0.5, 0.5);

	vec2 tc = mix((floor(pix_coord) + vec2(0.5, 0.5)) / TextureSize,
	              (floor(pix_coord) + vec2(1.0, -0.5)) / TextureSize,
	              VSCANLINES);

	vec2 fp = mix(fract(pix_coord), fract(pix_coord.yx), VSCANLINES);

	vec4 c00 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc - dx - dy).xyzw);
	vec4 c01 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc - dy).xyzw);
	vec4 c02 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc + dx - dy).xyzw);
	vec4 c03 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc + 2.0 * dx - dy).xyzw);

	vec4 c10 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc - dx).xyzw);
	vec4 c11 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc).xyzw);
	vec4 c12 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc + dx).xyzw);
	vec4 c13 = GAMMA_IN(texture(INPUT_TEXTURE_0, tc + 2.0 * dx).xyzw);

	mat4 invX = get_hfilter_profile();

	mat4 color_matrix0 = mat4(c00, c01, c02, c03);
	mat4 color_matrix1 = mat4(c10, c11, c12, c13);

	vec4 invX_Px = vec4(fp.x * fp.x * fp.x, fp.x * fp.x, fp.x, 1.0) * invX;
	vec4 color0  = color_matrix0 * invX_Px;
	vec4 color1  = color_matrix1 * invX_Px;

	//  Get min/max samples
	vec4 min_sample0 = min(c01, c02);
	vec4 max_sample0 = max(c01, c02);
	vec4 min_sample1 = min(c11, c12);
	vec4 max_sample1 = max(c11, c12);

	// Anti-ringing
	vec4 aux = color0;
	color0   = clamp(color0, min_sample0, max_sample0);
	color0   = mix(aux,
                     color0,
                     CRT_ANTI_RINGING * step(0.0, (c00 - c01) * (c02 - c03)));

	aux      = color1;
	color1   = clamp(color1, min_sample1, max_sample1);
	color1   = mix(aux,
                     color1,
                     CRT_ANTI_RINGING * step(0.0, (c10 - c11) * (c12 - c13)));

	float pos0 = fp.y;
	float pos1 = 1.0 - fp.y;

	vec4 lum0 = mix(vec4(beam_min_width), vec4(beam_max_width), color0);
	vec4 lum1 = mix(vec4(beam_min_width), vec4(beam_max_width), color1);

	vec4 d0 = scanlines_strength * pos0 / (lum0 + 0.0000001);
	vec4 d1 = scanlines_strength * pos1 / (lum1 + 0.0000001);

	d0 = exp(-d0 * d0);
	d1 = exp(-d1 * d1);

	vec4 color = color_boost * (color0 * d0 + color1 * d1);

	// Mask
	vec2 mask_coords = gl_FragCoord.xy - viewport.xy;
	mask_coords      = mix(mask_coords.xy, mask_coords.yx, VSCANLINES);
	color.rgb *= mask_weights(mask_coords, MASK_INTENSITY, int(PHOSPHOR_LAYOUT));

	// Output gamma
	color = clamp(GAMMA_OUT(color), 0.0, 1.0);

	FragColor = vec4(color.rgb, 1.0) * v_color;
}
