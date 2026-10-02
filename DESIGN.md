---
name: Death by MPV
description: A video player whose interface is panes of real glass laid over the film.
colors:
  lit: "#ffffff"
  strong: "#ffffffe0"
  body: "#ffffffc0"
  quiet: "#ffffffa0"
  dim: "#ffffff80"
  played: "#fffffff2"
  fill: "#ffffffcc"
  rail: "#ffffff2e"
  knob: "#101010e6"
  ground: "#000000"
  wash-strong: "#ffffff34"
  wash-button: "#ffffff24"
  wash-soft: "#ffffff14"
typography:
  title:
    fontFamily: "IBM Plex Sans"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: "IBM Plex Sans"
    fontSize: "14px"
    fontWeight: 500
  named:
    fontFamily: "IBM Plex Sans"
    fontSize: "13px"
    fontWeight: 500
  readout:
    fontFamily: "IBM Plex Sans"
    fontSize: "13px"
    fontWeight: 500
  hint:
    fontFamily: "IBM Plex Sans"
    fontSize: "13px"
    fontWeight: 500
  timecode:
    fontFamily: "IBM Plex Mono"
    fontSize: "13px"
    fontWeight: 500
  label:
    fontFamily: "IBM Plex Sans"
    fontSize: "12px"
    fontWeight: 500
    letterSpacing: "1px"
  glyph:
    fontFamily: "IBM Plex Sans"
    fontSize: "16px"
    fontWeight: 500
  prompt:
    fontFamily: "IBM Plex Sans"
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
  group: "16px"
  offset: "12px"
  gap: "16px"
  margin: "24px"
  trail-x: "26px"
  label-x: "30px"
components:
  icon-button:
    size: "40px"
    rounded: "{rounded.button}"
    textColor: "{colors.body}"
    backgroundColor: "transparent"
  icon-button-hover:
    backgroundColor: "{colors.wash-button}"
    textColor: "{colors.lit}"
  icon-button-open:
    backgroundColor: "{colors.wash-strong}"
  icon-button-focus:
    backgroundColor: "{colors.wash-strong}"
  icon-button-disabled:
    textColor: "{colors.dim}"
  control-pill:
    height: "48px"
    rounded: "{rounded.pill}"
    padding: "4px"
    backgroundColor: "transparent"
  timeline-row:
    height: "28px"
    rounded: "{rounded.timeline}"
    textColor: "{colors.strong}"
    typography: "{typography.readout}"
    backgroundColor: "transparent"
  timeline-time:
    width: "60px"
    textColor: "{colors.strong}"
    typography: "{typography.readout}"
  menu-row:
    height: "34px"
    padding: "0 26px 0 30px"
    textColor: "{colors.body}"
    typography: "{typography.body}"
    backgroundColor: "transparent"
  menu-row-hover:
    backgroundColor: "{colors.wash-soft}"
    rounded: "{rounded.row}"
  menu-row-focus:
    backgroundColor: "{colors.wash-strong}"
    rounded: "{rounded.row}"
  menu-row-active:
    textColor: "{colors.lit}"
  section-label:
    height: "28px"
    padding: "0 26px 0 30px"
    textColor: "{colors.body}"
    typography: "{typography.label}"
  section-label-named:
    height: "28px"
    padding: "0 26px 0 30px"
    textColor: "{colors.body}"
    typography: "{typography.named}"
  slider-row:
    height: "30px"
    padding: "0 26px 0 30px"
    textColor: "{colors.body}"
    typography: "{typography.readout}"
  stepper-button:
    size: "26px"
    rounded: "13px"
    backgroundColor: "{colors.wash-soft}"
    textColor: "{colors.strong}"
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
    textColor: "{colors.body}"
    typography: "{typography.body}"
  end-pill:
    height: "48px"
    rounded: "{rounded.pill}"
    padding: "0 18px"
    textColor: "{colors.lit}"
    typography: "{typography.prompt}"
  end-pill-hover:
    backgroundColor: "{colors.wash-button}"
  end-choice:
    height: "40px"
    rounded: "{rounded.button}"
    padding: "0 14px"
    textColor: "{colors.lit}"
    typography: "{typography.prompt}"
  resume-row:
    height: "34px"
    padding: "0 26px 0 30px"
    textColor: "{colors.strong}"
    typography: "{typography.body}"
  seek-preview:
    rounded: "{rounded.preview}"
    padding: "9px"
    textColor: "{colors.strong}"
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
whatever is playing. The interface is white at a handful of alphas and full
white, a single near-black switch knob, and — before anything is loaded — the
dimmed light of the last film left unfinished. Even the
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
- Text on five alpha steps, 0x20 apart, from a floor that does not move
- One radius rule: every corner nests concentrically inside its parent
- Motion is short, eased out, and never bounces
- Uppercase for the interface's own labels, natural case for the user's content

## Colors

A single hue — white — at an alpha ladder, over live video. The ladder *is* the
hierarchy; there is no second colour to promote anything with. Every value
below is named once, in the `Paint` global at the top of `app.slint`, and
nowhere else in the interface is a colour written out.

### Primary
- **Lit** (`#ffffff`): the active state, and where the hand or the keyboard
  is. A selected track, the playing episode, a hovered chevron or glyph, the
  glyph of a control that is currently on, the words of the row the keyboard
  is on, the flash and the end-of-file prompt. Full white is the system's only
  emphasis and it is spent sparingly.

### Neutral

**Text and glyphs** — four steps under Lit, 0x20 apart, except at the top:
Lit is `0xff` and Strong `0xe0`, which is `0x1f`. The ladder is built upward
from the floor at `0x80` in steps of `0x20` — `0x80, 0xa0, 0xc0, 0xe0` — and
the step above `0xe0` would be `0x100`, which does not exist in eight bits.
Full white is one short of the rule and always will be; every step that can
keep it does:

- **Strong** (`#ffffffe0`): text that leads. The media title, the elapsed and
  total times, the timestamp under a seek preview, a switch's label, a
  stepper's value and its minus and plus, a fatal headline, and the fill of a
  playlist row's resume measure.
- **Body** (`#ffffffc0`): the ordinary run of the interface. Row labels and menu
  items, a hover label's name, a slider's and a stepper's label, a driver's
  error text; every section heading and its icon, and a show's name in the same
  slot; back rows and drill rows; and every icon button's glyph at rest.
- **Quiet** (`#ffffffa0`): what sits beside or under something else. A
  shortcut's keys, a playlist row's running time, the prose hints, a chevron at
  rest, a stepper row's icon, a scroll thumb, the unity dot where it sits off
  the volume fill, and text stating an absence.
- **Dim** (`#ffffff80`): a slider's own value, which sits beside the name of the
  thing it sets and must not compete with it; and the glyph of a bar button
  with nowhere to go — Previous on the first file, Next on the last. **The
  floor**: see below.

**Marks:**

- **Played** (`#fffffff2`): the played length of the timeline — the one
  near-solid white that reads as a measurement rather than as text.
- **Fill** (`#ffffffcc`): the filled length of the volume track and every
  settings slider, a lit switch track, and an unlit switch's knob.
- **Rail** (`#ffffff2e`): the unfilled length of every track, switch and
  measure.
- **Knob** (`#101010e6`): the only non-white in the system. A dark disc on a lit
  switch track, so the knob reads as a hole punched through it; the unity dot
  where it sits on the fill.
- **Ground** (`#000000`): the window before a frame exists.

**Washes** — three, and the two a row can wear sit 0x20 apart:

- **Strong wash** (`#ffffff34`): where the keyboard is, on a row or on a bar
  button; the held background of a button whose panel is open; a hovered
  stepper disc.
- **Button wash** (`#ffffff24`): a hovered icon button, and a hovered choice on
  the end-of-file pill. Never on a row, so it never has to be told apart from
  the washes either side of it.
- **Soft wash** (`#ffffff14`): the inset highlight behind a hovered row of any
  kind; a stepper disc at rest; a scroll hint's rail; and the one panel fill in
  the system, on the fatal-error pane — see Elevation & Depth.

**The Rail** and the Strong wash were once the same `#ffffff2e` under two
names, and the row washes stepped 0x0f apart — so a hand resting in a panel and
the keyboard walking it lit two rows in one colour, and nothing said which one
Enter would press. The washes now keep the Two-Step Rule the text keeps, and
the rail is a mark with a value of its own.

### Named Rules

**The Borrowed Colour Rule.** The interface owns no hue. Any proposal that adds
a brand colour, a semantic red/green, or an accent to this palette is a proposal
to stop the film from being the only source of colour on screen. Status is
carried by alpha and by glyph, never by hue.

**The Two-Step Rule.** Adjacent text steps differ by at least 0x20 of alpha,
with the one exception arithmetic forces on the ladder's top rung — see above,
and do not "fix" it by moving Strong down, which would spend a real step to
satisfy a rule about the one gap nobody can widen.
Closer than that and the hierarchy stops reading over moving video, where the
backdrop changes faster than the eye can calibrate. Measured over the same
glass, the steps land at 1.00, 0.79, 0.56 and 0.39 luminance — Lit, Strong,
Body, Quiet — each one plainly apart from the next.

The ladder had ten text roles on eight values 0x0d apart before this, because
each role had been tuned on its own, one careful nudge at a time. Five steps
cannot give ten roles a value each, and they should not have: **a new role
takes an existing step.** Roles that sit in different places, or that differ by
case, size or tracking, share one rather than wear a near-copy of it. A new step
is only admissible with 0x20 clear on both sides, and there is no room left for
one.

**The Fixed Floor Rule.** The ladder is built upward from Dim (`#ffffff80`), and
Dim does not move. The lowest step is what sets the worst-case contrast over a
bright frame, where the glass's absorption is doing its hardest work; every step
above it inherits its margin. Retuning the ladder means moving the steps above
the floor, never the floor.

## Typography

**Every role but one:** IBM Plex Sans, compiled into the executable in the
three weights the scale uses — Medium (500), 600 and 700 — rather than asked
of the system. **Timecode:** IBM Plex Mono Medium (500). There is no display
type anywhere in the product.

**Plain is 500, not 400.** At 400 the 13–14px lines that carry most of the
interface read thin over glass, where the picture behind eats into every
stroke. Plex's Text weight, 450, was tried first and was not quite enough;
Medium gives the rows body without looking bold. It does bring plain within a
step of the Title's 600, so Title and the playing row at 700 now lean on their
Lit alpha and their place as well as on weight.

**Character:** quiet, not anonymous; an instrument's lettering rather than a
brand's. The interface never speaks above the film, so the type is sized,
weighted and spaced for legibility over arbitrary moving content before
anything else. Plex was chosen over Geist and Atkinson Hyperlegible Next with
all three captured side by side on the shortcuts page, the playlist and the
bar. The episode code decided it. Geist draws a capital O and a zero alike, so
"S07E06" read "SO7EO6" in every title; Atkinson slashes every zero, which
settles the question at the cost of a list full of Ø. Plex tells its narrow
zero from its round O without a mark. It also sets a little narrower, so a
long title keeps a few more characters, and its tracked capitals look like
panel lettering, which is what the controls are. Bundled, it is the same face
on every platform the player runs on.

**Latin, and then the system.** The four files compiled in cover Latin and
nothing else, so a track named in its own script — العربية and فارسی sit in
the tracks panel of the episode this was last checked against — is drawn by
whatever the machine has. The words are right and the face is not ours: the
fallback sets smaller at the same size and on its own baseline, and on a
machine with nothing for that script it is tofu. Endonyms are the Capitals
Rule taken seriously, so the answer is not to romanise them; it is either a
font for each script the table names, which is several megabytes per script
in a binary that ships its own libmpv, or this, knowingly. It is this.

**The times are monospaced because they move.** In a proportional face the
elapsed time changes width as it counts, and a figure that shuffles sideways
once a second reads as motion in the one place the eye goes to check where
the film is. Plex Mono holds every digit to one width, and being Plex it is
the same family drawn differently rather than a second voice. It is only for
figures that tick — the two ends of the timeline and the time over the seek
preview. A playlist's running times, a slider's value and a shortcut's keys
stay in Plex Sans, where they sit still.

A weight the scale does not already use is a fourth file in the binary, not a
number to type; reach for 500 first.

### Hierarchy
- **Title** (600, 14px): the media title in the bar, the flash, and a fatal
  headline.
- **Body** (500, 14px): row labels, menu items, switch labels — the name of
  anything you can press. The current row in a list takes Lit at 700 instead.
- **Named** (500, 13px): a playlist heading that is a show's name rather than a
  section label. The Label role's weight and alpha and neither of its other
  habits: its own case, its own size, no tracking. Both fill the same slot at
  the top of a panel, so what separates them is what they *are* — the user's
  word or the interface's — and nothing else has to say it.
- **Readout** (500, 13px): figures — timestamps, slider and stepper values, a
  row's running time, a shortcut's keys — and the labels of the dense rows
  those sit in, which read as part of the same line rather than as menu entries.
- **Timecode** (500, 13px, Plex Mono): a readout that ticks — the elapsed and
  total times at either end of the timeline, and the time over the seek
  preview.
- **Hint** (500, 13px): prose added underneath something else — the drop hint,
  the line under a failure saying what usually fixes it, a driver's own error
  text.
  A readout's size under its own name, because prose is what would want a
  different one first.
- **Label** (500, 12px, tracked 1px): section headings and back rows, written in
  capitals.
- **Glyph** (500, 16px): a character standing in for an icon — the stepper's
  minus and plus, sized against the 26px disc rather than against the text.
- **Prompt** (600, 16px): the one thing the interface says louder than the
  rest, the words on the end-of-file pill, sized against the 24px glyph beside
  them.

The scale lives in the `Type` global in `app.slint`, not at its use sites.
Four sizes, nine roles, and several roles deliberately share a size: what
separates a title from a body line is weight, and what separates a readout from
a hint is that one is a figure and the other is prose.

### Named Rules

**One Weight For Headings.** Every heading in the system is plain (500),
whatever it holds. A show's name at 600 beside a kicker at 400 read as two different
designs in two panels — and it had also become a small copy of the playing row
rather than a heading of its own. A panel's top line is one slot; only the
thing in it should differ.

**The Label Must Not Out-Ink Its Content.** A section label is furniture; the
rows under it are the point. At 700 a kicker put down 34.9 lit pixels per
character against a body row's 25.6 — heavier ink than the thing it labelled,
while the alpha ladder claimed it was quieter — and the only cue left telling
them apart was the capitals. Capitals alone are not enough in a list of
nineteen. A label is told from its content by case, size, tracking, its icon and
the space above it; it wears its rows' own alpha, and it is never told apart by
being the boldest thing on the page.

Two things follow, both learned the hard way. **Tracking is the cue**: capitals
set solid read as a word, capitals with air between them read as a label. And
**lighter means plain**. Under Segoe UI, 600 was not a weight at all — it ships
Semibold as a family of its own, so 600 and 700 rasterised identically, to the
pixel — and that is why the scale steps from plain straight to its two bolds.
Plex carries both, so the Title at 600 and the playing row at 700 now sit
visibly apart; but a heading that needs to be quieter still goes to plain, not
to the step between.

**The Twelve Pixel Floor.** Nothing is smaller than 12px, and 12px is only for
capitals, which are read by their shape. Below that, white text over arbitrary
moving content stops being read and starts being recognised — which is fine for
a heading whose wording you already know and no use at all for a number that
changes. The scale was a step lower than this until every readout in the
interface was found sitting below even the size this document claimed for it;
naming the roles in one place is what keeps that from happening quietly again.

**The Capitals Rule.** The interface shouts its own words and never the user's.
SUBTITLES, AUDIO and EFFECTS are tracked capitals at 12/500 because they name
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
window — until centring it would bring it within 10px of the right pill, below
1060px, where it holds that gap and gives way leftwards — and a 380px pill on the right holding sound, effects and window
controls. They sit 24px from the window edges with 10px between the timeline
row and the pills. Inside a pill, 40px buttons sit 6px apart with 4px of
padding; text sitting directly on a pill is inset 18px, clear of the capsule's
curve.

The title pill hugs its title, up to the room the transport leaves, and
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
the remainder to the other. A list opens on its current row, centred as far as
its ends allow — the playlist on the episode playing, the tracks on the
subtitle in use — however it was opened. The hover label opens on that same
line, 12px above the timeline; the seek preview floats 10px above it.

**The middle of the picture** belongs to things that are about the film rather
than controls for it: the action flash, the end-of-file pill, and — with no film
loaded — the panel that offers a way in.

**Every length is arithmetic on constants and the window width** — never a
question asked of a layout. The glass rects are sampled once per frame to tell
the renderer where to draw, and a binding that depends on a layout forces a full
layout pass on every one of those reads.

**Density:** rows are 34px, headings 28px, switch rows 40px, stepper rows 38px,
slider rows 30px. A heading that follows a group is 44px — the same 28px with
the 16px group gap carried above it. The group gap is the row gap: the same
16px that insets every wash and pads every panel, so the space that separates
two ideas is the space the rows already stand in — and it is *derived* from
it, not typed again, because two literals asserting an identity is one of them
waiting to drift. Every number in this paragraph is named once, in the `Rows`
global, where the components that draw the rows can also reach it; the panel
arithmetic that sizes a page used to write 34, 30 and 38 out by hand, one
literal per page, under a comment warning against exactly that. So is the
12px a surface keeps from the next one — a panel above the timeline, the
hover label on that line, the flash's caption under its ring, the seek
preview clear of an open panel. One idea, one name. The window opens at 1280×720 and floors at
900×480, below which the title would have next to no room left; it never
reflows. The floor was once where the pills met, and it stayed put while the
right pill grew twice, until at 900px the volume track lay over Play.

### Named Rules

**The One Panel Rule.** Opening any panel closes the others. Two sheets of glass
over the same film is noise, and it doubles the refraction cost on the one
surface the design exists for. The hover label stands down while any panel is
open, for the same reason. With no film loaded the way in is the panel: the
tracks and the playlist describe a film and do not open, and the keys that act
on one — seeking, Home, the subtitle delay, the volume — do nothing, so no flash
lands on top of it either.

**The Bar Stays For A Pause.** The bar hides after three still seconds while a
film plays, and not while it is paused. Pausing is stopping to look at where you
are, which is the moment the bar is for. Nor while an open is in flight:
*Opening …* takes the title pill's place, and it is the only sign anywhere that
a folder on a share is still being read. The open is over when mpv finishes
opening the new file — not when there is a file, which over a film already
playing was true at once and cleared the line before it was ever drawn.

**The Bar Is A Floor.** While the bar is up, subtitles rest above it. The line
keeps its own place — the Position on the subtitles page — and rises only as
far as it must: to sit mpv's own bottom margin above the timeline, the same air
it keeps above the window's edge when nothing is in the way. It climbs on the
bar's own fade and settles as the bar goes. A line already above the timeline
does not move, and one resting in the letterbox comes all the way up. The case
that made this necessary is pausing to read a line, which holds the bar up over
exactly that line. The one exception is setting the Position: from its first
press the line goes down to where it rests, so the stepper moves what is being
looked at, and it comes back up when Timing or Size is pressed or the page is
left — those two are set by reading the line. The readout always shows where
the line was put, never where the bar has moved it.

**The Gap Goes Above Rule.** The 16px group gap always sits above the thing that
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

**What the material is, and what you want of it, are different questions.**
Three of the glass page's sliders answered the first and have stopped being
settings: the **bevel** (1.0 — the rim is the corner), the **index of
refraction** (2.0 — well past a window pane, short of the diamond that pulls a
black oval across the top of the way in) and the **reflection strength** (1.0 —
the Fresnel term is already the physical answer and there is no reason to scale
it). They are constants in `pipeline.rs`, and none of them had a range where
the result was a matter of taste rather than of whether the panes still read as
one material. What is left on that page — blur, refraction depth, fringe, sky,
tint — is how much of the material you want, which is the question worth a
slider.

**The fringe is a spectrum, not three copies.** The index of refraction is a
function of wavelength (Cauchy's law, anchored so the index above is the one
at the sodium d line), and the transmitted colour is integrated across the
visible band under the CIE 1931 observer and D65 — the illuminant sRGB white
already is. The Fringe slider is dispersive power, the reciprocal of an Abbe
number. It used to scale one offset three ways, red up and blue down, which
read as a coloured edge while subtle and as three coloured copies of the edge
past a pixel or two; it also had the order backwards, since short wavelengths
bend more. Now the band is sampled as finely as the smear is wide — one
wavelength across the flat middle of every panel, where the slope and so the
spread are zero, up to twenty four at the rim — and each sample's weight is
its exact share of the curve, so a flat backdrop comes through white at any
count. At Fringe 0 it is the old single ray, to within 1/255.

Fading a surface in fades its glass with it: opacity scales the shader's
coverage, so a panel arriving at a third of its opacity refracts a third as
hard. Glass at full strength beneath a half-faded surface reads as a hole in the
picture.

The glass runs whether or not a film is loaded, and needs something to be
glass over. Over a black frame refraction has nothing to bend and the rim
nothing to mirror, and with the tint at zero the way in's panel was not
faint — it was absent, three lines of text on black. So before a film is
loaded the pipeline draws **the backdrop** into the frame instead: one
seek-preview tile of the last film left unfinished, taken at the point you
stopped, covered across the window, softened past the point where its pixels
read, dimmed to 42% and falling away toward the corners. It arrives over 400ms,
the one arrival slower than a surface, because it is the window's light
changing rather than a control answering. Everything downstream treats it as
the frame, so the way in refracts it like any panel refracts a film.

With no unfinished film, or no atlas for it, **the mark lights the window
instead**: the logo, drawn into a 32×18 tile and given to the same pass, so it
is covered, softened, dimmed and fallen away exactly as a film's frame would
be. At that size the pass leaves no edge to read — photographed at 64×36 the
eye sockets and jaw still made a face glowing behind the panel, and a face is
the one thing this pass exists to soften away — so what reaches the glass is
the mark's coral as light, brightest behind the panel and falling off across
its rim, where refraction has something to show. A fresh install opens in this
state every time, and it used to open on black: three lines of white text with
the whole of what the product is for missing, at the one moment it is being met
for the first time.

The **one exception** to glass is *there is no glass to paint over*: the way in
carrying a fatal error, where every failure that raises it leaves the player
with no render context, so the glass it publishes is drawn by nobody. It then
carries a Soft wash fill (`#ffffff14`) at the panel radius. The same fill stays
as the fallback for a window with no backdrop at all, which the mark leaves
only for a logo that would not draw. Nowhere else. Everywhere else the material
is real, and a fill would cover the thing this product exists for.

### Named Rules

**The Proportional Rim Rule.** Bevel is a fraction of the corner radius, never a
count of pixels. The same absolute rim that looks right rolling around a 32px
panel corner swallows a 28px pane whole. One number therefore means the same
thing on every piece of glass, and a small pane simply has a small rolled edge —
which is also how real glass is made. It is 1.0: the rim runs the full width of
the corner, which is why a capsule is rim the whole way through.

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
- **Default:** transparent, glyph at Body
- **Hover:** Button wash, 45ms; glyph to Lit
- **Active:** glyph to Lit, and every button that sets it swaps its own glyph as
  well — muted for unmuted, slashed for unslashed. A wash behind an engaged
  button was tried and taken back out: it made the state read as chrome.
  **A button lights for a state you put it in, never for the state it came
  in.** Mute is off until you mute it, so Mute earns the Lit glyph; the
  ambient border is on to begin with, so lighting for that put the brightest
  mark in the right pill on a control nobody had touched, and spent the
  system's only emphasis on a default. It wears the swapped glyph alone,
  which is the channel that carries this anyway, and its tip says which way
  pressing goes.
- **Open:** a held Strong wash for as long as the panel it opened is showing — a
  separate channel from the lit glyph, so the two never share one highlight.
  A button that opens something does not also light for a state. The
  subtitles and audio button once did, and a glyph that lights like a switch
  is clicked like one — to turn subtitles off, and answered with a menu. It
  wears the plain subtitles mark and says *Subtitles & audio*; the menu's
  *Off* row is where subtitles go off.
- **Focus:** the Strong wash and the glyph at Lit, while the keyboard is on it.
  The wash alone is the one an open panel holds; the lit glyph is what says the
  keyboard is here. Tab walks the bar left to right while a film is loaded and
  nothing is open — then on to the end-of-file choices, or the credits pill,
  while either is up —
  and Enter presses; the arrows, Space and every other key keep their meaning
  wherever the ring stands, and Escape puts the ring away and does nothing
  else — it once left fullscreen in the same press, which threw a hand
  clearing a highlight out of the film. The hover label comes up for the
  focused button as it does for the pointer. A panel opened from the bar takes
  the ring, and hands it back to the button when it closes. The ring goes with
  the bar when the bar hides, unless it stands on a pill: left live under a bar
  nobody could see, Enter pressed whatever it was last on. Its place is kept,
  and the next Tab brings it back there.
- **Nowhere to go:** glyph at Dim, no wash, no pointer, and a label that says
  so — *No earlier file*, with no key beside it. Its key does nothing either,
  raises no flash, and Tab walks past it.

### Control pills
- **Shape:** capsule, height 48px (a 40px button with 4px of padding), radius 24px
- **Background:** none. The pill is a glass rect published to the renderer; a
  fill here would paint over the material.
- **Contents:** 40px buttons at 6px apart; the title pill's text is the Title
  role at Strong, inset 18px, eliding. At any speed but normal the speed rides
  beside the title at Quiet in Readout size — *1.5×* — for as long as it holds:
  a caption that came and went was the only sign a film was running fast.

### Timeline row
- **Shape:** one 28px capsule of glass across the bar's full width
- **Times:** Strong in Readout size, centred in 60px each — wide enough for
  `1:23:45`, so the row does not shift when a film passes the hour
- **Track:** 5px at Rail, growing to 7px under the pointer or a drag (100ms
  ease-out); played length at Played, width-driven so it stays crisp
- **Hit area:** the full row height, not the track it draws — a 5px target is
  miserable to grab. The wheel over it seeks.
- **Chapters:** a dot at each chapter start, the unity dot's shape and its
  rule — Knob where the played length covers it, Quiet ahead of it. None at
  the very start, which is the start rather than a boundary, and none at all
  for the many files without chapters.

### Volume
- **Track:** 96px, 5px at Rail growing to 7px (100ms ease-out), filled at Fill
  across mpv's full 0–200% range
- **Unity dot:** a dot, not a line, at 100%. A line the height of the track
  divides it into two ranges; a dot is a landmark on one. It reverses against
  the fill the way the switch knob does: Knob on the fill, Quiet off it.

### Menu rows
- **Shape:** 34px tall; the wash is inset 16px on both sides at a 16px radius,
  full row height so adjacent washes meet
- **Text:** Body, starting 30px from the panel edge, eliding 26px from the right
- **Hover:** Soft wash, 45ms
- **Focus:** Strong wash, shown only while the keyboard is in use — never a ring.
  A pointer moving over a different row stands it down, so a panel never shows
  two lit rows
- **Active:** Lit at 700, and the active mark: a 3×14 capsule in Lit, 20px from
  the panel edge, inside the wash so a row that is both current and focused
  wears both marks without them touching. A shape, because weight and alpha
  alone were close to invisible over moving video.

### Playlist rows
- **Shape:** a menu row with two more facts on it
- **Running time:** Quiet in Readout size, right-aligned so a column of them
  reads as a column; empty for a file never opened and not yet described.
  A file that would not play says *Could not play* there instead, without the
  measure, for the rest of the run
- **Order:** the order a person watches in. A show's files by season and
  episode, kept together where the first of them falls by name; everything
  else by name with digit runs read as numbers and case ignored. Two release
  groups' names used to split one show around another. Autoplay, Next and the
  end of a season all trust it
- **Title:** elides clear of the time and the measure, never under them

### The playlist's parts

A season of one show is shown as it always was, one list, with the show and the
season as its heading: *Game of Thrones · Season 7*. A list holding more than that is shown in
parts, on two levels, the way the settings are — **including the way they
move.** Opening a part and coming back out is the same 220ms drill: the page
being left goes out the side the new one comes in from, and the panel grows or
shrinks to the new page's height under them both, keeping its bottom edge on
the line every panel sits on. One list becoming another in a single frame said
only that everything was different.

Both pages are built whether or not they are showing, which the settings pages
are not: those hold their lists behind a condition, and a named element inside
one cannot be reached from the key handler at the window. These two are asked
to put a row in the middle from four places, and the list of parts is short by
construction — a show's seasons, not the files in them.

- **The list of parts** comes first. A list spanning seasons is its seasons —
  *Specials*, *Season 1*, *Season 2* — and a folder of several shows is each
  show's seasons, named with the show: *Frieren · Season 2*. Anything that is
  one file on its own, a film or a stray episode, stands among them as a
  playlist row and plays from there.
- **A part's row** is a drill row with a count: its name, *10 episodes* at
  Quiet in the running times' column, then the chevron. No measure: a season's
  share watched, summed from its episodes, said nothing its own page does not
  say better. The part holding the file playing wears the active mark. File
  rows on a page with parts leave the chevron's column empty, so the columns
  line up.
- **The season is said once.** Under a heading or a way back that names it, an
  episode is *E03 · The Long Night*, not *S08E03 · The Long Night* on every
  row. A file standing among other things keeps its whole name.
- **A part's page** is headed by its way back: a back row reading *SEASON 7*,
  or a show's name in its own case, in the Named role, where the part carries
  one.
- **The seasons beside.** When the list is one season in one folder, the
  seasons of the same show in folders beside it are parts too, in order among
  it. They are not queued — opening an episode means its season, and autoplay
  stays in it — but they can be read and started from: an episode picked there
  opens its season at that episode.
- **Where it opens:** on the page of the part holding the file playing, at its
  row. A page opened from the list lands where watching would carry on — the
  file playing, or else the first not finished. Escape goes back a page before
  it closes the panel. Home and End go to the first and last row and PgUp and
  PgDn a page at a time, here and in the tracks, stopping at the ends; with a
  list open they are the list's keys, not the film's — **every list, including
  the one with no row to land on.** See Shortcut rows.

### The resume measure

A 44×4 capsule on a playlist row, at Rail with a fill at Strong, sitting on
the row's own line a group gap (16px) before the running time's column — 48px,
an hour-long time's width, so a season's measures stand in one column however
the digits beside them run. It says how far into
that file you got, and appears only when there is something to say: a resume
point, or a file **finished** — watched to its end or into its credits — which
wears the measure full. mpv deletes a position once a file is watched through,
and a scan gives every file a length, so without a record of its own the player
drew a finished episode exactly like one never started. Started again, the row
goes back to saying how far.

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
  2px top and bottom; the thumb a capsule at Quiet, proportional to how much of
  the list is visible, with a 24px floor
- **Role:** a hint, not a control. It says there is more and roughly where you
  are; the wheel, the flick and the keyboard move the list. The keyboard scrolls
  by the least that reveals the focused row.

### Sliders
- **Row:** 30px; label 78px at Body in Readout size, value 46px right-aligned at
  Dim
- **Track:** 4px at Rail, growing to 6px on hover or press (100ms ease-out) —
  a step lighter than the bar's 5px tracks, because these sit inside a panel of
  rows rather than alone on glass
- **Fill:** width-driven from the left at Fill, never clipped
- **Readout:** precision taken from the parameter's *range* rather than its
  value — three decimals on a 0–0.1 edge blur, one on a 0–40 blur
- **Focus:** the row takes the Strong wash; the sliders have no hover wash,
  since the growing track already answers the pointer

### Steppers
- **Row:** 38px; an 18px icon at Quiet, the label at Body in Readout size, then
  two 26px discs flanking a 62px value at Strong
- **Discs:** Soft wash at rest, Strong wash on hover (45ms); `−` (U+2212, to match
  the digits) and `+` in Glyph size at Strong
- **Use:** for quantities read as numbers rather than positions — a subtitle
  delay is +0.30 s and the useful move is one step. Sliders are for material
  parameters, where the eye judges the result and the number is incidental.

### Switches
- **Row:** 40px; switch first, label after it at Body, 12px apart — the rows'
  own step, because a switch sits under a list as a setting *about* it, and at
  Strong the autoplay row out-inked the episodes it governs
- **Shape:** 34×18 capsule, 14px knob inset 2px
- **On:** track at Fill, knob at Knob; **off:** track at Rail, knob at Fill —
  dark on the lit track, light on the dark one, so the knob reads as a hole
  punched through it either way
- **Motion:** 120ms ease-out on the knob, 120ms on the track colour
- **Hover:** Soft wash; one target for the whole row

### Drill rows
- **Row:** 34px; an 18px icon and the label, both at Body, 10px apart, with a
  chevron at the end — Quiet at rest, Lit on hover and under the ring. The ring
  lights the icon with the words, since both name the same subject
- **Use:** every row of the settings tree. An effect that can be turned off says
  so with its glyph rather than with a switch, because a switch beside a chevron
  is two things to press in one row.

### Back rows
- **Row:** 34px, the page's heading and the way out of it at once
- **Text:** Label role at Body, starting on the row-label line; Lit on hover,
  over a Soft wash
- **Chevron:** 18px, in the icon column, laid out exactly as a drill row lays
  out its icon and with the same air after it. A back row is the way out of a
  page of those, and the pair read as one movement only when they are built
  the same. It hung in the margin at 14px once, which did line the label up
  with the headings — and left the arrow pinched between the panel edge and
  the first letter, which is the cost the eye actually notices. That was the
  last icon in the system at any size but 24 or 18, so the exception the icon
  rule used to carry is gone with it.

### Panels
- **Shape:** 32px radius, 16px padding, no background of its own
- **Entry:** fade and 6px rise over 120ms ease-out, glass fading with the surface
- **Headings:** tracked capitals at Body, except a playlist heading carrying a
  show name, which takes the Named role — natural case, no tracking, and the
  same alpha and weight. A heading with a group above it carries the 16px gap
  itself rather than being pushed down by the row above, because the last row
  of a group does not know that it is last
- **Heading icons:** a heading over rows without icons of their own carries
  one — the playlist, SUBTITLES and AUDIO in the tracks panel, every group on
  the shortcuts page. 18px, a row's size: icons are shown at 24px or 18px and
  nothing between (see Don'ts); at the heading's own alpha, so mark
  and words are one thing; starting on the row-label line with the words 8px after
  it. It cannot hang in the margin as the back row's chevron does: that margin
  is 14px to the edge of the wash. The settings page's headings have none,
  because every row under them already wears one
- **Trailing rows:** a reset or a setting *about* the list gets a 16px gap above
  it. Without it the eye reads one more entry and only the wording says otherwise.
- **Resets:** two presses, then a way back. The first changes the row to *Press
  again to reset* for 2.5 seconds; the second resets, and the row becomes *Undo
  reset* until the page is left. Settings always save, so a reset is on disk
  within a second, and a look tuned by eye cannot be typed back in. The ring
  stops at the ends of a page that has one rather than wrapping, and Up from a
  hidden ring lands on the last control, never on the reset.
- **Shortcut rows:** Body label, keys at Quiet in Readout size, 12px apart. On
  the reference page they are inert — no wash and no pointer, because a wash
  under a row that does nothing when clicked is an offer the row cannot keep.
  The page lists what the pointer does as well, under POINTER — click, double
  click, the two wheels and, where the platform has one, a drop — because it is
  the one reference the player has
- **A page that is read, not walked.** Every row of the reference is a
  statement, so the ring has only the way back to stand on and the arrows have
  nowhere to go. They move the list instead: ↑ and ↓ by a row, PgUp and PgDn by
  a screenful less a row, Home and End to the ends. This is not a nicety. The
  keys were falling through to the film underneath — Home restarted it, PgUp
  and PgDn jumped its chapters — so the one page that documents those keys was
  the one page where pressing them did something else, and reaching it with
  `?` and then pressing Home threw the film back to 0:00. **No key belonging
  to a list may reach the film while a panel is open**, whether or not that
  panel has a use for it; on a page with none, doing nothing is the answer
- **The keys beyond the bar** are mpv's own, so the habit carries over:
  Shift with an arrow for a one-second seek, `[` `]` and Backspace for speed
  on a fixed ladder (0.25× to 4×), `,` `.` for a frame, PgUp PgDn for chapters,
  J and Shift+J through the subtitle tracks, # through the audio, and `?`
  straight to this page, which is also a row on the way in. The bindings
  the player already had — the arrows' volume, S, T, A — stay where they were

### The hover label

A 28px capsule of glass that names the control under the pointer and gives the
key that does the same thing. The name is Body, the key is Quiet at Readout
size, 12px apart with 12px of padding — the two roles of a shortcuts row,
unchanged, because the tip is that list arriving one row at a time where the
question was asked.

- **500ms before it opens**, then a fade and a 6px rise like any other surface.
  Long enough that crossing the bar raises nothing; short enough that stopping
  on a glyph feels answered rather than eventually attended to.
- **Then the bar is being read, and waits no more.** While one is up, or for
  300ms after it goes, the next control's name replaces it in place with no
  delay and no fade — a label, a label, not a label, nothing, a label.
- **A fade is of what was there.** The name, key and position a tip rose with
  stay put while it goes, rather than turning into the next button's or into an
  empty pill around a key.
- **Pressing puts it away**, and it comes back on the next arrival; a name
  hanging over the result is in the way of seeing it. A resize takes it at
  once, because every control has moved out from under it.
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
timestamp alone — a 74×26 capsule — when no atlas exists, which is the state it
sits in for the first seconds of every unseen file and permanently on a machine
with no ffmpeg.

### The action flash
An answer, not a control: a 24px glyph in Lit inside a 56px circle of glass,
and 12px under it, when there is one, a 28px capsule
carrying the one number that has nowhere else to appear — Title at 600 in Lit,
14px of padding, never wider than the bar. It lives 700ms, long enough to read a
two-character figure and short enough that a held key flickers rather than
strobes.

**A seek is thrown to the side it seeks to** — a fifth of the way across for
back, four fifths for forward — and everything else sits in the middle. The
glyph already points the right way, but an arrow held down is read where it
lands rather than looked at, and a mark that arrives on the left and one that
arrives on the right are told apart before either is a shape. Clamped inside
the panel margins, so the throw shortens on a narrow window rather than
taking the ring off the edge of it.

A **figure** replaces the ring with its words alone, for 1200ms: the speed
after `[`, `]` or Backspace, the chapter after PgUp or PgDn, the track after J
or #, and after C whether subtitles are showing at all, which the subtitles
button stopped saying when it stopped being a switch. The icon set has no glyph for any of them and the figure is the whole
answer. A frame step and the one-second seek raise nothing — the picture and
the timeline already answer.

A **notice** from the player is a sentence alone, centred, reason first —
*Unrecognized file format — could not play Game of Thrones · S07E06* — because
the capsule elides at the bar's width and the reason is the part worth
keeping. It holds for 3200ms and then until the next key or pointer event, so
a failure glanced away from is still there. No part of any of it takes the
pointer: it sits where a click means play or pause.

### The end-of-file pill
The bar's own 48px capsule, in the middle of the picture where the film
stopped: a 24px glyph and the name of what comes next — *E04 · Slabtown*, in
the words its playlist row uses — in Prompt at Lit, 12px apart, inset 18px. It
grows with the name to half the bar's width, and past that the name elides; a
film's can run to a sentence, and this is a capsule, not a banner. Into
another part the part goes first — *Season 6 · E01 · No Way Out* — because an
E01 on its own could be anybody's. *Next* is what is left when the list cannot
say. Button wash on hover. It is the
one surface not tied to the idle clock: the bar hides after a few still seconds,
and this is what should still be there when it does.

It takes the keyboard's ring when it appears, as the credits pill does, so
Enter at the end of anything is the way on, and the ring goes when the pill
does. → goes on too — the next file, or the next season when one was found —
but never to the dialog *Open folder…* raises: an arrow held a moment too long
is not a press deliberate enough to put one up.

At the end of the last file — the end of a season, or of a film alone in its
folder — it offers two things, the way on first and *Restart* second.

**The way on is always named, where the player can name it.** This is the
loudest the interface ever speaks, and it is the last thing it says about a
film; spending that on a verb while the player still knows something to call
the thing is the one ending worth getting right. That holds mid-season as much
as at the end of one — the rule once stopped at the end, and said *Next* over
an episode whose name was on the playlist's next row. At the end of the last
file, in order: the next season, by name — *Season 8* — when a folder beside this one
holds the same show at a later season; failing that, the newest other film
still unfinished, by name, which is the same film the way in offers to
continue and the only other thing the player can name without being told; and
only when it can name neither, *Open folder…*, opening on the shelf above this
file's folder. A noun in every case, as *Season 8* is: the thing you are going
to, not a verb about going there.

The credits pill does **not** take the second of those. It is the offer to
skip the credits into the next episode, and only another episode of the same
show can honour that; another film is an honest thing to offer over a file that
has ended and a dishonest thing to cut a film's credits short for.

Restart alone was the least likely thing anyone wanted there. With two,
each choice is a 40px capsule of its own, set 4px inside the pill (24 − 4 = 20,
concentric) with its words still 18px from the pill's edge, 6px apart, each
taking the Button wash on its own and the Strong wash under the keyboard.

### The credits pill
While an episode's closing credits play, the end pill's single capsule offers
what comes next with the Next glyph, named as the end pill names it — the next
file as its row says it, or at the end of a season the season after it — and
*Next episode* only when neither can be said. It is no wider than the bar's
right pill, the corner it keeps to; a long title elides rather than reach over
the names. The credits are a chapter named for them —
*End Credits*, *Credits*, *Outro*, *ED*, *Ending* — in the file's second half,
and the pill stays from there to the end. Where no chapter has a real name —
each titled with its own start time, *Chapter 7*, or nothing, as broadcast
episodes split at their act breaks usually are — the last chapter is the
credits if it runs three minutes or less. A file without chapters, or a film,
never gets one. It needs somewhere to go: a later file, or at the
end of a season the next season found beside it.

It keeps out of the middle, where the names roll, and sits at the bottom right
on the bar's lines: at the 24px margin while the bar is away, lifted 12px above
the timeline row while it is up, riding the bar's own fade between the two.
Its arrival wakes the bar once and holds nothing up — the clock runs out, the
bar leaves, the pill stays, and doing nothing is watching the credits. It also
takes the keyboard's ring, so Enter is the whole of going on; the ring stays
with it after the bar leaves and goes when it does, so Enter never presses a
pill that has gone. Like the
end pill it is not tied to the idle clock. It stands aside for an open panel
and for the seek preview when the preview reaches its column; a hover label
that would land on it goes up a row instead. Glass never lies on glass.

### The way in
With no film loaded, a panel of glass centred in the window, over the backdrop.
With nothing to continue, it is headed with the player's own name — *Death by
MPV*, in the Named role, where a show's name goes over Continue — because then
the panel is about the player rather than a film, and the first window anyone
ever sees used to say *Open file…* without saying what was asking:

- **Continue**, first, when there is a film to continue: a play glyph at Body,
  the film's name at Strong in Body size — the one line on the window about a
  film rather than the player — then the resume measure and, where a playlist
  row keeps its running time, what is left at Quiet: *43 min left*. It is the
  newest file mpv still holds a position for that is still on disk; the same
  thing the backdrop is a frame of. A group gap follows it, because it is a
  film and the rows below it are dialogs, and the panel widens from 320px to
  420px so an episode keeps its number beside the time. An episode's show goes
  on a heading line of its own above it, in the Named role, where a playlist
  puts it: sharing the row, the show's name took the room and the episode
  number was what elided. A film watched into its credits is not offered.
  Enter presses it at once, without first raising the ring: it is the first
  row, and the one the window is opened to get back to.
- **Open file, Open folder, Settings and Shortcuts** as shortcut rows with their keys, then
  a line at Quiet saying a file can be dropped anywhere — **on the platforms
  where one can.** The drop target is Windows-only; Linux would be a different
  implementation against a different protocol and does not have one yet. Both
  places that offer a drop — this line and the POINTER row on the reference
  page — are conditioned on it, and where there is none the line has nothing to
  say at rest and takes no height. *Guess well, and never lie about it* is a
  product principle, and a printed instruction that does nothing is the
  plainest way to break it. **The gap goes with the line.** A note carries its
  own 16px above it, so a window with nothing to add leaves its last row the
  same air the heading has above it — and not, as it did, sixteen pixels of
  glass below the thing that ended, which is the Gap Goes Above Rule read
  backwards.

While an open is in flight Continue and the two open rows fade to half and stop
answering — to the pointer and to every key — and the line names what is
arriving; Settings stays at full strength, because it still works. Carrying a
failure it widens to 420px and offers nothing to open: a Title headline at
Strong, the driver's own error text at Body in Hint size wrapping to three
lines, a line at Quiet saying what usually puts it right — a driver update,
except for shaders the driver would not build, where a current driver means
the bug is the player's, and asked for on its own account rather than
alongside the drop line, which it shared until the drop line learned to
disappear and took it along — and after a
group gap two rows — *Copy details*, which puts the headline and the driver's
words on the clipboard and says *Copied* for two seconds, and *Close the player*.

## Do's and Don'ts

### Do:
- **Do** derive every length from the constants at the top of the component.
  Panel geometry is read once per frame by the renderer; a binding that asks a
  layout for an answer costs a full layout pass each time.
- **Do** take every colour from the `Paint` global. A new role gets an existing
  step; a colour written at its use site is how the ladder drifted last time.
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
- **Do** open a list on its current row, and make a destructive row take two
  presses and offer itself back.
- **Do** keep the accessible roles and labels Slint makes cheap — an icon
  button's is its tip, a row's is its words — and promise nothing past them.
  Screen readers are not a goal of this player: the keyboard's ring is a wash,
  not focus a reader can follow, notices are not announced, and no change is
  checked with one.
- **Do** take font weights from `Type` — plain, title, current — not literals.

### Don't:
- **Don't** add a hue. Not an accent, not a semantic red, not a brand colour.
  The film is the only source of colour on screen.
- **Don't** add a text step between two existing ones, or move Dim. There is no
  gap 0x20 wide left to put one in, and the floor is the contrast margin
  everything else stands on.
- **Don't** give a panel a background fill. The glass beneath it is the surface;
  a fill paints over the thing this product exists for. The way in is the one
  exception, where there is no glass to paint over: carrying a fatal error,
  with no render context to draw it. A window with no film to light it is not
  that case — the mark lights it, and the way in is glass. See Elevation &
  Depth.
- **Don't** draw a border, a ring or a hairline. There is no outline anywhere in
  this interface, and a focus ring would make the keyboard the only thing that
  draws one.
- **Don't** let it drift toward the utilitarian player — VLC, MPC-HC, mpv's own
  OSC. Grey chrome, system widgets and dense toolbars are the failure mode that
  function-first reasoning always argues for, and the reason this was rebuilt.
- **Don't** open two panels at once.
- **Don't** animate for its own sake. Motion here is a response: 45ms for a
  wash, 100ms for a track, 120ms for a surface or a switch, ease-out, no bounce,
  no overshoot. Four things depart from that, each for a reason that is about
  what is moving rather than about how it looks:
  - the **title pill** resizing to a new title over 160ms ease-in-out, so a
    file change is a movement rather than a flicker;
  - the **backdrop's** 400ms arrival, the one motion that is not a response to
    anything — it is the window's light changing, not a control answering;
  - a **drill**, 220ms ease-in-out, moving the page sideways and the
    panel's own height with it. Longer than a surface because it is a journey
    rather than an arrival, and eased at both ends because it starts and stops
    in place rather than coming from nowhere. The panel and the page are one
    number, so the box and what is in it arrive together. **Both trees move,
    and by the same number**: the settings pages behind their rows, and the
    playlist between its list of parts and a part's page. A panel with two
    levels that changes which one it shows in a single frame leaves no
    account of what happened, and that is as true of a season as of a page of
    sliders;
  - the **seek preview's** fade, 60ms — half the shared duration, and the one
    surface with a number of its own. It is not announcing itself; it is
    following a pointer, and at 120ms it read as trailing the hand rather than
    belonging to it.
- **Don't** substitute an icon library. The set is 60 SVGs drawn at 24×24 as
  one family; extend it rather than replacing it.
- **Don't** show an icon at a size other than 24px or 18px. Scaled from the
  24×24 drawings, those give a 2px and a 1.5px line and both stay legible; at
  16px the line falls to 1.33px, and at 12px every line lands on a pixel
  boundary and turns to two grey rows, so detailed icons blur into blobs. A
  smaller size needs its own drawing. There are **no** exceptions: the back
  row's 14px chevron was the only one and it is now 18px, for a reason that
  had nothing to do with this rule and happened to settle it.
