//!HOOK MAIN
//!BIND HOOKED
//!DESC RustyFin mild sharpening
vec4 hook() {
    vec4 center = HOOKED_tex(HOOKED_pos);
    vec3 neighbours = HOOKED_texOff(vec2(1, 0)).rgb + HOOKED_texOff(vec2(-1, 0)).rgb
                    + HOOKED_texOff(vec2(0, 1)).rgb + HOOKED_texOff(vec2(0, -1)).rgb;
    return vec4(max(center.rgb + 0.24 * (center.rgb - 0.25 * neighbours), vec3(0.0)), center.a);
}
