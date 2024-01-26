uniform sampler2D tex;
uniform sampler2D ltex;
uniform vec2 ltexreg; // lookup texture region normalization vector

in  vec2  f_uv;
in  vec4  f_color;
in  float f_opacity;

out vec4  fragColor;

vec3 linearTosRGB(vec3 linear) {
    vec3 lower = linear * 12.92;
    vec3 higher = 1.055 * pow(linear, vec3(1.0 / 2.4)) - 0.055;

    // Use smoothstep for a smoother transition
    vec3 transition = smoothstep(vec3(0.0031308 - 0.00001), vec3(0.0031308 + 0.00001), linear);
    
    return mix(lower, higher, transition);
}

void main() {
    vec4 texel = texture(tex, f_uv);
    texel = vec4(linearTosRGB(texel.rgb), texel.a); // Convert to linear space
    texel.rg = texel.rg * ltexreg;
    if (texel.a > 0.0) { // Non-transparent pixel
        vec4 lt_texel = texture(ltex, texel.rg);
        fragColor = vec4(
            mix(lt_texel.rgb, f_color.rgb, f_color.a),
            lt_texel.a * f_opacity
        );
    } else {
        fragColor = vec4(
            mix(texel.rgb, f_color.rgb, f_color.a),
            texel.a * f_opacity
        );
    }
}
