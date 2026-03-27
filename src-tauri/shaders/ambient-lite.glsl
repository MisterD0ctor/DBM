//!HOOK POSTKERNEL        
//!BIND HOOKED        
//!DESC ambient-light-glow
// This shader is now hooked to the *canvas* rather than just the video
// plane.  the hook function will therefore be executed for every pixel
// in the viewport, including the black letter‑box bars that mpv draws
// around a nondistorted image.

// iResolution is provided by the mpv glsl-shader environment when the
// canvas hook is active.  it contains the full drawable size (window)
// so we can produce gradients, edge‑glows, etc.  you can remove it if
// you just want a constant colour.
// uniform vec2 iResolution;

vec4 hook() {
    // compute a very simple radial gradient centred on the screen; the
    // result clearly shows the shader affecting the whole canvas (bars
    // included).
    // vec2 uv = gl_FragCoord.xy / iResolution;
    // float d = distance(uv, vec2(0.5));
    // float glow = smoothstep(0.4, 0.5, d);
    // vec3 colour = mix(vec3(1.0), vec3(0.2, 0.6, 1.0), glow);
    return vec4(1.0, 1.0, 1.0, 1.0);
}