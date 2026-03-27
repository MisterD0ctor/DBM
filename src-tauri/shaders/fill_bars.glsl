// mpv-specific shader syntax
//!HOOK OVERLAY
//!DESC Ambient background fill
//!BIND HOOKED

vec4 hook() {
    vec2 uv = HOOKED_pos;
    // If uv is outside the standard 0.0-1.0 range (the bars),
    // sample the nearest edge pixel of the video.
    vec2 edge_uv = clamp(uv, 0.0, 1.0);
    vec4 color = HOOKED_tex(edge_uv);
    
    // Apply a blur or darkening effect here to the "fill" area
    return color * 0.5; 
}