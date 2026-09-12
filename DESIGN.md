---
name: Death by MPV
description: A video player whose interface is panes of real glass laid over the film.
colors:
  lit: "#ffffff"
  text-strong: "#ffffffe6"
  text-title: "#ffffffe0"
  text-readout: "#ffffffd0"
  text-named: "#ffffffc4"
  text-body: "#ffffffb0"
  text-back: "#ffffff90"
  text-kicker: "#ffffff80"
  text-faint: "#ffffff70"
  rail: "#ffffff2e"
  wash-hover: "#ffffff1f"
  wash-soft: "#ffffff14"
  knob-on: "#101010e6"
  void: "#000000"
typography:
  title:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "14px"
    fontWeight: 400
  named:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "13px"
    fontWeight: 600
  readout:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "13px"
    fontWeight: 400
  hint:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "13px"
    fontWeight: 400
  label:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "12px"
    fontWeight: 700
  glyph:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "16px"
    fontWeight: 400
  prompt:
    fontFamily: "system UI (Segoe UI on Windows)"
    fontSize: "16px"
    fontWeight: 600
rounded:
  row: "16px"
  panel: "32px"
  pill: "24px"
  button: "20px"
  timeline-pill: "14px"
  preview: "18px"
  preview-tile: "9px"
spacing:
  hair: "4px"
  tight: "6px"
  group: "10px"
  gap: "16px"
  margin: "24px"
  trail-x: "26px"
  label-x: "30px"
components:
  icon-button:
    size: "40px"
    rounded: "{rounded.button}"
    textColor: "{colors.text-body}"
    backgroundColor: "transparent"
  icon-button-hover:
    backgroundColor: "#ffffff24"
    textColor: "{colors.lit}"
  icon-button-open:
    backgroundColor: "#ffffff2e"
    textColor: "{colors.lit}"
  control-pill:
    height: "48px"
    rounded: "{rounded.pill}"
    padding: "4px"
    backgroundColor: "transparent"
  time-pill:
    height: "28px"
    width: "60px"
    rounded: "{rounded.timeline-pill}"
    textColor: "{colors.text-readout}"
    typography: "{typography.readout}"
  menu-row:
    height: "34px"
    padding: "0 26px 0 30px"
    textColor: "{colors.text-body}"
    typography: "{typography.body}"
    backgroundColor: "transparent"
  menu-row-hover:
    backgroundColor: "{colors.wash-hover}"
    rounded: "{rounded.row}"
  menu-row-active:
    textColor: "{colors.lit}"
  section-label:
    height: "28px"
    padding: "0 26px 0 30px"
    textColor: "{colors.text-kicker}"
    typography: "{typography.label}"
  stepper-button:
    size: "26px"
    rounded: "13px"
    backgroundColor: "{colors.wash-soft}"
    textColor: "#ffffffdd"
  toggle-track-on:
    width: "34px"
    height: "18px"
    rounded: "9px"
    backgroundColor: "#ffffffcc"
  toggle-track-off:
    width: "34px"
    height: "18px"
    rounded: "9px"
    backgroundColor: "{colors.rail}"
---

# Design System: Death by MPV

## Overview

**Creative North Star: "The Cinema Window"**

Panes of real glass, laid over the film. Not a metaphor for glass — the
interface is genuinely refractive: mpv's frame is rendered into the
application's own GL context as a texture, and every surface bends the picture
behind it through a rounded-box SDF, a dome profile, Snell's law and an exact
unpolarised Fresnel term. Thickness, bevel and index of refraction are real
parameters, and the rim behaves like a rim because it is one.

The consequence is that this system has almost no palette. Colour comes from
whatever is playing. The interface is white at eleven alphas, a single near-black
switch knob, and the void behind an unloaded window. Even the letterbox is not
neutral: the ambient border extends the frame's own edge colours outward, so a
red scene bleeds red into the bars. Nothing here is tinted by a brand.

Controls behave like instrument switches: definite, short, slightly firm. A
hover wash lands in 45ms and surfaces arrive in 120ms with 6px of travel, which
is fast enough that the movement registers as a response rather than an
animation. Density is generous — a 16px gap on all four sides of every row, and
panels that hold at most a heading and a list.

**Key Characteristics:**
- Real per-pixel refraction, not a translucent blur panel
- No palette of its own; colour is the film's
- One radius rule: every corner nests concentrically inside its parent
- Motion is short, eased out, and never bounces
- Uppercase for the interface's own labels, natural case for the user's content

## Colors

A single hue — white — at an alpha ladder, over live video. The ladder *is* the
hierarchy; there is no second colour to promote anything with.

### Primary
- **Lit** (`#ffffff`): the active state and nothing else. A selected track, a
  hovered chevron, the glyph of a control that is currently on. Full white is
  the system's only emphasis and it is spent sparingly.

### Neutral
- **Strong** (`#ffffffe6`): the played portion of the timeline — the one place a
  near-solid white reads as a measurement rather than as text.
- **Title** (`#ffffffe0`): the media title in the bar, the strongest running text.
- **Readout** (`#ffffffd0`): timestamps and stepper values. Slightly under body
  weight because numbers read heavier than words at the same alpha.
- **Named** (`#ffffffc4`): a playlist heading that carries a show's name.
- **Body** (`#ffffffb0`): every ordinary row label and menu item.
- **Back** (`#ffffff90`): the label of a back row, which is a heading you can press.
- **Kicker** (`#ffffff80`): section labels — SUBTITLES, AUDIO, EFFECTS.
- **Faint** (`#ffffff70`): chevrons at rest, and text stating an absence.
- **Glyph at rest** (`#ffffffbb`): an icon button's own glyph before it is
  hovered. A hair above Body, and deliberately its own value: a 24px mark and a
  14px word at the same alpha do not read as the same weight.
- **Rail** (`#ffffff2e`): the unfilled length of every track and switch.
- **Hover wash** (`#ffffff1f`) and **soft wash** (`#ffffff14`): the inset
  highlight behind a hovered row; soft where the row is a heading or a switch.
- **Knob** (`#101010e6`): the only non-white in the system. A dark disc on a lit
  switch track, so the knob reads as a hole punched through it.
- **Void** (`#000000`): the window before a frame exists.

### Named Rules

**The Borrowed Colour Rule.** The interface owns no hue. Any proposal that adds
a brand colour, a semantic red/green, or an accent to this palette is a proposal
to stop the film from being the only source of colour on screen. Status is
carried by alpha and by glyph, never by hue.

**The Two-Step Rule.** Adjacent text roles differ by at least 0x20 of alpha.
Closer than that and the hierarchy stops reading over moving video, where the
backdrop changes faster than the eye can calibrate.

## Typography

**All roles:** the platform UI font (Segoe UI on Windows) — there is no custom
typeface and no display type anywhere in the product.

**Character:** deliberately anonymous. The interface never speaks above the
film, so the type has no voice of its own; it is sized, weighted and spaced for
legibility over arbitrary moving content and nothing else.

### Hierarchy
- **Title** (600, 14px): the media title in the bar, the flash, a fatal
  headline, and the playlist entry that is playing.
- **Body** (400, 14px): row labels, menu items, switch labels — the name of
  anything you can press.
- **Named** (600, 13px): a playlist heading that is a show's name rather than a
  section label.
- **Readout** (400, 13px): figures — timestamps, slider and stepper values, a
  row's running time, a shortcut's keys — and the labels of the dense rows
  those sit in, which read as part of the same line rather than as menu entries.
- **Hint** (400, 13px): prose added underneath something else — the drop hint,
  the line saying a driver update is the usual fix. A readout's size under its
  own name, because prose is what would want a different one first.
- **Label** (700, 12px): section headings, written in capitals.
- **Glyph** (400, 16px): a character standing in for an icon — the stepper's
  minus and plus, sized against the 26px disc rather than against the text.
- **Prompt** (600, 16px): the one thing the interface says louder than the
  rest, the word on the end-of-file pill, sized against the 24px arrow beside it.

The scale lives in the `Type` global in `app.slint`, not at its use sites.
Four sizes, eight roles, and several roles deliberately share a size: what
separates a title from a body line is weight, and what separates a readout from
a hint is that one is a figure and the other is prose.

### Named Rules

**The Twelve Pixel Floor.** Nothing is smaller than 12px, and 12px is only for
capitals, which are read by their shape. Below that, white text over arbitrary
moving content stops being read and starts being recognised — which is fine for
a heading whose wording you already know and no use at all for a number that
changes. The scale was a step lower than this until every readout in the
interface was found sitting below even the size this document claimed for it;
naming the roles in one place is what keeps that from happening quietly again.

### Named Rules

**The Capitals Rule.** The interface shouts its own words and never the user's.
SUBTITLES, AUDIO and EFFECTS are capitals at 11/700 because they name parts of
the application. A show's name, a track's name, a filename — anything that came
from the user's library — keeps its own case and gets the Named role instead.
Capitalising someone's content is how a tool starts sounding like a database.

## Layout

A dark field with four glass objects along the bottom and at most one panel
above them.

**The bar** is five separate pieces, not one slab: two 60×28 time pills at the
outer ends of the timeline, and three control pills 48px tall — a title pill on
the left, a 232px transport pill centred in the window, and a 380px pill on the
right holding sound, effects and window controls. They sit 24px from the window
edges with 10px between the timeline row and the pills.

**Panels** open above the bar, anchored to the button that opened them and
clamped inside the same 24px margins. Only one is ever open. Each is capped at
the space between the top margin and the bar, and any list inside it scrolls
within that cap; where two lists share a panel they take a fair share each,
except that a list which fits keeps its full height and hands the remainder to
the other.

**Every length is arithmetic on constants and the window width** — never a
question asked of a layout. The glass rects are sampled once per frame to tell
the renderer where to draw, and a binding that depends on a layout forces a full
layout pass on every one of those reads.

**Density:** rows are 34px, headings 28px, switch rows 40px, stepper rows 38px.
The window floors at 900×480, below which the three pills would meet.

### Named Rules

**The One Panel Rule.** Opening any panel closes the others. Two sheets of glass
over the same film is noise, and it doubles the refraction cost on the one
surface the design exists for.

## Elevation & Depth

There are no shadows in this system — not as a prohibition, but as an
observation about what is currently built. Depth is carried entirely by the
material: each panel's rim is a domed bevel whose width is a fraction of that
panel's own corner radius, lit by a Fresnel term that makes the edge turn
mirror-like as it curves away, with a screen-space reflection of the backdrop
riding on top of it. A surface reads as raised because its edge bends light,
which is the same reason a real one does.

Fading a surface in fades its glass with it: opacity scales the shader's
coverage, so a panel arriving at a third of its opacity refracts a third as
hard. Glass at full strength beneath a half-faded surface reads as a hole in the
picture.

### Named Rules

**The Proportional Rim Rule.** Bevel is a fraction of the corner radius, never a
count of pixels. The same absolute rim that looks right rolling around a 32px
panel corner swallows a 28px time pill whole. One slider therefore means the
same thing on every piece of glass, and a small pane simply has a small rolled
edge — which is also how real glass is made.

**The Room to Be Glass Rule.** A surface must leave more margin around its
contents than its own bevel is wide, or the material has nowhere to render. The
seek preview shipped once with 5px of padding around a thumbnail and an 8px
bevel; the glass was there and entirely invisible.

## Shapes

Rounded rectangles throughout, at one governing rule: **radii nest
concentrically.** Two corners share a centre — and so keep an even gap the whole
way round — when their radii differ by exactly the gap between them. So the
row radius (16px) plus the row gap (16px) *is* the panel radius (32px), and the
panel's padding is that same 16px gap, which makes the first and last rows come
out concentric with the panel's own corners without being special-cased.

The same arithmetic governs the seek preview: an 18px panel with 9px of padding
gives a 9px thumbnail. Turn either the radius or the gap and the third value
follows.

Control pills are true capsules (radius = half the height: 24px on a 48px pill,
14px on a 28px time pill). Icon buttons are circles at 40px. The switch is a
capsule with a circular knob inset 2px.

### Named Rules

**The Concentric Rule.** Inner radius = outer radius − the gap between them.
Anything else pinches the curves together at 45° while still meeting cleanly on
the axes, which is exactly the part the eye catches.

## Components

### Icon buttons
- **Shape:** circle (40px, radius 20px), glyph drawn at 24px
- **Default:** transparent, glyph at `#ffffffbb`
- **Hover:** wash at `#ffffff24`, 45ms ease-out; glyph to Lit
- **Open:** a held wash at `#ffffff2e` for as long as the panel it opened is
  showing — a separate channel from the lit glyph, because a button can mean
  both at once. The subtitles button reports whether subtitles are on *and*
  whether its menu is open, and one highlight cannot say both.

### Control pills
- **Shape:** capsule, height 48px (a 40px button with 4px of padding), radius 24px
- **Background:** none. The pill is a glass rect published to the renderer; a
  fill here would paint over the material.
- **Contents:** 40px buttons at 6px apart; the volume track is 96px.

### Menu rows
- **Shape:** 34px tall; the hover wash is inset 16px on both sides at a 16px radius
- **Text:** Body, starting at 30px from the panel edge, eliding at 26px from the right
- **Active:** Lit at 700 weight — colour and weight together, since either alone
  is unreliable over moving video
- **Hover:** wash at `#ffffff1f`, 45ms

### Sliders
- **Track:** 5px tall at Rail, growing to 7px on hover or press (100ms ease-out)
- **Fill:** width-driven from the left at `#ffffffcc`, never clipped, so it stays
  crisp at any size and needs no layer
- **Readout:** right-aligned, with precision taken from the parameter's *range*
  rather than its value — three decimals on a 0–0.1 edge blur, one on a 0–40 blur

### Steppers
- **Shape:** two 26px circles flanking a 62px centred value
- **Use:** for quantities read as numbers rather than positions — a subtitle
  delay is +0.30 s and the useful move is one step. Sliders are for material
  parameters, where the eye judges the result and the number is incidental.

### Switches
- **Shape:** 34×18 capsule, 14px knob inset 2px
- **On:** track `#ffffffcc`, knob `#101010e6` — dark on the lit track, light on
  the dark one, so the knob reads as a hole punched through it either way
- **Motion:** 120ms ease-out on the knob, 120ms on the track colour

### Panels
- **Shape:** 32px radius, 16px padding, no background of its own
- **Entry:** fade and 6px rise over 120ms ease-out, glass fading with the surface
- **Headings:** capitals at Kicker, except a playlist heading carrying a show
  name, which takes the Named role
- **Trailing rows:** a reset or a setting *about* the list gets a 10px gap above
  it. Without it the eye reads one more entry and only the wording says otherwise.

### The seek preview
The signature component. A thumbnail from an 8×8 ffmpeg sprite atlas, one tile
picked with a source-clip, inside an 18px glass panel with 9px of padding and a
9px tile radius, floating above the timeline and clamped inside the bar's
margins. Falls back to the timestamp alone — a 60×26 pill — when no atlas exists,
which is the state it sits in for the first seconds of every unseen file.

## Do's and Don'ts

### Do:
- **Do** derive every length from the constants at the top of the component.
  Panel geometry is read once per frame by the renderer; a binding that asks a
  layout for an answer costs a full layout pass each time.
- **Do** keep radii concentric: inner = outer − gap (16 + 16 = 32).
- **Do** express state in two channels where a control carries two meanings —
  glyph brightness for what it controls, held background for what it opened.
- **Do** size a bevel as a fraction of its own corner radius.
- **Do** leave a surface more padding than its bevel is wide.
- **Do** capitalise the interface's own labels and leave the user's content in
  its own case.
- **Do** fade glass with the surface above it.

### Don't:
- **Don't** add a hue. Not an accent, not a semantic red, not a brand colour.
  The film is the only source of colour on screen.
- **Don't** give a panel a background fill. The glass beneath it is the surface;
  a fill paints over the thing this product exists for.
- **Don't** let it drift toward the utilitarian player — VLC, MPC-HC, mpv's own
  OSC. Grey chrome, system widgets and dense toolbars are the failure mode that
  function-first reasoning always argues for, and the reason this was rebuilt.
- **Don't** open two panels at once.
- **Don't** animate for its own sake. Motion here is a response: 45ms for a
  wash, 120ms for a surface, ease-out, no bounce, no overshoot.
- **Don't** substitute an icon library. The set is 54 SVGs drawn at 24×24 as
  one family; extend it rather than replacing it.
