//!HOOK MAIN
//!BIND HOOKED
//!DESC RustyFin mild smoothing
vec4 hook() {
    vec4 center = HOOKED_tex(HOOKED_pos);
    vec3 neighbours = HOOKED_texOff(vec2(1, 0)).rgb + HOOKED_texOff(vec2(-1, 0)).rgb
                    + HOOKED_texOff(vec2(0, 1)).rgb + HOOKED_texOff(vec2(0, -1)).rgb;
    return vec4(0.75 * center.rgb + 0.0625 * neighbours, center.a);
}
