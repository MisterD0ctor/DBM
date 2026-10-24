//! The backdrop blur: a Gaussian of any width for about the cost of a small
//! one.
//!
//! Up to σ 6 it runs at full size, as a separable Gaussian. Past that the
//! image is first averaged down a level at a time, halving until what is left
//! to blur is under σ 6, blurred there, and drawn back at full size, so a σ 100
//! blur costs about what a σ 6 one does and is still a Gaussian rather than a
//! kernel cut short. The method of Blender's compositor (Pranckevičius, "Fast
//! blur with animated radius", 2026; Blender PR #164496, GPL-2.0-or-later),
//! itself close to Skia's: a one-pixel border on each level that keeps the
//! picture's own edge, the softening that averaging down and drawing back add
//! taken out of the Gaussian so the levels meet without a step, a kernel
//! tapered to nothing over its last deviation, and a cubic B-spline drawing
//! it back past twice the size.

use crate::gpu::gfx::{bind_texture, Program, ScreenQuad, Target};

/// The widest Gaussian run at any one level. Past it, the blur moves down a
/// level, so a blur up to this stays at full size, exactly as sharp.
const MAX_WORKING_SIGMA: f32 = 6.0;

/// Taps either side of the centre, two texels each: 24 texels, four
/// deviations at `MAX_WORKING_SIGMA`. Must match the arrays in
/// blur_gauss.frag.
pub const MAX_TAP_PAIRS: usize = 12;

/// How far the kernel reaches, in deviations: a margin that holds this much
/// round a region holds all of its blur.
pub const REACH: f32 = 4.0;

pub struct Blur {
    down: Program,
    gauss: Program,
    up: Program,
    /// The levels below full size, finest first, each with its border.
    levels: Vec<Target>,
    /// The horizontal pass, at the working level.
    across: Target,
    /// Both passes, at the working level, when that is not full size.
    filtered: Target,
}

/// An image the blur reads: its target, and the image's size inside the
/// border along with the border.
#[derive(Clone, Copy)]
struct Level<'a> {
    target: &'a Target,
    size: (u32, u32),
    pad: (u32, u32),
}

impl Blur {
    pub fn new(gl: &glow::Context, vert: &str) -> Result<Self, String> {
        let program =
            |name: &str, src: &str| Program::new(gl, vert, src).map_err(|e| format!("{name}: {e}"));
        Ok(Self {
            down: program("blur_down", include_str!("../../shaders/blur_down.frag"))?,
            gauss: program("blur_gauss", include_str!("../../shaders/blur_gauss.frag"))?,
            up: program("blur_up", include_str!("../../shaders/blur_up.frag"))?,
            levels: Vec::new(),
            across: Target::new(),
            filtered: Target::new(),
        })
    }

    /// The horizontal pass's target, for the diagnostic that probes each
    /// stage.
    pub fn across(&self) -> &Target {
        &self.across
    }

    /// Blur `src` by `sigma` pixels into `dst`, which must already be its
    /// size. A sigma of 0 is an exact copy.
    pub fn run(
        &mut self,
        gl: &glow::Context,
        quad: &ScreenQuad,
        src: &Target,
        dst: &Target,
        sigma: f32,
    ) {
        let size = src.size();
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let sigma = sigma.max(0.0);
        let target = (working_level(sigma, size.0), working_level(sigma, size.1));

        // Down both axes until one reaches its level, then the other alone.
        let steps = target.0.max(target.1) as usize;
        while self.levels.len() < steps {
            self.levels.push(Target::new());
        }
        let mut at = (0, 0);
        for step in 0..steps {
            let next = ((at.0 + 1).min(target.0), (at.1 + 1).min(target.1));
            let out_size = (downsampled(size.0, next.0), downsampled(size.1, next.1));
            let out_pad = (u32::from(next.0 > 0), u32::from(next.1 > 0));
            let (done, rest) = self.levels.split_at_mut(step);
            let out = &mut rest[0];
            if !out.ensure_size(gl, out_size.0 + 2 * out_pad.0, out_size.1 + 2 * out_pad.1) {
                return;
            }
            let input = match done.last() {
                Some(previous) => Level {
                    target: previous,
                    size: (downsampled(size.0, at.0), downsampled(size.1, at.1)),
                    pad: (u32::from(at.0 > 0), u32::from(at.1 > 0)),
                },
                None => Level {
                    target: src,
                    size,
                    pad: (0, 0),
                },
            };
            draw_down(gl, quad, &self.down, input, out, (out_size, out_pad));
            at = next;
        }
        let working = match self.levels[..steps].last() {
            Some(level) => Level {
                target: level,
                size: (downsampled(size.0, at.0), downsampled(size.1, at.1)),
                pad: (u32::from(at.0 > 0), u32::from(at.1 > 0)),
            },
            None => Level {
                target: src,
                size,
                pad: (0, 0),
            },
        };

        let (w, h) = working.target.size();
        if !self.across.ensure_size(gl, w, h) {
            return;
        }
        let kernel_x = Kernel::new(working_sigma(sigma, working.size.0, size.0));
        let kernel_y = Kernel::new(working_sigma(sigma, working.size.1, size.1));
        let gauss = |src: &Target, dst: &Target, dir, kernel: &Kernel| {
            draw_gauss(gl, quad, &self.gauss, src, dst, dir, kernel);
        };
        gauss(working.target, &self.across, (1.0, 0.0), &kernel_x);
        if steps == 0 {
            gauss(&self.across, dst, (0.0, 1.0), &kernel_y);
            return;
        }
        if !self.filtered.ensure_size(gl, w, h) {
            return;
        }
        gauss(&self.across, &self.filtered, (0.0, 1.0), &kernel_y);
        let filtered = Level {
            target: &self.filtered,
            ..working
        };
        draw_up(gl, quad, &self.up, filtered, dst);
    }

    pub fn release(&mut self, gl: &glow::Context) {
        for target in self
            .levels
            .iter_mut()
            .chain([&mut self.across, &mut self.filtered])
        {
            target.release(gl);
        }
        self.levels.clear();
        for p in [&self.down, &self.gauss, &self.up] {
            p.release(gl);
        }
    }
}

/// `out` is the level drawn: its image's size inside the border, and the
/// border.
fn draw_down(
    gl: &glow::Context,
    quad: &ScreenQuad,
    p: &Program,
    input: Level,
    out: &Target,
    (out_size, out_pad): ((u32, u32), (u32, u32)),
) {
    p.bind(gl);
    set_alloc(gl, p, input.target);
    p.set_vec2(gl, "u_src_size", input.size.0 as f32, input.size.1 as f32);
    p.set_vec2(gl, "u_src_pad", input.pad.0 as f32, input.pad.1 as f32);
    p.set_vec2(gl, "u_dst_size", out_size.0 as f32, out_size.1 as f32);
    p.set_vec2(gl, "u_dst_pad", out_pad.0 as f32, out_pad.1 as f32);
    bind_texture(gl, p, "u_src", 0, input.target.texture());
    quad.draw_to(gl, out);
}

fn set_alloc(gl: &glow::Context, p: &Program, target: &Target) {
    let alloc = target.alloc();
    p.set_vec2(gl, "u_alloc", alloc.0.max(1) as f32, alloc.1.max(1) as f32);
}

fn draw_gauss(
    gl: &glow::Context,
    quad: &ScreenQuad,
    p: &Program,
    src: &Target,
    dst: &Target,
    dir: (f32, f32),
    kernel: &Kernel,
) {
    let (w, h) = src.size();
    p.bind(gl);
    set_alloc(gl, p, src);
    p.set_vec2(gl, "u_size", w as f32, h as f32);
    p.set_vec2(gl, "u_dir", dir.0, dir.1);
    p.set_f32(gl, "u_center", kernel.center);
    p.set_i32(gl, "u_taps", kernel.taps as i32);
    p.set_f32_array(gl, "u_weight[0]", &kernel.weights);
    p.set_f32_array(gl, "u_offset[0]", &kernel.offsets);
    bind_texture(gl, p, "u_src", 0, src.texture());
    quad.draw_to(gl, dst);
}

fn draw_up(gl: &glow::Context, quad: &ScreenQuad, p: &Program, src: Level, dst: &Target) {
    let (w, h) = dst.size();
    p.bind(gl);
    set_alloc(gl, p, src.target);
    p.set_vec2(gl, "u_src_size", src.size.0 as f32, src.size.1 as f32);
    p.set_vec2(gl, "u_src_pad", src.pad.0 as f32, src.pad.1 as f32);
    p.set_vec2(gl, "u_dst_size", w as f32, h as f32);
    bind_texture(gl, p, "u_src", 0, src.target.texture());
    quad.draw_to(gl, dst);
}

/// One axis of the Gaussian, as blur_gauss.frag takes it: the centre's
/// weight, and each tap's weight and offset in texels, normalised to sum to 1.
#[derive(Debug)]
struct Kernel {
    center: f32,
    taps: usize,
    weights: [f32; MAX_TAP_PAIRS],
    offsets: [f32; MAX_TAP_PAIRS],
}

impl Kernel {
    fn new(sigma: f32) -> Self {
        let mut kernel = Self {
            center: 1.0,
            taps: 0,
            weights: [0.0; MAX_TAP_PAIRS],
            offsets: [0.0; MAX_TAP_PAIRS],
        };
        if sigma < 0.01 {
            return kernel;
        }
        // Out to four deviations, easing to zero over the last, so a bright
        // light's blur doesn't stop at a visible edge and a changing sigma
        // doesn't step as taps come and go.
        let weight = |i: f32| {
            let x = i / sigma;
            let t = (REACH - x).clamp(0.0, 1.0);
            (-0.5 * x * x).exp() * t * t * (3.0 - 2.0 * t)
        };
        let pairs = ((2.0 * sigma).ceil() as usize).min(MAX_TAP_PAIRS);
        let mut sum = 1.0;
        for j in 0..pairs {
            let i = (2 * j + 1) as f32;
            let (a, b) = (weight(i), weight(i + 1.0));
            let w = a + b;
            kernel.weights[j] = w;
            kernel.offsets[j] = if w > 0.0 { i + b / w } else { i };
            sum += 2.0 * w;
            if w > 0.0 {
                kernel.taps = j + 1;
            }
        }
        kernel.center = 1.0 / sum;
        for w in &mut kernel.weights {
            *w /= sum;
        }
        kernel
    }
}

/// A size halved `level` times, rounding up, so an odd size keeps its last
/// texel.
fn downsampled(size: u32, level: u32) -> u32 {
    size.div_ceil(1 << level)
}

/// The finest level at which what is left of the blur is under
/// `MAX_WORKING_SIGMA`. With sizes rounded up, 5 to 3 is a 3/5 scale, not 1/2.
fn working_level(sigma: f32, size: u32) -> u32 {
    let mut level = 0;
    while downsampled(size, level) > 1
        && sigma >= MAX_WORKING_SIGMA * (size as f32 / downsampled(size, level) as f32)
    {
        level += 1;
    }
    level
}

/// The softening drawing back from `size` to `out` adds, as a variance in
/// output pixels: bilinear at phase f is `f(1 - f)` working texels squared,
/// which an exact doubling only meets at 1/4 and 3/4 (3/16) and other scales
/// up to two average to 1/6; the cubic B-spline's is 1/3.
fn reconstruction_variance(size: u32, out: u32) -> f32 {
    let scale = out as f32 / size as f32;
    if size == out {
        0.0
    } else if 2 * size >= out {
        let phases = if out == 2 * size {
            3.0 / 16.0
        } else {
            1.0 / 6.0
        };
        scale * scale * phases
    } else {
        scale * scale / 3.0
    }
}

/// The sigma to blur by at a level `size` wide, in its own texels, for a
/// result of `sigma` once drawn back to `out`.
///
/// Variances add under convolution, so what averaging down (a box `scale`
/// wide, `(scale² - 1) / 12`) and drawing back already soften is taken out of
/// the Gaussian. That is what keeps a blur crossing into the next level from
/// stepping.
fn working_sigma(sigma: f32, size: u32, out: u32) -> f32 {
    let scale = out as f32 / size as f32;
    let averaged = (scale * scale - 1.0) / 12.0;
    let variance = sigma * sigma - averaged - reconstruction_variance(size, out);
    variance.max(0.0).sqrt() / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kernel_sums_to_one_and_fits_the_shader() {
        for sigma in [0.5, 1.0, 3.0, 5.99, 6.0, 40.0] {
            let k = Kernel::new(sigma);
            let sum = k.center + 2.0 * k.weights[..k.taps].iter().sum::<f32>();
            assert!((sum - 1.0).abs() < 1e-5, "{sigma}: {sum}");
            assert!(k.taps <= MAX_TAP_PAIRS);
            // Each tap reads between its two texels.
            for (j, o) in k.offsets[..k.taps].iter().enumerate() {
                let i = (2 * j + 1) as f32;
                assert!((i..=i + 1.0).contains(o), "{sigma}: tap {j} at {o}");
            }
        }
    }

    #[test]
    fn no_blur_is_a_copy() {
        let k = Kernel::new(0.0);
        assert_eq!((k.center, k.taps), (1.0, 0));
    }

    #[test]
    fn the_kernel_reaches_four_deviations() {
        let k = Kernel::new(3.0);
        assert_eq!(k.taps, 6);
        assert!(k.offsets[k.taps - 1] <= REACH * 3.0);
    }

    #[test]
    fn small_blurs_stay_at_full_size() {
        assert_eq!(working_level(0.0, 1920), 0);
        assert_eq!(working_level(5.99, 1920), 0);
        assert_eq!(working_level(6.0, 1920), 1);
        // The slider's top on a 1080p window: a 32nd, about σ 3 left.
        assert_eq!(working_level(100.0, 1920), 5);
        let w = working_sigma(100.0, downsampled(1920, 5), 1920);
        assert!((2.5..3.5).contains(&w), "{w}");
    }

    #[test]
    fn a_level_never_shrinks_past_one_texel() {
        assert_eq!(working_level(1000.0, 3), 2);
        assert_eq!(downsampled(3, 2), 1);
    }

    #[test]
    fn every_level_is_left_a_real_gaussian_to_run() {
        // Under MAX_WORKING_SIGMA, or the kernel would be cut short, and well
        // over nothing, or the level above would have done.
        for size in [1920, 1081, 540] {
            for tenth in 60..=1000 {
                let sigma = tenth as f32 / 10.0;
                let small = downsampled(size, working_level(sigma, size));
                let w = working_sigma(sigma, small, size);
                assert!(
                    (2.5..MAX_WORKING_SIGMA).contains(&w),
                    "{size} σ{sigma}: {w}"
                );
            }
        }
    }
}
