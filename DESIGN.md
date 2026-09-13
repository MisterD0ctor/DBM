---
name: Death by MPV
description: A video player whose interface is panes of real glass laid over the film.
colors:
  lit: "#ffffff"
  text-strong: "#fffffff2"
  text-title: "#ffffffe5"
  text-readout: "#ffffffd8"
  text-named: "#ffffffcb"
  text-body: "#ffffffbe"
  text-glyph: "#ffffffbb"
  text-kicker: "#ffffffb1"
  text-hint: "#ffffffa4"
  text-faint: "#ffffff97"
  text-dim: "#ffffff80"
  fill: "#ffffffcc"
  rail: "#ffffff2e"
  wash-strong: "#ffffff2e"
  wash-button: "#ffffff24"
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
    fontWeight: 400
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
    fontWeight: 400
    letterSpacing: "1px"
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
  timeline: "14px"
  preview: "18px"
  preview-tile: "9px"
spacing:
  hair: "4px"
  tight: "6px"
  group: "10px"
  offset: "12px"
  gap: "16px"
  margin: "24px"
  trail-x: "26px"
  label-x: "30px"
components:
  icon-button:
    size: "40px"
    rounded: "{rounded.button}"
    textColor: "{colors.text-glyph}"
    backgroundColor: "transparent"
  icon-button-hover:
    backgroundColor: "{colors.wash-button}"
    textColor: "{colors.lit}"
  icon-button-open:
    backgroundColor: "{colors.wash-strong}"
  control-pill:
    height: "48px"
    rounded: "{rounded.pill}"
    padding: "4px"
    backgroundColor: "transparent"
  timeline-row:
    height: "28px"
    rounded: "{rounded.timeline}"
    textColor: "{colors.text-readout}"
    typography: "{typography.readout}"
    backgroundColor: "transparent"
  timeline-time:
    width: "60px"
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
  menu-row-focus:
    backgroundColor: "{colors.wash-strong}"
    rounded: "{rounded.row}"
  menu-row-active:
    textColor: "{colors.lit}"
  section-label:
    height: "28px"
    padding: "0 26px 0 30px"
    textColor: "{colors.text-kicker}"
    typography: "{typography.label}"
  section-label-named:
    height: "28px"
    padding: "0 26px 0 30px"
    textColor: "{colors.text-named}"
    typography: "{typography.named}"
  slider-row:
    height: "30px"
    padding: "0 26px 0 30px"
    textColor: "{colors.text-body}"
    typography: "{typography.readout}"
  stepper-button:
    size: "26px"
    rounded: "13px"
    backgroundColor: "{colors.wash-soft}"
    textColor: "#ffffffdd"
  stepper-button-hover:
    backgroundColor: "{colors.wash-strong}"
  toggle-track-on:
    width: "34px"
    height: "18px"
    rounded: "9px"
    backgroundColor: "{colors.fill}"
  toggle-track-off:
    width: "34px"
    height: "18px"
    rounded: "9px"
    backgroundColor: "{colors.rail}"
  hover-label:
    height: "28px"
    rounded: "14px"
    padding: "0 12px"
    textColor: "{colors.text-body}"
    typography: "{typography.body}"
  end-pill:
    height: "48px"
    rounded: "{rounded.pill}"
    padding: "0 18px"
    textColor: "{colors.lit}"
    typography: "{typography.prompt}"
  end-pill-hover:
    backgroundColor: "{colors.wash-button}"
  seek-preview:
    rounded: "{rounded.preview}"
    padding: "9px"
    textColor: "{colors.text-strong}"
    typography: "{typography.readout}"
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
whatever is playing. The interface is white at sixteen alphas and full white, a
single near-black switch knob, and the void behind an unloaded window. Even the
letterbox is not neutral: the ambient border extends the frame's own edge
colours outward, so a red scene bleeds red into the bars. Nothing here is tinted
by a brand.

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
- **Lit** (`#ffffff`): the active state and nothing else. A selected track, the
  playing episode, a hovered chevron, the glyph of a control that is currently
  on, the flash and the end-of-file prompt. Full white is the system's only
  emphasis and it is spent sparingly.

### Neutral
Text roles, brightest first:

- **Strong** (`#fffffff2`): the played portion of the timeline, and the
  timestamp under a seek preview — the places a near-solid white reads as a
  measurement rather than as text.
- **Title** (`#ffffffe5`): the media title in the bar, a fatal headline, a
  switch's label and a stepper's value — the strongest running text.
- **Readout** (`#ffffffd8`): the elapsed and total times, and the fill of a
  playlist row's resume measure. Slightly under Title because numbers read
  heavier than words at the same alpha.
- **Named** (`#ffffffcb`): a playlist heading that carries a show's name.
- **Body** (`#ffffffbe`): ordinary row labels, menu items, a hover label's name,
  a slider's and a stepper's label, a driver's error text.
- **Glyph at rest** (`#ffffffbb`): an icon button's own glyph before it is
  hovered. Deliberately its own value rather than Body's: a 24px mark and a
  14px word at the same alpha do not read as the same weight.
- **Kicker** (`#ffffffb1`): section labels — SUBTITLES, AUDIO, EFFECTS — with
  their icons; the label and chevron of a back row, which is a heading you can
  press; and the icon and label of a row that leads to another page. Brighter
  than the rungs below it *because* tracked capitals at 400 put down thin
  strokes, and thin strokes at a low alpha dissolve. It is subordinate by
  shape, so it does not have to be faint.
- **Hint** (`#ffffffa4`): prose added under something else — the line saying a
  driver update is the usual fix, the third way to open a file — and a
  stepper row's icon.
- **Faint** (`#ffffff97`): a shortcut's keys, a playlist row's running time, a
  chevron at rest, a scroll thumb, and text stating an absence.
- **Dim readout** (`#ffffff80`): a slider's own value, which sits beside the
  name of the thing it sets and must not compete with it.

Marks and surfaces:

- **Fill** (`#ffffffcc`): the filled length of the volume track and every
  settings slider, a lit switch track, and an unlit switch's knob.
- **Rail** (`#ffffff2e`): the unfilled length of every track and switch, and of
  the resume measure.
- **Strong wash** (`#ffffff2e`): where the keyboard is on a row; the held
  background of a button whose panel is open; a hovered stepper disc.
- **Button wash** (`#ffffff24`): a hovered icon button, and the hovered
  end-of-file pill.
- **Hover wash** (`#ffffff1f`): the inset highlight behind a hovered row.
- **Soft wash** (`#ffffff14`): a hovered row that is a heading or a switch; a
  stepper disc at rest; a scroll hint's rail; and the one panel fill in the
  system, on the fatal-error pane — see Elevation & Depth.
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

*Currently broken, and the code is what is wrong.* The text ladder was retuned
brighter and now steps by 0x0d: Strong `f2`, Title `e5`, Readout `d8`, Named
`cb`, Body `be`, Kicker `b1`, Hint `a4`, Faint `97` — with Glyph at rest (`bb`)
three below Body, and only Dim readout (`80`) a clear step from its neighbour.
The rule stands. Bringing the values back into line with it — by spreading the
ladder or by merging roles that no longer need to differ — is open work, and no
new role may be squeezed into the gaps meanwhile.

## Typography

**All roles:** the platform UI font (Segoe UI on Windows) — there is no custom
typeface and no display type anywhere in the product.

**Character:** deliberately anonymous. The interface never speaks above the
film, so the type has no voice of its own; it is sized, weighted and spaced for
legibility over arbitrary moving content and nothing else.

### Hierarchy
- **Title** (600, 14px): the media title in the bar, the flash, and a fatal
  headline.
- **Body** (400, 14px): row labels, menu items, switch labels — the name of
  anything you can press. The current row in a list takes Lit at 700 instead.
- **Named** (400, 13px): a playlist heading that is a show's name rather than a
  section label. The Label role's weight and neither of its other habits: its
  own case, its own size, no tracking, and a brighter alpha. Both fill the same
  slot at the top of a panel, so what separates them is what they *are* — the
  user's word or the interface's — and nothing else has to say it.
- **Readout** (400, 13px): figures — timestamps, slider and stepper values, a
  row's running time, a shortcut's keys — and the labels of the dense rows
  those sit in, which read as part of the same line rather than as menu entries.
- **Hint** (400, 13px): prose added underneath something else — the drop hint,
  the line saying a driver update is the usual fix, a driver's own error text.
  A readout's size under its own name, because prose is what would want a
  different one first.
- **Label** (400, 12px, tracked 1px): section headings and back rows, written in
  capitals.
- **Glyph** (400, 16px): a character standing in for an icon — the stepper's
  minus and plus, sized against the 26px disc rather than against the text.
- **Prompt** (600, 16px): the one thing the interface says louder than the
  rest, the word on the end-of-file pill, sized against the 24px arrow beside it.

The scale lives in the `Type` global in `app.slint`, not at its use sites.
Four sizes, eight roles, and several roles deliberately share a size: what
separates a title from a body line is weight, and what separates a readout from
a hint is that one is a figure and the other is prose.

### Named Rules

**One Weight For Headings.** Every heading in the system is 400, whatever it
holds. A show's name at 600 beside a kicker at 400 read as two different
designs in two panels — and since 600 is Bold here, it had also become a small
copy of the playing row rather than a heading of its own. A panel's top line is
one slot; only the thing in it should differ.

**The Label Must Not Out-Ink Its Content.** A section label is furniture; the
rows under it are the point. At 700 a kicker put down 34.9 lit pixels per
character against a body row's 25.6 — heavier ink than the thing it labelled,
while the alpha ladder claimed it was quieter — and the only cue left telling
them apart was the capitals. Capitals alone are not enough in a list of
nineteen. A label is told from its content by case, size, tracking and weight;
it is never told apart by being the boldest thing on the page.

Two things follow, both learned the hard way. **Tracking is the cue**: capitals
set solid read as a word, capitals with air between them read as a label. And
**600 is not a weight here** — Segoe UI ships Semibold as its own family rather
than as a weight of Segoe UI, so 600 and 700 rasterise identically, to the
pixel. Going lighter means going to 400.

**The Twelve Pixel Floor.** Nothing is smaller than 12px, and 12px is only for
capitals, which are read by their shape. Below that, white text over arbitrary
moving content stops being read and starts being recognised — which is fine for
a heading whose wording you already know and no use at all for a number that
changes. The scale was a step lower than this until every readout in the
interface was found sitting below even the size this document claimed for it;
naming the roles in one place is what keeps that from happening quietly again.

**The Capitals Rule.** The interface shouts its own words and never the user's.
SUBTITLES, AUDIO and EFFECTS are tracked capitals at 12/400 because they name
parts of the application. A show's name, a track's name, a filename — anything
that came from the user's library — keeps its own case and gets the Named role
instead. Capitalising someone's content is how a tool starts sounding like a
database.

## Layout

A dark field with four glass objects along the bottom and at most one panel
above them.

**The bar** is four separate pieces, not one slab. The timeline row is one pane
28px tall running the full width, holding the elapsed time at its left end, the
total at its right, and the track between them. Below sit three control pills
48px tall — a title pill on the left, a 232px transport pill centred in the
window, and a 380px pill on the right holding sound, effects and window
controls. They sit 24px from the window edges with 10px between the timeline
row and the pills. Inside a pill, 40px buttons sit 6px apart with 4px of
padding; text sitting directly on a pill is inset 18px, clear of the capsule's
curve.

The title pill hugs its title, up to the room the centred transport leaves, and
past that the title elides. It moves to a new width over 160ms ease-in-out
rather than jumping: a pill that resized itself on every file change would be a
flicker, and the glass follows because it is drawn from the same number.

The timeline row is one pane because it makes one statement: where you are, out
of how much. It was three for a while — a pill around each time and the track
bare over the video between them — which cut glass between parts of a sentence
and left the long run in the middle looking like the only part that needed a
surface. It also left the track with no glass at all, and a 5px line over a
white frame resolved to 1.02:1 against the picture with the played edge at
1.13:1 against the rail. Not dim: gone.

A row with contents at both ends and nothing but a line in the middle is the
one place in this system where **the glass is sized by what the row means, not
by what is written in it.**

**Panels** open 12px above the timeline row, anchored to the button that opened
them and clamped inside the same 24px margins: the tracks panel and the
playlist at 380px, settings at 340px, the open menu at 260px. Only one is ever
open. Each is capped at the space between the top margin and the bar, and any
list inside it scrolls within that cap; where two lists share a panel they take
a fair share each, except that a list which fits keeps its full height and hands
the remainder to the other. The hover label opens on that same line, 12px above
the timeline; the seek preview floats 10px above it.

**The middle of the picture** belongs to things that are about the film rather
than controls for it: the action flash, the end-of-file pill, and — with no film
loaded — the panel that offers a way in.

**Every length is arithmetic on constants and the window width** — never a
question asked of a layout. The glass rects are sampled once per frame to tell
the renderer where to draw, and a binding that depends on a layout forces a full
layout pass on every one of those reads.

**Density:** rows are 34px, headings 28px, switch rows 40px, stepper rows 38px,
slider rows 30px. A heading that follows a group is 38px — the same 28px with
the 10px group gap carried above it. The window opens at 1280×720 and floors at
900×480, below which the three pills would meet; it never reflows.

### Named Rules

**The One Panel Rule.** Opening any panel closes the others. Two sheets of glass
over the same film is noise, and it doubles the refraction cost on the one
surface the design exists for. The hover label stands down while any panel is
open, for the same reason.

**The Gap Goes Above Rule.** The 10px group gap always sits above the thing that
starts something new, never below the thing that ended — above a section
heading, above a trailing reset. Split evenly, or left out, a heading sits the
same distance from the group it names as from the group it follows: measured at
30px above and 32px below on the shortcuts list, which is a label equidistant
between two things, labelling neither. Spacing is the only thing in a list of
identical rows that says where one idea stops.

## Elevation & Depth

There are no shadows in this system — not as a prohibition, but as an
observation about what is currently built. Depth is carried entirely by the
material: each panel's rim is a domed bevel whose width is a fraction of that
panel's own corner radius, lit by a Fresnel term that makes the edge turn
mirror-like as it curves away, with a screen-space reflection of the backdrop
riding on top of it. A surface reads as raised because its edge bends light,
which is the same reason a real one does.

Every surface opts into the tint, and with it absorption: the transmitted part
of the glass darkens as the frame behind it brightens, on a smooth curve rather
than a threshold, so white text keeps a ground over a white scene. The pull
toward the tint colour is a setting; the absorption under it is not.

Fading a surface in fades its glass with it: opacity scales the shader's
coverage, so a panel arriving at a third of its opacity refracts a third as
hard. Glass at full strength beneath a half-faded surface reads as a hole in the
picture.

The glass runs whether or not a film is loaded — at rest it composites over a
black frame, where the tint and the rim still resolve into a faint lift with an
edge on it. So the empty window's panel is glass like every other surface. The
**single exception** is the fatal-error pane, which carries a Soft wash fill
(`#ffffff14`) at the panel radius: every failure that raises it leaves the
player with no render context, so the glass it publishes is drawn by nobody.

### Named Rules

**The Proportional Rim Rule.** Bevel is a fraction of the corner radius, never a
count of pixels. The same absolute rim that looks right rolling around a 32px
panel corner swallows a 28px pane whole. One slider therefore means the same
thing on every piece of glass, and a small pane simply has a small rolled edge —
which is also how real glass is made.

**The Room to Be Glass Rule.** A surface must leave more margin around its
contents than its own bevel is wide, or the material has nowhere to render. The
seek preview shipped once with 5px of padding around a thumbnail and an 8px
bevel; the glass was there and entirely invisible.

**The Transmissive Middle Rule.** Absorption — the thing that keeps white text
and white controls legible over a bright frame — is applied to the transmitted
component only, never to the rim, the sky highlight or the mirrored backdrop.
The rim is the full corner radius, so a capsule is rim the whole way through
and has no transmitted component to darken. **A pane that exists to give
something a ground must be tall enough to have a flat middle.** This is why the
timeline's glass is the height of its row and not the height of its 5px track:
glass at the track's own size would have been all edge and would have darkened
for nobody, which is the one thing it was added to do.

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

Control pills, the timeline row, the hover label, the flash's caption and the
end-of-file pill are true capsules (radius = half the height: 24px on 48px,
14px on 28px). Icon buttons are circles at 40px and the flash is a circle at
56px. Every track, fill, measure and scroll thumb is a capsule too. The switch
is a capsule with a circular knob inset 2px.

There are no borders anywhere, and no hairlines. Where a state would
conventionally be drawn as an outline — the keyboard's position — it is carried
by a stronger wash instead.

### Named Rules

**The Concentric Rule.** Inner radius = outer radius − the gap between them.
Anything else pinches the curves together at 45° while still meeting cleanly on
the axes, which is exactly the part the eye catches.

## Components

### Icon buttons
- **Shape:** circle (40px, radius 20px), glyph drawn at 24px
- **Default:** transparent, glyph at Glyph at rest
- **Hover:** Button wash, 45ms; glyph to Lit
- **Active:** glyph to Lit, and every button that sets it swaps its own glyph as
  well — muted for unmuted, slashed for unslashed. A wash behind an engaged
  button was tried and taken back out: it made the state read as chrome.
- **Open:** a held Strong wash for as long as the panel it opened is showing — a
  separate channel from the lit glyph, because a button can mean both at once.
  The subtitles button reports whether subtitles are on *and* whether its menu
  is open, and one highlight cannot say both.

### Control pills
- **Shape:** capsule, height 48px (a 40px button with 4px of padding), radius 24px
- **Background:** none. The pill is a glass rect published to the renderer; a
  fill here would paint over the material.
- **Contents:** 40px buttons at 6px apart; the title pill's text is Title at
  600, inset 18px, eliding.

### Timeline row
- **Shape:** one 28px capsule of glass across the bar's full width
- **Times:** Readout, centred in 60px each — wide enough for `1:23:45`, so the
  row does not shift when a film passes the hour
- **Track:** 5px at Rail, growing to 7px under the pointer or a drag (100ms
  ease-out); played length at Strong, width-driven so it stays crisp
- **Hit area:** the full row height, not the track it draws — a 5px target is
  miserable to grab. The wheel over it seeks.

### Volume
- **Track:** 96px, 5px at Rail growing to 7px (100ms ease-out), filled at Fill
  across mpv's full 0–200% range
- **Unity dot:** a dot, not a line, at 100%. A line the height of the track
  divides it into two ranges; a dot is a landmark on one. It reverses against
  the fill the way the switch knob does.

### Menu rows
- **Shape:** 34px tall; the wash is inset 16px on both sides at a 16px radius,
  full row height so adjacent washes meet
- **Text:** Body, starting 30px from the panel edge, eliding 26px from the right
- **Hover:** Hover wash, 45ms
- **Focus:** Strong wash, shown only while the keyboard is in use — never a ring
- **Active:** Lit at 700, and the active mark: a 3×14 capsule in Lit, 20px from
  the panel edge, inside the wash so a row that is both current and focused
  wears both marks without them touching. A shape, because weight and alpha
  alone were close to invisible over moving video.

### Playlist rows
- **Shape:** a menu row with two more facts on it
- **Running time:** Faint at Readout, right-aligned so a column of them reads
  as a column; empty for a file never opened and not yet described
- **Title:** elides clear of the time and the measure, never under them

### The resume measure

A 44×4 capsule on a playlist row, at Rail with a fill at Readout, sitting on
the row's own line 10px before the running time. It says how far into
that file you got, and appears only when there is a resume point to show.

- **Fixed length, never a share of the row.** It has to read the same however
  wide the panel is, and it must never reach a width that would make it a rule.
- **A floor on the fill**, one capsule's height. A file barely started still
  has to show that it was started, and a single pixel of white does not.

### Named Rules

**The Not Under The Words Rule.** A measure on a text row goes on the line,
never beneath it. Under a title at the width of its column it is a divider —
and it is one whichever way it is filled, so at the end of a file the fill
covers the track and leaves a rule sitting between two titles. Under a title
and short, it lands at about the width of the words above it and becomes an
underline. There is no length that is safe underneath; the fix is the line the
row already has, beside the fact that is already there.

### Scroll hint
- **Shape:** a 3px rail at Soft wash, 7px from the list's right edge and inset
  2px top and bottom; the thumb a capsule at Faint, proportional to how much of
  the list is visible, with a 24px floor
- **Role:** a hint, not a control. It says there is more and roughly where you
  are; the wheel, the flick and the keyboard move the list. The keyboard scrolls
  by the least that reveals the focused row.

### Sliders
- **Row:** 30px; label 78px at Body in Readout size, value 46px right-aligned at
  Dim readout
- **Track:** 4px at Rail, growing to 6px on hover or press (100ms ease-out) —
  a step lighter than the bar's 5px tracks, because these sit inside a panel of
  rows rather than alone on glass
- **Fill:** width-driven from the left at Fill, never clipped
- **Readout:** precision taken from the parameter's *range* rather than its
  value — three decimals on a 0–0.1 edge blur, one on a 0–40 blur
- **Focus:** the row takes the Strong wash; the sliders have no hover wash,
  since the growing track already answers the pointer

### Steppers
- **Row:** 38px; an 18px icon at Hint, the label at Body in Readout size, then
  two 26px discs flanking a 62px value at Title
- **Discs:** Soft wash at rest, Strong wash on hover (45ms); `−` (U+2212, to match
  the digits) and `+` in Glyph size at `#ffffffdd`
- **Use:** for quantities read as numbers rather than positions — a subtitle
  delay is +0.30 s and the useful move is one step. Sliders are for material
  parameters, where the eye judges the result and the number is incidental.

### Switches
- **Row:** 40px; switch first, label after it at Title in Body size, 12px apart
- **Shape:** 34×18 capsule, 14px knob inset 2px
- **On:** track at Fill, knob `#101010e6`; **off:** track at Rail, knob at Fill —
  dark on the lit track, light on the dark one, so the knob reads as a hole
  punched through it either way
- **Motion:** 120ms ease-out on the knob, 120ms on the track colour
- **Hover:** Soft wash; one target for the whole row

### Drill rows
- **Row:** 34px; an 18px icon and the label, both at Kicker, 10px apart, with a
  chevron at the end — Faint at rest, Lit on hover
- **Use:** every row of the settings tree. An effect that can be turned off says
  so with its glyph rather than with a switch, because a switch beside a chevron
  is two things to press in one row.

### Back rows
- **Row:** 34px, the page's heading and the way out of it at once
- **Text:** Label role at Kicker, starting on the row-label line; Lit on hover,
  over a Soft wash
- **Chevron:** 14px, hanging in the 14px margin so the words land where every
  heading and row label in the panel starts

### Panels
- **Shape:** 32px radius, 16px padding, no background of its own
- **Entry:** fade and 6px rise over 120ms ease-out, glass fading with the surface
- **Headings:** tracked capitals at Kicker, except a playlist heading carrying
  a show name, which takes the Named role — natural case, no tracking, one
  step of alpha brighter, and the same weight. A heading with a group above it
  carries the 10px gap itself rather than being pushed down by the row above,
  because the last row of a group does not know that it is last
- **Heading icons:** a heading over rows without icons of their own carries
  one — the playlist, SUBTITLES and AUDIO in the tracks panel, every group on
  the shortcuts page. 16px, not a row's 18px, because the words beside it are
  12px capitals rather than 14px text; at the heading's own alpha, so mark and
  words are one thing; starting on the row-label line with the words 8px after
  it. It cannot hang in the margin as the back row's chevron does: that margin
  is 14px to the edge of the wash. The settings page's headings have none,
  because every row under them already wears one
- **Trailing rows:** a reset or a setting *about* the list gets a 10px gap above
  it. Without it the eye reads one more entry and only the wording says otherwise.
- **Shortcut rows:** Body label, keys at Faint in Readout size, 12px apart. On
  the reference page they are inert — no wash and no pointer, because a wash
  under a row that does nothing when clicked is an offer the row cannot keep.

### The hover label

A 28px capsule of glass that names the control under the pointer and gives the
key that does the same thing. The name is Body, the key is Faint at Readout
size, 12px apart with 12px of padding — the two roles of a shortcuts row,
unchanged, because the tip is that list arriving one row at a time where the
question was asked.

- **500ms before it opens**, then a fade and a 6px rise like any other surface.
  Long enough that crossing the bar raises nothing; short enough that stopping
  on a glyph feels answered rather than eventually attended to.
- **Above the timeline row, on the line panels open from**, centred on its
  control and clamped inside the bar's margins. There is nowhere closer: the
  row between is one unbroken pane, and a tip laid over it would be glass on
  glass.
- **It says what pressing will do**, not what the glyph already shows. The
  glyph says which state you are in; the label is the only thing that can say
  what happens next, so a muted control offers *Unmute*.
- **One surface for the whole bar.** Only one is ever up, and thirteen would be
  thirteen more rects published to the renderer for a thing that is never in
  two places.

### Named Rules

**The Label Cannot Outlive Its Control.** Whether a tip is drawn is an
expression, never a decision taken when the pointer arrives. Both ways it can
go wrong happen afterwards: a pointer resting on a button raises no events, so
the idle clock runs out under a label that is already up and the bar leaves
without it; and a panel can open underneath one, which it did, over the very
row the tip was covering, because the check for that ran once on hover and
never again.

### The seek preview
The signature component. A thumbnail from an 8×8 ffmpeg sprite atlas, one tile
picked with a source-clip, inside an 18px glass panel with 9px of padding and a
9px tile radius, with the timestamp beneath it at Strong in a 22px caption. It
floats 10px above the timeline, centred on the pointer and clamped inside the
bar's margins, and takes no pointer events of its own. Falls back to the
timestamp alone — a 74×26 pane — when no atlas exists, which is the state it
sits in for the first seconds of every unseen file and permanently on a machine
with no ffmpeg.

### The action flash
An answer, not a control: a 24px glyph in Lit inside a 56px circle of glass at
the centre of the picture, and 12px under it, when there is one, a 28px capsule
carrying the one number that has nowhere else to appear — Title at 600 in Lit,
14px of padding, never wider than the bar. It lives 700ms, long enough to read a
two-character figure and short enough that a held key flickers rather than
strobes. A notice from the player replaces the ring with its sentence alone,
centred, for 3200ms. No part of it takes the pointer: it sits where a click
means play or pause.

### The end-of-file pill
The bar's own 48px capsule, in the middle of the picture where the film
stopped: a 24px glyph and the word — *Next*, or *Restart* at the end of the last
file — in Prompt at Lit, 12px apart, inset 18px. Button wash on hover. It is the
one surface not tied to the idle clock: the bar hides after a few still seconds,
and this is what should still be there when it does.

### The way in
With no film loaded, a 320px panel of glass centred in the window: Open file,
Open folder and Settings as shortcut rows with their keys, then a line at Hint
saying a file can be dropped anywhere. While an open is in flight the two open
rows fade to half and stop answering, and the line names what is arriving —
Settings stays at full strength, because it still works. Carrying a failure it
widens to 420px and offers nothing: a Title headline at 600, the driver's own
error text at Body in Hint size wrapping to three lines, and the line at Hint
saying a driver update is the usual fix.

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
- **Do** show where the keyboard is with the Strong wash, and only while the
  keyboard is being used.

### Don't:
- **Don't** add a hue. Not an accent, not a semantic red, not a brand colour.
  The film is the only source of colour on screen.
- **Don't** give a panel a background fill. The glass beneath it is the surface;
  a fill paints over the thing this product exists for. The fatal-error pane is
  the one exception, because there is no glass to paint over.
- **Don't** draw a border, a ring or a hairline. There is no outline anywhere in
  this interface, and a focus ring would make the keyboard the only thing that
  draws one.
- **Don't** let it drift toward the utilitarian player — VLC, MPC-HC, mpv's own
  OSC. Grey chrome, system widgets and dense toolbars are the failure mode that
  function-first reasoning always argues for, and the reason this was rebuilt.
- **Don't** open two panels at once.
- **Don't** animate for its own sake. Motion here is a response: 45ms for a
  wash, 100ms for a track, 120ms for a surface or a switch, ease-out, no bounce,
  no overshoot.
- **Don't** substitute an icon library. The set is 59 SVGs drawn at 24×24 as
  one family; extend it rather than replacing it.
