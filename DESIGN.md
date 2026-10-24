---
name: Death by MPV
description: A video player whose interface is panes of real glass laid over the film.
colors:
  lit: "#ffffff"  # Paint.lit
  strong: "#ffffff"  # Paint.strong
  body: "#ffffff"  # Paint.body
  quiet: "#ffffffa0"  # Paint.quiet
  dim: "#ffffff80"  # Paint.dim
  played: "#fffffff2"  # Paint.played
  fill: "#ffffffcc"  # Paint.fill
  rail: "#ffffff2e"  # Paint.rail
  knob: "#101010e6"  # Paint.knob
  ground: "#000000"  # Paint.ground
  wash-ring: "#ffffff54"  # Paint.wash-ring
  wash-strong: "#ffffff34"  # Paint.wash-strong
  wash-button: "#ffffff24"  # Paint.wash-button
  wash-soft: "#ffffff14"  # Paint.wash-soft
typography:
  title:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "14px"  # Type.title
    fontWeight: 600  # Type.weight-title
  body:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "14px"  # Type.body
    fontWeight: 500  # Type.weight-plain
  named:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "13px"  # Type.named
    fontWeight: 500  # Type.weight-plain
  readout:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "13px"  # Type.readout
    fontWeight: 500  # Type.weight-plain
  hint:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "13px"  # Type.hint
    fontWeight: 500  # Type.weight-plain
  timecode:
    fontFamily: "IBM Plex Mono"  # Type.numerals
    fontSize: "13px"  # Type.readout
    fontWeight: 500  # Type.weight-plain
  label:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "12px"  # Type.label
    fontWeight: 500  # Type.weight-plain
    letterSpacing: "1px"  # Type.label-track
  glyph:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "16px"  # Type.glyph
    fontWeight: 500  # Type.weight-plain
  prompt:
    fontFamily: "IBM Plex Sans"  # Type.face
    fontSize: "16px"  # Type.prompt
    fontWeight: 600  # Type.weight-title
rounded:
  row: "16px"  # Rows.radius
  panel: "32px"  # Rows.panel-radius
  pill: "24px"  # MainWindow.pill-radius
  button: "20px"  # MainWindow.btn / 2
  preview: "18px"  # MainWindow.preview-radius
  preview-tile: "9px"  # MainWindow.preview-tile-radius
  window: "15px"  # window.CORNER_RADIUS
spacing:
  hair: "4px"  # MainWindow.pill-pad
  tight: "6px"  # MainWindow.btn-gap
  group: "16px"  # Rows.group-gap
  offset: "12px"  # Rows.offset
  gap: "16px"  # Rows.gap
  margin: "24px"  # MainWindow.panel-margin
  trail-x: "26px"  # Rows.trail-x
  label-x: "30px"  # Rows.label-x
components:
  icon-button:
    size: "40px"  # MainWindow.btn
    rounded: "{rounded.button}"
    textColor: "{colors.lit}"
    backgroundColor: "transparent"
  icon-button-hover:
    backgroundColor: "{colors.wash-button}"
  icon-button-open:
    backgroundColor: "{colors.wash-strong}"
  icon-button-focus:
    backgroundColor: "{colors.wash-ring}"
  icon-button-disabled:
    textColor: "{colors.dim}"
  bar:
    height: "94px"  # MainWindow.bar-h
    rounded: "{rounded.panel}"
    padding: "8px"  # MainWindow.bar-pad
    backgroundColor: "transparent"
  button-row:
    height: "48px"  # MainWindow.pill-h
    padding: "4px"  # MainWindow.pill-pad
  window-buttons:
    height: "34px"  # MainWindow.window-pane-h
    rounded: "17px"  # MainWindow.window-pane-h / 2
    padding: "3px"  # MainWindow.window-btn-pad
    backgroundColor: "transparent"
  window-button:
    size: "28px"  # MainWindow.window-btn
    rounded: "14px"  # MainWindow.window-btn / 2
    textColor: "{colors.lit}"
    backgroundColor: "transparent"
  window-button-hover:
    backgroundColor: "{colors.wash-button}"
  timeline-row:
    height: "30px"  # MainWindow.timeline-h
    textColor: "{colors.strong}"
    typography: "{typography.timecode}"
  timeline-time:
    padding: "0 13px"  # MainWindow.glyph-inset
    textColor: "{colors.strong}"
    typography: "{typography.timecode}"
  menu-row:
    height: "34px"  # Rows.row
    padding: "0 26px 0 30px"  # Rows.trail-x, Rows.label-x
    textColor: "{colors.body}"
    typography: "{typography.body}"
    backgroundColor: "transparent"
  menu-row-hover:
    backgroundColor: "{colors.wash-soft}"
    rounded: "{rounded.row}"
  menu-row-focus:
    backgroundColor: "{colors.wash-ring}"
    rounded: "{rounded.row}"
  menu-row-active:
    textColor: "{colors.lit}"
  section-label:
    height: "28px"  # Rows.heading
    padding: "0 26px 0 30px"  # Rows.trail-x, Rows.label-x
    textColor: "{colors.body}"
    typography: "{typography.label}"
  section-label-named:
    height: "28px"  # Rows.heading
    padding: "0 26px 0 30px"  # Rows.trail-x, Rows.label-x
    textColor: "{colors.body}"
    typography: "{typography.named}"
  slider-row:
    height: "30px"  # Rows.slider-row
    padding: "0 26px 0 30px"  # Rows.trail-x, Rows.label-x
    textColor: "{colors.body}"
    typography: "{typography.readout}"
  stepper-button:
    size: "26px"  # StepButton.width
    rounded: "13px"  # StepButton.width / 2
    backgroundColor: "{colors.wash-soft}"
    textColor: "{colors.strong}"
  stepper-button-hover:
    backgroundColor: "{colors.wash-strong}"
  toggle-track-on:
    width: "34px"  # ToggleRow.toggle.width
    height: "18px"  # ToggleRow.track.height
    rounded: "9px"  # ToggleRow.track.height / 2
    backgroundColor: "{colors.fill}"
  toggle-track-off:
    width: "34px"  # ToggleRow.toggle.width
    height: "18px"  # ToggleRow.track.height
    rounded: "9px"  # ToggleRow.track.height / 2
    backgroundColor: "{colors.rail}"
  hover-label:
    height: "28px"  # MainWindow.tip-h
    rounded: "14px"  # MainWindow.tip-h / 2
    padding: "0 12px"  # MainWindow.tip-pad
    textColor: "{colors.body}"
    typography: "{typography.body}"
  end-pill:
    height: "48px"  # MainWindow.end-h
    rounded: "{rounded.pill}"
    padding: "0 18px"  # MainWindow.end-pad
    textColor: "{colors.lit}"
    typography: "{typography.prompt}"
  end-pill-hover:
    backgroundColor: "{colors.wash-button}"
  end-choice:
    height: "40px"  # MainWindow.end-h - 2 * MainWindow.pill-pad
    rounded: "20px"  # (MainWindow.end-h - 2 * MainWindow.pill-pad) / 2
    padding: "0 14px"  # MainWindow.end-pad - MainWindow.pill-pad
    textColor: "{colors.lit}"
    typography: "{typography.prompt}"
  resume-row:
    height: "34px"  # Rows.row
    padding: "0 26px 0 30px"  # Rows.trail-x, Rows.label-x
    textColor: "{colors.strong}"
    typography: "{typography.body}"
  seek-preview:
    rounded: "{rounded.preview}"
    padding: "9px"  # MainWindow.preview-pad
    textColor: "{colors.strong}"
    typography: "{typography.timecode}"
---

# Design System: Death by MPV

## Overview

**Creative North Star: "The Cinema Window"**

Panes of real glass, laid over the film. Not a metaphor for glass — the
interface is genuinely refractive: mpv's frame is rendered into the
application's own GL context as a texture, and every surface bends the picture
behind it through a superelliptic box's distance field, a dome profile and
Snell's law. Thickness, bevel and index of refraction are real
parameters, and the rim behaves like a rim because it is one.

The consequence is that this system has almost no palette. Colour comes from
whatever is playing. The interface is white at a handful of alphas and full
white, a single near-black switch knob, and — before anything is loaded — the
dimmed light of the last film left unfinished. Even the
letterbox is not neutral: the ambient border extends the frame's own edge
colours outward, so a red scene bleeds red into the bars. Nothing here is tinted
by a brand.

Controls behave like instrument switches: definite, short, slightly firm. A
hover wash lands in `Motion.highlight` and surfaces arrive in `Motion.surface`
with a few pixels of travel, fast enough that the movement registers as a
response rather than an animation. Density is generous: `Rows.gap` on all four
sides of every row, and panels that hold at most a heading and a list.

**Key Characteristics:**
- Real per-pixel refraction, not a translucent blur panel
- No palette of its own; colour is the film's
- Text on five alpha steps, 0x20 apart, from a floor that does not move; the
  ordinary run of it shares the second step from the top
- One radius rule: every corner nests concentrically inside its parent
- Motion is short, eased out, and never bounces
- Uppercase for the interface's own labels, natural case for the user's content

## Colors

A single hue — white — at an alpha ladder, over live video. The ladder *is* the
hierarchy; there is no second colour to promote anything with. Every colour
is named once, in the `Paint` global in `theme.slint`, and nowhere else in the
interface is one written out. The values are in the front matter.

### Primary
- **Lit** (`Paint.lit`): the active state, and where the hand or the keyboard
  is. A selected track, the playing episode, a hovered chevron, the words of
  the row the keyboard is on, the flash and the end-of-file prompt; and every
  bar glyph that can be pressed, since over a bright frame anything less reads
  as greyed out. Full white is the system's only emphasis.

### Neutral

**Text and glyphs**: steps 0x20 of alpha apart, built upward from the floor,
Dim. The top step falls one short: Lit is full white, and 0x20 above Strong
does not exist in eight bits. Every step that can keep the rule does. Words use
only the top two, Lit and the step Strong and Body share; the two under them
are marks (see The Bright Text Rule):

- **Strong** (`Paint.strong`): text that leads. The media title, the elapsed
  and total times, the timestamp under a seek preview, a stepper's value and
  its minus and plus, a fatal headline, and the fill of a resume measure.
- **Body** (`Paint.body`, Strong's step): every other word. Row labels and
  menu items, switch labels, a hover label's name and key, a slider's label and
  value, a stepper's label, a driver's error text; every section heading and its
  icon, and a show's name in the same slot; back rows and drill rows; and what
  sits beside or under any of them: a shortcut's keys, a playlist row's running
  time, a search result's language, the speed beside the title, the prose hints,
  and text stating an absence. It shares Strong's step because it is the text
  there is most of, and a step lower read grey over a bright frame; a role moves
  on this ladder onto an existing step, never between two. Leading text is told
  from it by place, size and weight, and what sits beside it by place and size.
- **Quiet** (`Paint.quiet`): a mark on or beside a track. A chapter's dot ahead
  of the played length, the unity dot where it sits off the volume fill, a
  scroll thumb. Never text.
- **Dim** (`Paint.dim`): the glyph of a bar button with nowhere to go
  (Previous on the first file, Next on the last) and nothing else: the one
  mark meant to look greyed out. **The floor**: see below.

**Marks:**

- **Played** (`Paint.played`): the played length of the timeline — the one
  near-solid white that reads as a measurement rather than as text.
- **Fill** (`Paint.fill`): the filled length of the volume track and every
  settings slider, a lit switch track, and an unlit switch's knob.
- **Rail** (`Paint.rail`): the unfilled length of every track, switch and
  measure.
- **Knob** (`Paint.knob`): the only non-white in the system. A dark disc on a lit
  switch track, so the knob reads as a hole punched through it; the unity dot
  where it sits on the fill.
- **Ground** (`Paint.ground`): the window before a frame exists.

**Washes** — four, and the three a row or a bar button can wear sit 0x20 apart:

- **Ring wash** (`Paint.wash-ring`): where the keyboard is, on a row, a bar
  button or one of a pill's choices. It always stands on a pane of glass of
  its own, a disc on a bar button and the row's or the choice's own shape
  elsewhere (see The Ring Pane Rule).
- **Strong wash** (`Paint.wash-strong`): the held background of a button whose
  panel is open; a hovered stepper disc or remove disc.
- **Button wash** (`Paint.wash-button`): a hovered icon button, and a hovered choice on
  the end-of-file pill. Never on a row, so it never has to be told apart from
  the washes either side of it.
- **Soft wash** (`Paint.wash-soft`): the inset highlight behind a hovered row of any
  kind; a stepper disc at rest; a scroll bar's rail; and the one panel fill in
  the system, on the way in where there is no glass (see Elevation & Depth).

The row washes keep the Two-Step Rule the text keeps: closer, a hand resting in
a panel and the keyboard walking it would light two rows in one colour, and
nothing would say which one Enter presses; and on the bar the keyboard's place
would look like a panel left open. The rail is a mark with a value of its own,
not a wash.

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
backdrop changes faster than the eye can calibrate.

Two steps cannot give every text role a value of its own, and should not:
**a new role takes an existing step.** Roles that sit in different places, or that differ by
case, size or tracking, share one rather than wear a near-copy of it. A new step
is only admissible with 0x20 clear on both sides, and there is no room left for
one.

**The Bright Text Rule.** No word sits under Body's step. The glass is held dark
enough for the lowest step of text to read on it over a bright frame, so every
step lower darkens every pane over every bright scene: with a row's running time
and a shortcut's keys at Quiet, the glass over white would be held to a dark
smoked grey that reads as tint rather than as glass. How the glass looks comes before a
second, quieter level of words. What sits beside a row's name is told from it
by place and by size instead, at Readout or Hint, a size under Body.

**The Fixed Floor Rule.** The ladder is built upward from Dim (`Paint.dim`), and
Dim does not move: retuning the ladder means moving the steps above the floor,
never the floor. Dim is the one step meant to look greyed out and Quiet is for
marks, so neither is what the glass is held to; that is Body, the lowest step
that is ever text (see The Legible Glass Rule).

## Typography

**Every role but one:** IBM Plex Sans (`Type.face`), compiled into the
executable in the three weights the scale uses (`Type.weight-plain`,
`Type.weight-title`, `Type.weight-current`) rather than asked of the system.
**Timecode:** IBM Plex Mono (`Type.numerals`) at the plain weight. There is no
display type anywhere in the product.

**Plain is Medium, not Regular.** At Regular the lines that carry most of the
interface read thin over glass, where the picture behind eats into every
stroke. Plex's in-between Text weight was not quite enough; Medium gives the
rows body without looking bold. It does bring plain within a step of the
Title's weight, so the Title and the playing row lean on their Lit alpha and
their place as well as on weight.

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
the track lists of the episode this was last checked against — is drawn by
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
number to type; reach for the plain weight first.

### Hierarchy
- **Title** (`Type.title`, `Type.weight-title`): the media title in the bar,
  the flash, and a fatal headline.
- **Body** (`Type.body`, plain): row labels, menu items, switch labels — the
  name of anything you can press. The current row in a list takes Lit at
  `Type.weight-current` instead.
- **Named** (`Type.named`, plain): a playlist heading that is a show's name rather than a
  section label. The Label role's weight and alpha and neither of its other
  habits: its own case, its own size, no tracking. Both fill the same slot at
  the top of a panel, so what separates them is what they *are* — the user's
  word or the interface's — and nothing else has to say it.
- **Readout** (`Type.readout`, plain): figures — timestamps, slider and stepper values, a
  row's running time, a shortcut's keys — and the labels of the dense rows
  those sit in, which read as part of the same line rather than as menu entries.
- **Timecode** (`Type.readout` in `Type.numerals`): a readout that ticks — the elapsed and
  total times at either end of the timeline, and the time over the seek
  preview.
- **Hint** (`Type.hint`, plain): prose added underneath something else — the drop hint,
  the line under a failure saying what usually fixes it, a driver's own error
  text.
  A readout's size under its own name, because prose is what would want a
  different one first.
- **Label** (`Type.label`, plain, tracked `Type.label-track`): section headings
  and back rows, written in capitals.
- **Glyph** (`Type.glyph`, plain): a character standing in for an icon — the
  stepper's minus and plus, sized against its disc rather than against the text.
- **Prompt** (`Type.prompt`, `Type.weight-title`): the one thing the interface
  says louder than the rest, the words on the end-of-file pill, sized against
  the glyph beside them.

The scale lives in the `Type` global in `theme.slint`, not at its use sites.
Several roles deliberately share a size: what separates a title from a body
line is weight, and what separates a readout from a hint is that one is a
figure and the other is prose.

### Named Rules

**One Weight For Headings.** Every heading in the system is plain
(`Type.weight-plain`), whatever it holds. A show's name at 600 beside a kicker at 400 read as two different
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

**The Label Floor.** Nothing is smaller than the Label size, and that size is
only for capitals, which are read by their shape. Below it, white text over
arbitrary moving content stops being read and starts being recognised, which is
fine for a heading whose wording you already know and no use at all for a
number that changes. Naming the roles in one place, the `Type` global, is what
keeps the scale from drifting below it quietly.

**The Capitals Rule.** The interface shouts its own words and never the user's.
SUBTITLES, AUDIO and EFFECTS are tracked capitals in the Label role because
they name parts of the application. A show's name, a track's name, a filename — anything
that came from the user's library — keeps its own case and gets the Named role
instead. Capitalising someone's content is how a tool starts sounding like a
database.

## Layout

A dark field with one pane of glass along the bottom, the window's three
buttons in its top corner, and at most one panel above the bar.

**The bar** is one pane (`MainWindow.bar-h` tall), `MainWindow.panel-margin`
from the window's sides and bottom: the timeline row (`MainWindow.timeline-h`)
over a row of buttons (`MainWindow.pill-h`), with `MainWindow.bar-pad` of glass
round both. The timeline row holds the elapsed time at its left end, the total
at its right, and the track between them. The row of buttons holds three
groups: the title on the left, the transport (`MainWindow.center-w`) centred
in the window until that would bring it within `MainWindow.pill-gap` of the
right group, where it keeps the gap and gives way leftwards, and the right
group (`MainWindow.right-w`) holding sound, the two tracks panels, the settings
and the picture's own controls. Buttons (`MainWindow.btn`) sit
`MainWindow.btn-gap` apart, `MainWindow.pill-pad` in from the row's ends.

The padding is what takes the end buttons' corner out to the panels', so the
bar's corners are concentric with the buttons in them and the bar and the
panels share one corner (see Shapes). Each time is inset
`MainWindow.glyph-inset`, so at its full width it starts and ends where the end
buttons' drawings do, and each end of the bar is one edge from the time down to
its button.

The bar is one pane because a capsule is rim the whole way through (see The
Transmissive Middle Rule): four of them were four rolled edges with nothing
between to darken, and a slab taller than its corners has a flat middle for
the rim to roll down from. The timeline needs glass under it in any case,
since a bare track all but vanishes over a white frame (1.02:1 against the
picture, with the played edge at 1.13:1 against the rail). It is the one place
in this system where **the glass is sized by the window, not by what is
written on it.**

The title group opens with its two buttons, Playlist and then Open, and the
title follows them (`MainWindow.lp-title`). So the buttons, and the playlist
and open panels that hang from them, stand in one place for every film, and
only the group's far end moves with the title. After the title, they would
move with every file, and the playlist panel with them.

The title hugs its room, up to what the transport leaves, ending
`MainWindow.pill-text-pad` short of it, and past that it runs out at the end of
its room, blurring and thinning away there as a list's rows do (see List edges)
with no ellipsis. Short of room for an episode's whole title, it drops the
show's name first (`MainWindow.title-short`): *S04E06 · Live Bait*, not *The
Walking Dead · S04* running out, since the show is what one show's episodes
share and the episode is what tells them apart. That is judged by the most
room the title could have, not the room it has, which the choice itself would
change.

**Panels** open `Rows.offset` above the bar, anchored to the button
that opened them and kept inside the same margins. Their widths are the
`width` of `menu`, `audio-panel`, `playlist-panel`, `settings` and
`files-panel` in `MainWindow`. Only one is ever open. Each is capped at the
space between the bar and the window's top, less the same margin it keeps from
the window's sides, so a panel at its tallest is framed evenly. Any list inside
it scrolls within that cap. Every page of the settings is such a list — the
first page whole, the others under their way back — so on a small window
nothing on any of them is out of reach; a page that fits does not scroll and
looks as it always did. A list opens on its current row, centred as far as its
ends allow — the playlist on the episode playing, the subtitles on the one in
use, the audio on the track playing — however it was opened. The language page
has a current row too, and the angle that points back to it. The hover label
opens on that same line, `Rows.offset` above the bar; the seek preview floats
just above it.

**The window has no frame.** The film runs to its edges, the picture is what
drags it, and its buttons are its own: Minimize, Maximize and Close on one
small pane of glass, `MainWindow.window-btn-margin` from the window's top and
side. A title bar would be a strip of the desktop's chrome across the top of
every film, in a material that isn't glass. Its corners are round, as the
desktop's other windows' are (see Shapes).

**The middle of the picture** belongs to things that are about the film rather
than controls for it: the action flash, the end-of-file pill and the choice at
the start of a file left part way, and — with no film loaded — the panel that
offers a way in.

**Every length is arithmetic on constants and the window width** — never a
question asked of a layout. The glass rects are sampled once per frame to tell
the renderer where to draw, and a binding that depends on a layout forces a full
layout pass on every one of those reads.

**Density:** the row heights are named in the `Rows` global, where the
components that draw the rows can also reach them: `Rows.row`, `Rows.heading`,
`Rows.toggle-row`, `Rows.stepper-row` and `Rows.slider-row`. A heading that
follows a group carries the group gap above it. The group gap is the row gap:
the same `Rows.gap` that insets every wash and pads every panel, so the space
that separates two ideas is the space the rows already stand in, and it is
*derived* from it, not typed again, because two literals asserting an identity
is one of them waiting to drift. So is the air a surface keeps from the next
one, `Rows.offset`: a panel above the bar, the hover label on that line,
the flash's caption under its ring, the seek preview clear of an open panel.
One idea, one name. The window opens at its preferred size and floors at
three quarters of it, the same 16:9. It never reflows; the breakpoints in the
sidecar give the sizes.

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
*Opening …* takes the title's place, and it is the only sign anywhere that
a folder on a share is still being read. The open is over when mpv finishes
opening the new file — not when there is a file, which over a film already
playing was true at once and cleared the line before it was ever drawn.

**The Bar Is A Floor.** While the bar is up, subtitles rest above it. The line
keeps its own place — the Position on the subtitles page — and rises only as
far as it must: to sit mpv's own bottom margin above the bar, the same air it
keeps above the window's edge when nothing is in the way. It climbs on the
bar's own fade and settles as the bar goes. A line already above the bar does
not move, and one resting in the letterbox comes all the way up. The case
that made this necessary is pausing to read a line, which holds the bar up over
exactly that line. The one exception is setting the Position: from its first
press the line goes down to where it rests, so the stepper moves what is being
looked at, and it comes back up when Timing or Size is pressed or the page is
left — those two are set by reading the line. The readout always shows where
the line was put, never where the bar has moved it.

**The Gap Goes Above Rule.** The group gap always sits above the thing that
starts something new, never below the thing that ended: above a section
heading, above a trailing reset. Split evenly, or left out, a heading sits about
the same distance from the group it names as from the group it follows, which
is a label equidistant between two things, labelling neither. Spacing is the only thing in a list of
identical rows that says where one idea stops.

## Elevation & Depth

Depth is carried by the material first: each panel's rim is a domed bevel
whose width is a fraction of that panel's own corner radius, magnifying the
picture under it as it curves away, with a rim light where the surface turns
past 45°. A surface reads as raised because its edge bends light, which is the
same reason a real one does.

**One shadow, cast on the picture and nothing else.** Each pane casts a soft
shadow, the same on every side: darkest at the pane's edge and falling off
steeply, gone within the pane's own corner radius and a few pixels more, so a
pill casts a small one and a panel a larger one, as the rim scales. Every
shadow is laid on the frame before any glass, so no pane's shadow falls across
another pane's glass: they stand at the same height. How dark it is at its
darkest is the glass page's Shadow slider: enough by default to set a pane off
a bright frame, too little to read as a smudge over a dark one.

Every surface opts into the tint, and with it absorption: the transmitted part
of the glass darkens as the frame behind it brightens, on a smooth curve rather
than a threshold, so white text keeps a ground over a white scene. Both are
settings: the pull toward the tint colour, and Dimming — how much of a white
backdrop the pane absorbs, a darker one losing less along a hyperbola and black
losing nothing. Turned down, the pane clears.

**The Legible Glass Rule.** Absorption is a look, and its slider can turn it
down; legibility is not on that page. Whatever absorption leaves, the glass
lets through no more light than Body needs to keep `pipeline.LEGIBLE` of
contrast on it: a ceiling worked out from `Paint.body` (`legible_ceiling`, in
`gpu/pipeline.rs`), eased into from `glass.KNEE` of it so a bright area keeps
its shading rather than going flat at a line. Over dark glass it does
nothing; over a white frame the panes go to a mid grey, and over a coloured
one keep its colour, darker. Like absorption it darkens the transmitted light
only, so the rim keeps its edge. The pane under the keyboard's ring has a
ceiling of its own, lower: its words are Lit, but on the ring wash, which
lightens the glass under them, so it is worked out from `Paint.wash-ring`
as well (see The Ring Pane Rule). WCAG asks 4.5 of text this size; the margin
is for its thinnest strokes, which antialiasing leaves short of their full
alpha. The one way out is a switch of its own on the accessibility page,
*Darken bright glass* (`MainWindow.legible-glass`), for someone who would
rather have the glass as its sliders make it and accept words that can vanish
over a white scene: a choice about legibility, made where legibility is
chosen, and not undone by the glass page's reset. With the glass switched off
there is no ceiling either, and nothing but the words' own shadow between
them and the film.

**What the material is, and what you want of it, are different questions.**
Two properties answer the first, and are constants in `gpu/pipeline.rs`
rather than settings: the **bevel** (`BEVEL`: the rim is the corner) and the
**index of refraction** (`IOR`: well past a window pane, short of the diamond
that pulls a black oval across the top of the way in). Neither has a range
where the result is a matter of taste rather than of whether the panes still
read as one material. What is left on that page — blur, refraction depth,
fringe, rim, tint, absorption, shadow — is how much of the material you want,
which is the question worth a slider. Reflection is not traced but placed by
hand, from the way the mirrored ray points. The rim light is white added round
each pane's outermost edge, where the surface has turned past 45° and the
mirrored ray points back down, rising steeply to full at the very edge.

**The blur is a Gaussian at every strength.** Up to σ 6 it runs at the
window's own size. Past that the picture is averaged down a level at a time
until what is left is under σ 6, blurred there, and drawn back with a cubic
B-spline, whose smooth slope gives the refraction no seams to magnify (see
`gpu/blur.rs`). So the top of the Blur slider costs about what its bottom does,
and is still a Gaussian rather than a kernel cut short.

**The pane stays drawn over black.** Over a dark frame there is nothing for the
glass to bend or blur, and a pane all but vanishes: a bar over a fade, or the
way on at the end of an episode, read as words on nothing. So two soft lights
fall on the dome, absorption's opposite, both raised toward the eye: a key
from above, across the pane and brightest at its top, and a fainter fill from
below, along its bottom edge. Each adds light to the glass as far as the
mirrored ray faces it, whole over black and fading out as the glass behind it
lightens, so over a lit frame, where the refraction draws the pane by itself,
they are gone. Like the ceiling they are not on the glass page, since they
are what keeps a pane there at all.

**The fringe is a spectrum, not three copies.** The index of refraction is a
function of wavelength (Cauchy's law, anchored so the index above is the one
at the sodium d line), and the transmitted colour is integrated across the
visible band under the CIE 1931 observer and D65 — the illuminant sRGB white
already is. The Dispersion slider runs to the most the glass can disperse with
every wavelength still bending: at 1 the band's red end, 705nm, is at an index
of exactly 1, a dispersive power of about 2.16 (an Abbe number of 0.46, where
dense flint is 20). That is at the default blur. A fringe is seen against the
edges it splits, so
at any other blur each wavelength's ray is moved off the d line's in step with
how wide the blurred backdrop's edges are (`pipeline::fringe_scale`): the same
setting is the same look at any blur, a perceptual slider rather than a
physical one. One offset scaled three ways reads as a coloured edge while
subtle and as three coloured copies past a pixel or two, so the band is sampled
as finely as the smear is wide against those edges: one wavelength across the
flat middle of every panel, where the slope and so the spread are zero, up to
twenty four at the rim. Each sample's weight is its exact share of the curve,
so a flat backdrop comes through white at any count. At Dispersion 0 it is a
single ray, to within 1/255.

Fading a surface in fades its glass with it: opacity scales the shader's
coverage, so a panel arriving at a third of its opacity refracts a third as
hard. Glass at full strength beneath a half-faded surface reads as a hole in the
picture.

The glass runs whether or not a film is loaded, and needs something to be
glass over: over a black frame refraction has nothing to bend and the rim
nothing to catch, and with the tint at zero the way in's panel is not faint
but absent. So before a film is loaded the pipeline draws **the backdrop** into
the frame instead: one seek-preview tile of the last film left unfinished,
taken at the point you stopped, covered across the window, softened past the
point where its pixels read, dimmed (`DIM` in `backdrop.frag`) and falling away
toward the corners. It arrives over `BACKDROP_ARRIVAL`, the one arrival slower
than a surface, because it is the window's light changing rather than a control
answering. Everything downstream treats it as
the frame, so the way in refracts it like any panel refracts a film.

With no unfinished film, or no atlas for it, **the mark lights the window
instead**: the logo, drawn into a tiny tile (see `gpu/mark.rs`) and given to the
same pass, so it is covered, softened, dimmed and fallen away exactly as a
film's frame would be. At that size the pass leaves no edge to read (a larger
tile still showed a face glowing behind the panel), so what reaches the glass
is the mark's coral as light, brightest behind the panel and falling off across
its rim, where refraction has something to show. A fresh install opens in this
state.

The **one exception** to glass is *there is no glass to paint over*: the way in
carrying a fatal error, where every failure that raises it leaves the player
with no render context, so the glass it publishes is drawn by nobody. It then
carries a Soft wash fill (`Paint.wash-soft`) at the panel radius. The same fill stays
as the fallback for a window with no backdrop at all, which the mark leaves
only for a logo that would not draw. Nowhere else. Everywhere else the material
is real, and a fill would cover the thing this product exists for.

### Named Rules

**The Proportional Rim Rule.** Bevel is a fraction of the corner radius, never a
count of pixels. The same absolute rim that looks right rolling around a
panel's corner swallows a pill whole. One number therefore means the same thing
on every piece of glass, and a small pane simply has a small rolled edge, which
is also how real glass is made. At `BEVEL` of 1 the rim runs the full width of
the corner, which is why a capsule is rim the whole way through.

**The Room to Be Glass Rule.** A surface must leave more margin around its
contents than its own bevel is wide, or the material has nowhere to render: a
seek preview padded thinner than its bevel shows no glass at all.

**The Transmissive Middle Rule.** Absorption — the thing that keeps white text
and white controls legible over a bright frame — is applied to the transmitted
component only, never to the rim light.
The rim is the full corner radius, so a capsule is rim the whole way through
and has no transmitted component to darken. **A pane that exists to give
something a ground must be tall enough to have a flat middle.** This is why the
bar is one pane and not a capsule for each row and group: glass at a row's own
size would be all edge and would darken little, which is the one thing it is
there to do.

## Shapes

Rounded rectangles throughout, at one governing rule: **radii nest
concentrically.** Two corners share a centre — and so keep an even gap the whole
way round — when their radii differ by exactly the gap between them. So
`Rows.radius` plus `Rows.gap` *is* `Rows.panel-radius`, and the panel's padding
is that same gap, which makes the first and last rows come out concentric with
the panel's own corners without being special-cased. The bar runs it the
other way: half its row of buttons plus `MainWindow.bar-pad` is the panels'
corner, so the padding follows from the two radii and the end buttons sit
concentric in the bar's corners.

The same arithmetic governs the seek preview: the thumbnail's corner
(`MainWindow.preview-tile-radius`) is the preview's less its padding. Turn
either the radius or the gap and the third value follows.

The hover label, the flash's caption and the end-of-file pill are true
capsules, their radius half their height. Icon
buttons are circles, and so is the flash's ring. Every track, fill, measure and
scroll thumb is a capsule too. The switch is a capsule with a circular knob
inset from its ends.

The corners are the glass's own, and not a circle's: superelliptic, easing out
of the straight edges over `CORNER_SOFTNESS` radii (in `gpu/pipeline.rs`) and
turning in further at the diagonal for it. Everything drawn on the glass or
in its place takes the same outline — a row's wash, the end and skip pills'
highlights, the fill that stands in where there is no glass — through
`Squircle`, which `interface::squircle` draws from the shader's own rule. A
circle's corner at the same radius stands proud of the glass's, past each end
of a pill at higher softness. Circles stay circles, and so
do the thin capsules of tracks and fills, where the two differ by less than a
pixel. The seek preview's thumbnail keeps a circle's corner too, since Slint
clips only to that.

The window's own corners are circles as well, at `window.CORNER_RADIUS`,
libadwaita's: the window's edge is the desktop's, beside other windows, and
not a pane of glass. They are cut from the finished frame, last of all, and
square while the window is maximized or fullscreen, as other windows' are
then. On Windows, Windows 11 rounds them itself. Nothing inside nests with
them: the bar and the window's buttons stand far enough in that no corner of
theirs comes near the cut.

There are no borders anywhere, and no hairlines. Where a state would
conventionally be drawn as an outline — the keyboard's position — it is carried
by a stronger wash instead, standing on a pane of glass whose own rim lights
it.

### Named Rules

**The Ring Pane Rule.** The keyboard's place is a pane of glass of its own,
laid on the surface it is in, under the Ring wash. A wash alone can't do both
of its jobs: no alpha of it stands out from the glass beside it at 3:1 and
keeps Lit words on it at 4.5:1, since the brighter it is, the less its words
stand out. So the wash is the area and the pane is the edge. The pane is real
glass, refracting the frame on its own, held darker than the glass around it
(see The Legible Glass Rule), and its rim is lit at `pipeline.RING_RIM`
whatever the Rim slider says, so its edge carries the 3:1. It
casts no shadow, and never reaches past the pane it lies on, which a page
sliding in would otherwise carry out over the film. On a bar button the window
places a disc (`MainWindow.ring-disc`); on a row or one of a pill's choices,
which the window can't reach into, the row says where it is through `Ring`
(`RingPane`). In a scrolling list the pane is cut to the list and fades as its
row leaves, gone by the time half the row is.

**The Concentric Rule.** Inner radius = outer radius − the gap between them.
Anything else pinches the curves together at 45° while still meeting cleanly on
the axes, which is exactly the part the eye catches. A superelliptic corner at
the inner radius is not quite concentric with one at the outer. Where a wash
has to follow the glass's edge, it is the glass's outline moved in by the gap
(`Squircle`'s `inset`), as the end pill's two choices are. The rows take a
corner of their own instead, since a row is too short for the panel's corner
moved in, which would draw its ends out; their own narrows the gap to the panel
by under a pixel at the corner.

## Components

### Icon buttons
- **Shape:** a circle (`MainWindow.btn`), its glyph drawn at `IconButton.glyph`
- **Default:** transparent, glyph at Lit, as is every glyph on the bar that can
  be pressed: at Body the bar's controls read greyed out over a bright frame.
- **Hover:** Button wash, over `Motion.highlight`; the glyph is already Lit
- **Active:** every button that sets a state swaps its own glyph (muted for
  unmuted, slashed for unslashed), and that swap is the whole of the sign. A
  wash behind an engaged button makes the state read as chrome.
  **A button's state is its glyph, never its brightness**: every glyph on the
  bar is Lit, and Mute muted or Fullscreen on say so by the mark they wear,
  which their tips name.
- **Open:** a held Strong wash for as long as the panel it opened is showing — a
  step under the ring's, so the two never share one highlight.
  A button that opens something does not also show a state, because a glyph
  that lights like a switch is clicked like one. The subtitles button wears the
  plain subtitles mark, and its menu's *Off* row is where subtitles go off;
  the audio button wears Speaking, the mark over its panel's heading.
- **Focus:** the Ring wash, on a disc of glass laid on the bar
  (`MainWindow.ring-disc`), while the keyboard is on it: see The Ring Pane
  Rule. With the glass switched off the wash is the whole of it, a step over
  the open one.
  Tab walks the bar left to right while a film is loaded and
  nothing is open — then on to the end-of-file choices, or the credits pill,
  while either is up —
  and Enter presses; the arrows, Space and every other key keep their meaning
  wherever the ring stands, and Escape puts the ring away and does nothing
  else, so clearing a highlight never leaves fullscreen. The hover label comes
  up for the focused button as it does for the pointer. A panel opened from the
  bar takes the ring, and hands it back to the button when it closes. The ring
  goes with the bar when the bar hides, unless it stands on a pill, or Enter
  would press a button nobody can see. Its place is kept, and the next Tab
  brings it back there.
- **Nowhere to go:** glyph at Dim, no wash, no pointer, and a label that says
  so — *No earlier file*, with no key beside it. Its key does nothing either,
  raises no flash, and Tab walks past it.

### The bar
- **Shape:** one pane (`MainWindow.bar-h`) across the window inside its
  margins, at the panels' corner (`Rows.panel-radius`), `MainWindow.bar-pad`
  of glass round its two rows
- **Background:** none. The pane is a glass rect published to the renderer; a
  fill here would paint over the material. A click on it that no control
  takes stops there, rather than pausing the film under it.
- **Rows:** the timeline row (below) over a row of buttons (`MainWindow.pill-h`:
  a button with `MainWindow.pill-pad` of padding), with nothing between them
- **Contents:** buttons `MainWindow.btn-gap` apart in three groups; in the
  title group, the two buttons first and then the title, the Title role at
  Strong, ending `MainWindow.pill-text-pad` short of its room and running out
  there. At
  any speed but normal the speed rides beside the title at Body in Readout size (*1.5×*) for as long as it holds, rather than only in a passing
  flash.

### The window's buttons
- **Shape:** one capsule of glass, `MainWindow.window-btn-pad` round three
  buttons `MainWindow.window-btn` across and `MainWindow.window-btn-gap` apart,
  its ends concentric with the end buttons as the bar's corners are with its
  own. One pane, as the bar is one, so the three read as one object rather
  than three beads, and a click on the glass between them stops there rather
  than pausing the film.
- **A button is its disc:** its glyph at `IconButton.glyph`, and the Button
  wash filling it under the pointer. Smaller than the bar's buttons, because
  none of the three is pressed often and they are over the picture whenever
  the chrome is.
- **Shown** with the chrome, on its fade, and always over an empty window,
  which has no film to keep clear. Never in fullscreen, where the film has the
  whole screen and Esc is the way out.
- **Comes down** `MainWindow.lift` into place rather than rising: the buttons
  come in from the window's top edge, as the bar does from its bottom one.
- **Maximize turns into Restore** while the window is maximized, glyph and
  label both: a button's state is its glyph.
- **It makes way for a panel** tall enough to reach its row in its column, and
  comes back when the panel goes: the panel was asked for, and the buttons
  weren't.
- **No hover label.** The three are every desktop's own glyphs, with no key
  to teach; a screen reader still gets their names.
- **Not on the keyboard's ring.** Tab walks the film's controls; the desktop's
  own keys for minimizing, maximizing and closing a window still do those.

### Timeline row
- **Shape:** the bar's top row (`MainWindow.timeline-h`), across its full
  width
- **Times:** Strong in the Timecode role, each centred in a column as wide as
  the film's length reads, never narrower than `00:00`, with
  `MainWindow.glyph-inset` either side: a time at its full width starts and
  ends where the end buttons' drawings do, and one past the hour keeps the same
  air. Sized from the length rather than the time played, so it is set when a
  film opens and the row does not shift when the time played passes the hour
- **Track:** at Rail, growing under the pointer or a drag (`MainWindow.timeline-rail`,
  over `Motion.track-grow`); the played length at Played, sized by
  width so it stays crisp, and never taller than it is long, as a slider's
  fill is
- **Hit area:** the full row height, not the thin track it draws, which would
  be miserable to grab. The wheel over it seeks.
- **Scrubbing:** the film pauses for a drag and the picture follows the
  pointer as exactly as the hand is moving. Under about forty seconds of film
  a second it is shown the frame under the pointer; over about sixty, the
  keyframe before it, which is all a picture passing that fast can be told
  from. A pointer that stops with the button down gets its frame a tenth of a
  second later, and letting go lands on it exactly. In film and not in pixels:
  a pixel of this row is a third of a second of a music video and seven of a
  feature, and a keyframe is up to ten seconds from where it was asked for
  either way.
- **Letting go** ends a drag wherever the pointer has got to, on the row or
  off it, since a drag wanders off a row this thin more often than it stays on
  it.
- **Chapters:** a dot at each chapter start, the unity dot's shape and its
  rule — Knob where the played length covers it, Quiet ahead of it. None at
  the very start, which is the start rather than a boundary, and none at all
  for the many files without chapters.

### Volume
- **Track:** `MainWindow.vol-w` long, at Rail, growing like the timeline's
  (`MainWindow.vol-rail`), filled at Fill across mpv's full 0–200% range, never taller
  than it is long; empty at zero or muted. A mute fades the fill out and an
  unmute fades it back in, over `Motion.switch` as a switch's track turns,
  so the press is answered on the track as well as by the glyph.
- **Unity dot:** a dot, not a line, at 100%. A line the height of the track
  divides it into two ranges; a dot is a landmark on one. It reverses against
  the fill the way the switch knob does: Knob on the fill, Quiet off it, and
  turns as the fill fades.
- **Wheel:** `commands.VOLUME_STEP` a notch, over the track and nowhere else. Over the
  picture the wheel wakes the bar and changes nothing, so a brushed wheel or
  fingers resting on a touchpad can't reach the loudest control in the player.
- **Label:** the level as a number, which nothing else on the bar gives: the
  track is short for two hundred steps and the glyph beside it moves in tens.
  *Volume 84%* with ↑ ↓, or *Muted* alone, since a muted level is a number
  nobody can hear and the track is drawn empty to match.

### Menu rows
- **Shape:** `Rows.row` tall; the wash is inset `Rows.gap` on both sides at
  `Rows.radius`, full row height so adjacent washes meet
- **Text:** Body, starting `Rows.label-x` from the panel edge, eliding
  `Rows.trail-x` from the right
- **Hover:** Soft wash, over `Motion.highlight`
- **Focus:** Ring wash on a pane of its own, shown only while the keyboard is
  in use — never a ring (see The Ring Pane Rule). Everything on the row goes
  to Lit under it: the name, its icon, and a figure beside it, a running time
  or a count. A pointer moving over a different row stands the ring down, so a
  panel never shows two lit rows
- **Active:** Lit at `Type.weight-current`, and the active mark: a thin
  capsule in Lit (`ActiveMark`), inside the wash so a row that is both current
  and focused
  wears both marks without them touching. A shape, because weight and alpha
  alone were close to invisible over moving video.
- **Icon:** only on a row that is an action on a settings page — *Reset to
  defaults* wears Reset, and Warning for as long as it says *Press again to
  reset*; *Sync to speech* wears Speaking. Placed and spaced as a stepper's
  own, so marks and labels each stay in one column down the page; it lights
  with the words. A choice in a list has none.

### Finding subtitles
- **Where:** the last row of the subtitles menu, *Find subtitles…*, with the
  Search icon — an action among choices, so it wears the mark they do not. On
  a film with no subtitle tracks the menu is that one row, which is when it is
  most wanted. Absent from a build without an OpenSubtitles key, where a film
  with no subtitle tracks gets the menu anyway, holding one line that says so:
  *No subtitles*, as the audio panel says *No audio tracks*
- **The page:** pressed, it slides a page of the subtitles menu over the tracks
  with the drill's movement, headed by its way back, FIND SUBTITLES. Under
  that, one line while it searches — *Searching OpenSubtitles…* — then either
  the results or what came of it: *No subtitles found*, or why it could not
  ask, in a line rather than a notice, since the page that asked is the
  one on screen
- **A result:** the release it was timed against at Body, eliding, and beside
  it at Body in Readout size its language by its own name and what sets it
  apart: *Exact* for one found by the file's hash, timed for this very copy,
  which all come first; SDH; *Machine* for a machine translation. Pressed, the
  second column says *Downloading…* and the row wears the active mark until it
  arrives; then the menu slides back to the tracks, where it stands lit as the
  subtitle showing. A failure there is a notice, beside the panel
- **Afterwards:** a fetched subtitle is the film's from then on, loaded with
  it every time it opens, in the list beside its own tracks — until it is
  taken out with its row's Trash button, which forgets it and deletes it

### Removing a track
- **Which:** any track that came from a file of its own — fetched, dropped,
  or found beside the film — subtitle or audio. A film's own tracks have none
- **The button:** Trash at the row's trailing edge, in a disc: at Body, Lit
  with a Strong wash under the pointer. What takes the click is the row's full
  height from just before the disc to the row's end, since a click just off a
  small disc would choose the track instead, the one miss that does the
  opposite. It sits over the row's own press, so taking a track out
  never chooses it first; the label elides short of it
- **Keyboard:** Delete on the row the ring is on. Listed with the shortcuts
- **Two presses:** the first asks *Remove?* beside the button, with Warning in
  the disc, for `MainWindow.armed-for`; the second removes. The name stays,
  since it is what says which track the press would take; it elides short of
  the question, and a press on the question is a press on the button. The
  question stands on a Strong wash running from it to the button, which drains
  towards the button over the wait, so how long the row will go on asking is
  in sight; with animations off it stays full until the row stops asking. A
  fetched subtitle's file goes with it, and getting it back is another
  download against the key's small daily allowance. Closing the menu, or
  going to the search, stands the row down. Unlike a reset it does not offer
  itself back afterwards: the file is gone
- **What it does:** takes the track out of the film. A fetched subtitle is
  also forgotten and its file deleted, so it does not come back on the next
  open; a file beside the film is the person's and is left where it is, so
  that one returns with the film

### Playlist rows
- **Shape:** a menu row with two more facts on it
- **Running time:** Body in Readout size, right-aligned so a column of them
  reads as a column; empty for a file never opened and not yet described.
  A file that would not play wears the Warning mark there instead, without
  the measure, for the rest of the run: the mark alone, since words would take
  width from the title
- **Order:** the order a person watches in. A show's files by season and
  episode, kept together where the first of them falls by name; everything
  else by name with digit runs read as numbers and case ignored, so two
  release groups' names can't split one show around another. Autoplay, Next
  and the end of a season all trust it
- **Title:** elides clear of the time and the measure, never under them

### The playlist's parts

A season of one show is shown as one list, with the show and the season as
its heading: *Game of Thrones · Season 7*. A list holding more than that is
shown in parts, on two levels, the way the settings are, **including the way
they move.** Opening a part and coming back out is the same drill
(`Motion.drill`): the page being left goes out the side the new one comes in
from, and the panel grows or shrinks to the new page's height under them both,
keeping its bottom edge on the line every panel sits on. One list becoming
another in a single frame would say only that everything was different.

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
  Body in Readout size in the running times' column, then the chevron. No measure, since its
  own page says how much is watched better. The part holding the file playing wears the active mark. File
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

A short capsule on a playlist row (`PlaylistRow.measure-w`), at Rail with a
fill at Strong, sitting on the row's own line a group gap before the running
time's column, which is an hour-long time's width so a season's measures stand
in one column however the digits beside them run. It says how far into that
file you got, and appears only when there is something to say: a resume point,
or a file **finished** (watched to its end or into its credits), which wears the
measure full. mpv deletes a position once a file is watched through, and a scan
gives every file a length, so the player keeps a record of its own, or a
finished episode would look exactly like one never started. Started again, the
row goes back to saying how far.

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

### Scroll bar
- **Shape:** a thin rail at Soft wash (`ScrollList.bar`), near the list's right
  edge and inset top and bottom; the thumb a capsule at Quiet, proportional to
  how much of the list is visible, with a floor so it can still be taken hold of
- **Under the pointer:** rail and thumb grow (`Motion.track-grow`) about their
  own centre line, the way every track does, and for a drag as well. No change
  of colour: the growing is the answer, as it is on a slider
- **Hit area:** the margin the rows' washes leave clear, the full height of
  the list, not the thin rail that is drawn
- **Drag:** the thumb is held where it was taken, so it does not jump under the
  hand. A press on the rail brings the thumb there by its middle and the drag
  carries on, which is one press to anywhere in the list
- **Role:** it says there is more and roughly where you are, and it can be
  taken hold of, since a bar that only hinted would be the one thing on a panel
  that looked like a control and was not. The wheel, the flick and the keyboard
  move the list too; the keyboard scrolls by the least that reveals the focused
  row.

### Shadow under the words
Every word and mark the interface draws has a thin dark rim under it: no
offset, a Gaussian of `shadow.SIGMA`, taking away up to `shadow.DEPTH` of the
frame beneath where it is deepest. It is for contrast, not for depth: white
text keeps an edge over a bright frame, where the glass alone could leave it
nearly white on white, and it is darkest exactly there, because it takes away a
share of what is underneath rather than laying a fixed grey over it. Over dark
glass it is barely there.

What casts it is what the interface laid down as white at more than about a
third of full strength: text, glyphs down to Dim, a track's
filled length, a switch, the played timeline. A wash, the keyboard's
included, or a rail does not, so a highlighted row has no rim of its own. A picture the interface shows — the
seek preview's frame — is left alone, shadow and all.

Like the list edges it is drawn by the player on the finished frame — see
`gpu::shadow` — since Slint can shadow a rectangle and nothing else. It runs
only where there is glass, and with the chrome asleep it does nothing. It finds
the words by the white they added to the frame, and white on white adds none,
so wherever it runs the glass pass hands Slint the frame held
`shadow.HEADROOM` under white, and the shadow gives that back: over clear glass
on a white scene, where nothing else keeps a word readable, the shadow still
does. It is
drawn after the list edges, from what they left, or the rows thinning away at
an edge would leave their shadows behind them as a dark smear.

### List edges
Where a scrolling list's rows run on past its top or bottom, they are not cut
by a line: over the last `Rows.fade` they blur and thin away into the panel's
own glass. The blur is a Gaussian that grows from nothing at the strip's inner
edge to `fades.SIGMA` at the list's, and the rows fade on an eased curve
beside it, so neither end of the strip shows as an edge.

An end with nothing past it is left sharp. The strip grows to its full depth
over the same distance of scrolling away from that end, only ever as deep as
what has gone past, so the top row of a list at rest is whole. However shallow it is,
its outer edge is where things are gone entirely: nothing is ever half cut. The
strip spans the rows' washes and no more; the margins either side are outside
it, since no row draws in them, and the scroll bar's thumb keeps its shape at
the ends of its rail.

**The way to the current row.** A list opens on its current row — the file
playing, the season holding it, the track in use — but the wheel takes it
away, and then nothing on the panel says which way it went. So while it is
out of sight an angle points to it (`way.ICON`), centred across the list and
level with the row at that end, at Body, and Lit with a circle of Button wash
(`way.CIRCLE`) behind it under the pointer, eased in over `Motion.highlight`.
It fades by
where the row is, not on a clock: it starts going as the row's nearer edge
comes into sight, and is gone once that edge is clear of the soft edge, so
the list moving is the whole of the movement. Pressing it glides the list
back (`Motion.glide`) until the row is in the middle, as the list opened. It
has to take the press: an angle that took no pointer would hand it to the row
under it, and play an episode nobody chose.

It is drawn by the player rather than by Slint, over the finished frame once
the soft edge is done — see `gpu::way`. Drawn by Slint it would stand in that
edge and thin away with the rows; with the edge cut around it, the rows
passing under it would show there whole and cut off. The angle is the
icon itself, rendered by resvg, so there is still one drawing of it. The
shadow comes after both and gives it its rim.

The keyboard's ring keeps clear of it. Walking a list with the arrows
scrolls a row early, so there is always one whole row between the ring and
the end it is heading for, until the list itself runs out: the row the next
press goes to can be read before it is pressed.

The title in the bar runs out the same way, sideways: a title longer than its
room is cut at the end of it with the same strip turned on its side, in place
of an ellipsis. It comes on as the title outgrows the room, so one that fits
is untouched.

It is drawn by the player rather than by Slint, on the finished frame — see
`gpu::fade` — which is why it is true blur and not a gradient laid over the
rows: what they thin away to is the frame underneath, whatever the film is
doing in it.

### Sliders
- **Row:** `Rows.slider-row`; a row icon at Body, as a stepper has, then the
  label at Body in Readout size, in one column on both pages as wide as the
  widest name in the language spoken (`MainWindow.param-name-w`), so the
  tracks start together, and the value right-aligned at Body, the step a
  stepper's value stands on too. At Dim, a step under its name, it would read
  as greyed out
- **Track:** at Rail, growing on hover or press (`ParamSlider.rail`, over
  `Motion.track-grow`), a step thinner than the bar's tracks, because these sit
  inside a panel of rows rather than alone on glass
- **Fill:** width-driven from the left at Fill, never clipped — and never
  taller than it is long. It is a capsule like the rail, so below the track's
  own height it is a circle the width of the value, growing out of the rail's
  tip until it fills the track and then running along it. Held at full height
  it would have no room for its round ends, and at one or two per cent would
  stand in the rail's end as a sliver wider than the curve it is meant to be
  inside. Its edge is at the value all the way down, and at zero there is
  nothing. The smallest are drawn square, with nothing left to round. The
  timeline and the volume follow the same rule
- **Readout:** precision taken from the parameter's *range* rather than its
  value — three decimals on a 0–0.1 edge blur, none on a 0–100 blur
- **Focus:** the row takes the Ring wash and its pane, and its icon, name and
  value go to Lit; the sliders have no hover wash, since the growing track
  already answers the pointer
- **Keys:** ← and → move it a hundredth of its range, which the readout's
  precision always shows

### Steppers
- **Row:** `Rows.stepper-row`; a row icon and the label at Body in Readout
  size, then two discs (`StepButton.width`) flanking the value at Strong
- **Discs:** Soft wash at rest, Strong wash on hover (`Motion.highlight`); `−`
  (U+2212, to match the digits) and `+` in Glyph size at Strong
- **Focus:** the Ring wash and its pane, and everything on the row at Lit, the
  value and the discs' glyphs with it
- **Use:** for quantities read as numbers rather than positions — a subtitle
  delay is +0.30 s and the useful move is one step. Sliders are for material
  parameters, where the eye judges the result and the number is incidental.

### Switches
- **Row:** `Rows.toggle-row`; switch first, label after it at Body (the rows'
  own step, because a switch sits under a list as a setting *about* it, and at
  Strong the autoplay row would out-ink the episodes it governs)
- **Shape:** a capsule (`ToggleRow.toggle`, `ToggleRow.track`) with a knob
  inset in it (`ToggleRow.knob`). Both are drawn from shapes (`ui/shapes/`)
  rather than as rounded rectangles: the renderer lays a rounded corner down as
  a few straight chords that touch the curve only where they meet, and on a
  capsule one of those meetings is the middle of each end, so at this size the
  ends come to a point. A picture is rendered at the size it is shown, curve
  and all
- **On:** track at Fill, knob at Knob; **off:** track at Rail, knob at Fill —
  dark on the lit track, light on the dark one, so the knob reads as a hole
  punched through it either way
- **Motion:** `Motion.switch` ease-out on the knob and on the track colour
- **Hover:** Soft wash; one target for the whole row
- **Use:** Autoplay under the playlist; Ambient border at the head of its own
  page, over the sliders that tune it, since whether comes before how; and
  Animations, Darken bright glass, Tooltips and Shortcuts in tooltips on the
  accessibility page — a page of switches and no reset, because each switch is
  its own undo. B still turns the border on and off from anywhere, as the one
  effect worth changing mid-film
- **Unavailable:** a switch about something another switch has turned off
  fades to half, takes no pointer and no wash, and keeps its position —
  Shortcuts in tooltips while Tooltips is off

### Drill rows
- **Row:** `Rows.row`; a row icon and the label, both at Body, with a chevron
  at the end: Body at rest, Lit on hover and under the ring. The ring lights the
  icon with the words, since both name the same subject
- **Use:** every row of the settings tree. An effect that can be turned off says
  so with its glyph rather than with a switch, because a switch beside a chevron
  is two things to press in one row.

### Back rows
- **Row:** `Rows.row`, the page's heading and the way out of it at once
- **Text:** Label role at Body, starting on the row-label line; Lit on hover,
  over a Soft wash
- **Chevron:** a row icon in the icon column, laid out exactly as a drill row
  lays out its icon and with the same air after it. A back row is the way out of
  a page of those, and the pair read as one movement only when they are built
  the same. Hung in the margin it would line the label up with the headings,
  but pinch the arrow between the panel edge and the first letter, which is the
  cost the eye actually notices.

### Panels
- **Shape:** `Rows.panel-radius`, `Rows.panel-pad` of padding, no background of
  its own
- **Entry:** a fade and a rise of `MainWindow.lift` over `Motion.surface`
  ease-out, glass fading with the surface
- **Headings:** tracked capitals at Body, except a playlist heading carrying a
  show name, which takes the Named role — natural case, no tracking, and the
  same alpha and weight. A heading with a group above it carries the group gap
  itself rather than being pushed down by the row above, because the last row
  of a group does not know that it is last
- **Heading icons:** a heading over rows without icons of their own carries
  one — the playlist, SUBTITLES and AUDIO over the tracks panels, every group on
  the shortcuts page. A row's size (`MenuHeading.icon-size`), since icons come
  at two sizes and nothing between (see Don'ts); at the heading's own alpha, so
  mark and words are one thing; starting on the row-label line with the words
  after it. It cannot hang in the margin: the margin ends at the edge of the
  wash. The settings page's headings have none, because every row under them
  already wears one
- **Trailing rows:** a reset or a setting *about* the list gets a group gap
  above it. Without it the eye reads one more entry and only the wording says
  otherwise.
- **Resets:** two presses, then a way back. The first changes the row to *Press
  again to reset* for `MainWindow.armed-for`; the second resets, and
  the row becomes *Undo
  reset* until the page is left. Settings always save, so a reset is on disk
  within a second, and a look tuned by eye cannot be typed back in. The ring
  stops at the ends of a page that has one rather than wrapping, and Up from a
  hidden ring lands on the last control, never on the reset.
- **Sync to speech:** on the subtitles page, the first row, above Timing: it
  is the thing to try before setting the delay by hand, and what it settles
  on shows in the row under it. It wears Speaking, so it keeps the steppers'
  column of marks. One
  press and the row reads *Listening…* until the answer comes — seconds for an
  episode, a quarter of a minute for a film — and a press meanwhile does
  nothing. Then it is *Undo sync* until the page is left, the way a reset
  offers itself back: what a sync found is the player's reading of the film,
  and the timing it replaced may have been set by ear. Every outcome is said
  as a notice, because two of them change nothing on screen — *Subtitles
  already in sync*, and *Could not match these subtitles to the speech*, which
  is the answer whenever no timing stands clear of the rest. A sync that is
  not sure does nothing.
- **Shortcut rows:** Body label, keys at Body in Readout size. On
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
- **Left and right, in a panel,** move the row the ring is on where it is a
  slider or a stepper. Elsewhere they go the way the rows do: → opens the row
  the ring is on if it leads somewhere — a settings page, a season, the
  search for subtitles — as Enter does, and ← goes back a page if there is
  one, as Escape's first press does. Anywhere else they do nothing, by the
  rule above: the film jumping ten seconds behind a menu is a key pressed in
  the wrong place. Held with Shift or Ctrl they are the film's again, a
  second's seek and the next file
- **The keys beyond the bar** are mpv's own, so the habit carries over:
  Shift with an arrow for a one-second seek, `[` `]` and Backspace for speed
  on a fixed ladder (0.25× to 4×), `,` `.` for a frame, PgUp PgDn for chapters,
  J and Shift+J through the subtitle tracks, # through the audio, and `?`
  straight to this page, which is also a row on the way in. The bindings
  the player already had — the arrows' volume, S, T — stay where they were. A
  opens the audio, and V, beside C, the subtitles
- **Language:** the last row of the settings' INTERFACE group opens a page of
  menu rows, one lit: *System language* first, which is what the player speaks
  until told otherwise, then English, then each language built in under its
  own name for itself — *Svenska* — which is never translated, so a reader
  can find their own language from inside one they cannot read. Choosing one
  changes every word on screen in that frame, the rows of the page included.
  Key names stay as the keys are marked: Space, Home, Esc

### The hover label

A capsule of glass (`MainWindow.tip-h`) that names the control under the
pointer and gives the key that does the same thing. The name is Body, the key
Body at Readout size (`MainWindow.tip-gap` apart, `MainWindow.tip-pad` of
padding): the two roles of a shortcuts row, unchanged, because the tip is that
list arriving one row at a time where the question was asked.

- **`Motion.tip-delay` before it opens**, then a fade and a rise like any other
  surface.
  Long enough that crossing the bar raises nothing; short enough that stopping
  on a glyph feels answered rather than eventually attended to.
- **Then the bar is being read, and waits no more.** While one is up, or for
  `Motion.tip-grace` after it goes, the next control's name replaces it in
  place with no
  delay and no fade — a label, a label, not a label, nothing, a label.
- **A fade is of what was there.** The name, key and position a tip rose with
  stay put while it goes, rather than turning into the next button's or into an
  empty pill around a key.
- **Pressing puts it away**, and it comes back on the next arrival; a name
  hanging over the result is in the way of seeing it. A resize takes it at
  once, because every control has moved out from under it.
- **A reading does not wait, and is not put away.** The volume track's label
  is the level rather than a name. It opens after the same delay when the
  pointer only rests there, but a level that moves under the pointer is said at
  once — it is the answer to the wheel, the drag or the key that moved it, and
  each notch would otherwise start the wait again. A press leaves it up: here
  the label is the result, not something in front of it.
- **Above the bar, on the line panels open from**, centred on its control
  and clamped inside the bar's margins. There is nowhere closer: the bar is
  one unbroken pane, and a tip laid over it would be glass on glass.
- **It says what pressing will do**, not what the glyph already shows. The
  glyph says which state you are in; the label is the only thing that can say
  what happens next, so a muted control offers *Unmute*.
- **One surface for the whole bar.** Only one is ever up, and thirteen would be
  thirteen more rects published to the renderer for a thing that is never in
  two places.
- **Both halves can be turned off**, on the accessibility page: *Tooltips*
  for the whole label, the volume's reading included, and *Shortcuts in
  tooltips* for the key alone. Someone who knows the bar has learned what the
  tips teach, and from then on a pill rising over the film wherever the
  pointer rests is in the way. The controls keep their names for a screen
  reader either way.

### Named Rules

**The Label Cannot Outlive Its Control.** Whether a tip is drawn is an
expression, never a decision taken when the pointer arrives. Both ways it can
go wrong happen afterwards: a pointer resting on a button raises no events, so
the idle clock runs out under a label that is already up and the bar leaves
without it; and a panel can open underneath one, over the very row the tip
covers, which a check run once on hover never sees. A tip a panel hid stays
gone when the panel closes, until the pointer next arrives; uncovered, it
would show its fade-out in a place the hand may have left.

### The seek preview
The signature component. A thumbnail from an ffmpeg sprite atlas
(`preview.GRID` tiles a side), one tile picked with a source-clip, inside a
glass panel (`MainWindow.preview-radius`, `MainWindow.preview-pad` of padding)
whose tile's corners are concentric with it (`MainWindow.preview-tile-radius`),
with the timestamp beneath it at Strong in a caption
(`MainWindow.preview-caption`). It floats just above the bar, centred on
the pointer and clamped inside the bar's margins, and takes no pointer events
of its own. It falls back to the timestamp alone, in a capsule, when no atlas
exists, which is the state it sits in for the first seconds of every unseen
file and permanently on a machine with no ffmpeg.

### The action flash
An answer, not a control: a glyph in Lit (`MainWindow.flash-glyph`) inside a
circle of glass (`MainWindow.flash-ring`), and `Rows.offset` under it, when
there is one, a capsule (`MainWindow.flash-text-h`) carrying the one number that
has nowhere else to appear: Title in Lit, `MainWindow.flash-text-pad` of
padding, never wider than the bar. It lives long enough to read a
two-character figure and short enough that a held key flickers rather than
strobes (`MainWindow.flash-dwell`).

**A seek is thrown to the side it seeks to** — a fifth of the way across for
back, four fifths for forward — and everything else sits in the middle. The
glyph already points the right way, but an arrow held down is read where it
lands rather than looked at, and a mark that arrives on the left and one that
arrives on the right are told apart before either is a shape. Clamped inside
the panel margins, so the throw shortens on a narrow window rather than
taking the ring off the edge of it.

A **figure** replaces the ring with its words alone, and stays longer, since
it is read rather than recognised: the speed after `[`, `]` or Backspace, the
chapter after PgUp or PgDn, the track after J or #, and after C whether
subtitles are showing at all, which the subtitles button does not say, not
being a switch. The icon set has no glyph for any of them and the figure is
the whole answer. A frame step and the one-second seek raise nothing: the
picture and the timeline already answer.

A **notice** from the player is a sentence alone, centred, reason first —
*Unrecognized file format — could not play Game of Thrones · S07E06* — because
the capsule elides at the bar's width and the reason is the part worth
keeping. It holds longer still, and then until the next key or pointer event,
so a failure glanced away from is still there. No part of any of it takes the
pointer: it sits where a click means play or pause.

A notice that something did not happen — a file that would not play, a path
with nothing at it, a sync that found no answer, settings that could not be
saved — has the Warning mark before its words (`MainWindow.flash-mark`) at
Lit, inside the capsule. News has none: *Sound is back*, *Subtitles moved
5.0 s earlier*. The mark is for failures and the armed reset alone, and is
never on screen at rest.

Raised while a panel is open, a notice is that panel's answer — *Sync to
speech* saying what it found — and sits with it instead: to the panel's left,
one offset away and in line with the row that asked, and to its right only
where the panel is too far left for the words to fit before it. A notice no
row is known to have asked for is level with the panel's top edge. It comes
out sideways from the panel's edge rather than rising, and fades out with the
panel when that closes.

### The end-of-file pill
A capsule as tall as the bar's row of buttons (`MainWindow.end-h`), in the
middle of the picture where
the film stopped: a bar glyph and the name of what comes next (*E04 ·
Slabtown*, in the words its playlist row uses) in Prompt at Lit,
`MainWindow.end-gap` apart, inset `MainWindow.end-pad`. It
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
as at the end of one. At the end of the last
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

**At the start of a file left part way it asks.** A file chosen on purpose —
a playlist row, previous or next, the way on, a file or folder opened — that
was left part way is held, paused on the frame where it was left, and the same
pill asks which: *Continue from 12:34*, with the play glyph, and *From the
beginning*, with Restart's. By its time, because that is what the choice is
made by: the middle of something, or near its end. It is the same surface as
the end-of-file pill — a file's start and its end are never at once — so it
has the same glass, the same type and the same stop on the bar: it takes the
ring as it comes up, and whatever panel the file was chosen from goes, so
choosing an episode and going on with it from the keyboard is Enter, Enter.
Space goes on, Home starts over, and so does playing or seeking by any other
means. The corner pill stands down while it asks.

Not every saved place is a question. A file watched to its end, or left in
its last minute or its last twentieth — whichever is longer — starts over: its
credits are nothing to go back to. One left in its first minute starts over
too: that is a file opened and closed, and seconds are nothing to ask about.
The way in's *Continue* has already said which, and resumes. So do the media
keys, pressed by someone who may not be looking.

Autoplay cannot ask — nobody may be watching, and a pause held for an answer
would end the evening — so a file it brings in starts from the beginning, and
the place passed over is offered in the corner instead; see *Going back to
where you were*, under the credits pill.

Restart alone is the least likely thing anyone wants there. With two, each
choice is a capsule of its own, set `MainWindow.pill-pad` inside the pill so
the curves are concentric, with its words still `MainWindow.end-pad` from the
pill's edge, a button's gap apart, each taking the Button wash on its own and
the Ring wash under the keyboard.

**It makes way rather than standing down.** In a small window a panel can open
over the middle of the picture, and in a short window the seek preview reaches
it too. That is glass on glass, but where the preview stands down for a panel,
this cannot: it may be the question the player is paused on. So beside an open
panel it slides, at a page's pace, to the middle of the room the panel leaves,
which in the narrowest window (`MainWindow.min-width`) still holds the widest
pill; and over the seek
preview it rises clear, and settles back once the preview has gone by. Only for
a panel or a preview actually in its way.

### The credits pill
While an episode's closing credits play, the end pill's single capsule offers
what comes next with the Next glyph, named as the end pill names it — the next
file as its row says it, or at the end of a season the season after it — and
*Next episode* only when neither can be said. It is no wider than the bar's
right group, the corner it keeps to; a long title elides rather than reach over
the names. The credits are a chapter named for them —
*End Credits*, *Credits*, *Outro*, *ED*, *Ending* — in the file's second half,
and the pill stays from there to the end. Where no chapter has a real name —
each titled with its own start time, *Chapter 7*, or nothing, as broadcast
episodes split at their act breaks usually are — the last chapter is the
credits if it runs three minutes or less. A file without chapters, or with
chapters that say nothing either way, has its credits heard instead, as its
title sequence is — see *Skipping the titles* below: the last ten minutes of
this episode and of another of its season are compared, and the stretch of
music they share is the credits, provided it ends within five minutes of the
end. A short show's credits can run under ten seconds, and from eight seconds
up they count — but one shorter than a title sequence has to be the very end
of one of the two episodes, or match what is already known to be the other's
credits, because a cue that comes round every episode is that short too.
A film never gets one. It needs somewhere to go: a later file, or at the
end of a season the next season found beside it.

It keeps out of the middle, where the names roll, and sits at the bottom right
on the bar's lines: at `MainWindow.panel-margin` while the bar is away, lifted
`Rows.offset` above the bar while it is up, riding the bar's own fade
between the two.
Its arrival wakes the bar once and holds nothing up — the clock runs out, the
bar leaves, the pill stays, and doing nothing is watching the credits. It also
takes the keyboard's ring, so Enter is the whole of going on; the ring stays
with it after the bar leaves and goes when it does, so Enter never presses a
pill that has gone. Like the
end pill it is not tied to the idle clock. It stands aside for an open panel
and for the seek preview when the preview reaches its column; a hover label
that would land on it goes up a row instead. Glass never lies on glass.

**Skipping the titles.** The same pill at the other end of an episode: while
its title sequence plays the capsule says *Skip intro*, with the Skip glyph —
two arrowheads and no bar, further into this file rather than on to the next —
in the same corner, on the same lines, and for the same reason — a sequence is
something people watch, so the offer keeps out of its way and doing nothing is
watching it. It arrives as the credits pill does: one wake of the bar, the
ring, Enter. It lands on the first frame after the sequence, exactly, and goes
a moment before that (`state.INTRO_TAIL`): an offer that is over before it is
read is noise.
It needs nowhere to go, so the last episode of a season has it too; a film
never does, for the reason it has no credits pill.

**Going back to where you were.** The same pill again, for a file autoplay
brought in from the beginning though it had been left part way: for its first
seconds (`arrival.OFFER_FOR`) it says *Continue from 12:34*, with the play
glyph, and pressing it
goes there. Autoplay got there because the file before played to its end, so
the person is watching in order and the saved place is from an earlier
viewing; starting there with nobody at the controls is the worse mistake, but
the earlier viewing may be exactly what they rewatched the last episode to get
back to, and this keeps it one press away. It is offered from the file's first
frame and over *Skip intro*, which usually starts the file too — going on from
the place is past the sequence as well, and after that the sequence has the
corner back. Seconds of the file, not of the clock: paused, the offer waits.

The sequence is a chapter named for it — *Opening*, *Intro*, *OP*, *Main
Titles*, *Theme Song* — in the file's first half, to the next mark, and no
longer than five minutes. A *Recap*, *Previously On* or *Cold Open* is the
episode and is never skipped. Most files name nothing, and then the sequence
is heard: the first twenty minutes of this episode and of another in the
playlist — same show, same season — are compared, and the longest stretch of
the same sound in both, fifteen seconds or more, is the titles in each. The
closing credits are heard the same way from the other end. Up to four
episodes are tried: first those already known to hold the sequence, nearest
first, listening only to where it lies in them; then the nearest not yet
heard. Not just the two either side, because a season has its odd ones, and
a show made in eleven-minute halves has titles on only every other episode —
compared with its neighbours alone, every episode between two without found
nothing. That takes a few seconds the first time and is kept for both; an
episode that shares nothing with any of the four has no sequence and is not
asked again, until another found to share it says otherwise. The player says nothing either way: a sequence that was not
found is a pill that does not appear.

### The way in
With no film loaded, a panel of glass centred in the window, over the backdrop.
With nothing to continue, it is headed with the player's own name (*Death by
MPV*, in the Named role, where a show's name goes over Continue), because then
the panel is about the player rather than a film, and the first window anyone
ever sees should say what is asking before it says *Open file…*:

- **Continue**, first, when there is a film to continue: a play glyph at Body,
  the film's name at Strong in Body size — the one line on the window about a
  film rather than the player — then the resume measure and, where a playlist
  row keeps its running time, what is left at Body in Readout size: *43 min left*. It is the
  newest file mpv still holds a position for that is still on disk; the same
  thing the backdrop is a frame of. A group gap follows it, because it is a
  film and the rows below it are dialogs, and the panel widens
  (`MainWindow.invite-w`) so an episode keeps its number beside the time. An
  episode's show goes on a heading line of its own above it, in the Named role,
  where a playlist puts it: sharing the row, the show's name would take the
  room and the episode number would be what elided. A film watched into its
  credits is not offered.
  Enter presses it at once, without first raising the ring: it is the first
  row, and the one the window is opened to get back to.
- **Open file, Open folder, Settings and Shortcuts** as shortcut rows with their keys, then
  a line in Hint size saying a file can be dropped anywhere — **on the platforms
  where one can.** The drop target is Windows-only; Linux would be a different
  implementation against a different protocol and does not have one yet. Both
  places that offer a drop — this line and the POINTER row on the reference
  page — are conditioned on it, and where there is none the line has nothing to
  say at rest and takes no height. *Guess well, and never lie about it* is a
  product principle, and a printed instruction that does nothing is the
  plainest way to break it. **The gap goes with the line.** A note carries its
  own group gap above it, so a window with nothing to add leaves its last row
  the same air the heading has above it, not a gap of glass below the thing that
  ended, which would be the Gap Goes Above Rule read backwards.

While an open is in flight Continue and the two open rows fade to half and stop
answering — to the pointer and to every key — and the line names what is
arriving; Settings stays at full strength, because it still works. Carrying a
failure it widens as for Continue and offers nothing to open: a Title headline
at Strong, the driver's own error text at Body in Hint size wrapping to three
lines, a line in Hint size saying what usually puts it right (a driver update,
except for shaders the driver would not build, where a current driver means
the bug is the player's), on its own account rather than beside the drop
line, which can be gone; and after a group gap two rows: *Copy details*, which
puts the headline and the driver's words on the clipboard and says *Copied*
for a moment (`MainWindow.copied-timer.interval`), and *Close the player*.

## Do's and Don'ts

### Do:
- **Do** derive every length from the constants at the top of the component.
  Panel geometry is read once per frame by the renderer; a binding that asks a
  layout for an answer costs a full layout pass each time.
- **Do** take every colour from the `Paint` global. A new role gets an existing
  step; a colour written at its use site is how the ladder drifts.
- **Do** keep radii concentric: inner = outer − gap, as `Rows.panel-radius` is
  `Rows.radius` + `Rows.gap`.
- **Do** express state in two channels where a control carries two meanings —
  glyph brightness for what it controls, held background for what it opened.
- **Do** size a bevel as a fraction of its own corner radius.
- **Do** leave a surface more padding than its bevel is wide.
- **Do** capitalise the interface's own labels and leave the user's content in
  its own case.
- **Do** fade glass with the surface above it.
- **Do** show where the keyboard is with the Ring wash on a pane of glass of its
  own, and only while the keyboard is being used.
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
- **Don't** add a text step between two existing ones, or move Dim. There is
  no gap left wide enough to put one in, and the floor is the contrast margin
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
- **Don't** animate for its own sake. Motion here is a response:
  `Motion.highlight` for a wash, `Motion.track-grow` for a track,
  `Motion.surface` for a surface and `Motion.switch` for a switch or a mute,
  ease-out, no bounce, no overshoot. Four things depart from that, each for a
  reason that is about what is moving rather than about how it looks:
  - the **backdrop's** arrival (`pipeline.BACKDROP_ARRIVAL`), the one motion
    that is not a response to anything: it is the window's light changing, not
    a control answering;
  - a **drill** (`Motion.drill`), ease-in-out, moving the page sideways and the
    panel's own height with it. Longer than a surface because it is a journey
    rather than an arrival, and eased at both ends because it starts and stops
    in place rather than coming from nowhere. The panel and the page are one
    number, so the box and what is in it arrive together. **Both trees move,
    and by the same number**: the settings pages behind their rows, and the
    playlist between its list of parts and a part's page. A panel with two
    levels that changes which one it shows in a single frame leaves no
    account of what happened, and that is as true of a season as of a page of
    sliders. Never to a new window, though: while the window is being resized,
    the panels' heights take their new size in the frame they are given it,
    since eased they would trail the window's edge for as long as the drag
    lasted;
  - a **glide** (`Motion.glide`), ease-in-out, carrying a list back to its
    current row when the angle pointing to it is pressed. Longer than a drill,
    because the distance is anything up to a whole season and at a drill's
    length the rows go past as a smear;
  - the **seek preview's** fade, half of `Motion.surface`, the one surface with
    a duration of its own. It is not announcing itself; it is following a
    pointer, and at a surface's pace it reads as trailing the hand rather than
    belonging to it.

  **All of it stops with one switch**: Animations, on the settings panel's
  accessibility page. Every duration that moves something is a token in the
  `Motion` global, and each is zero with the switch off, so surfaces, pages,
  washes, tracks and switches arrive in the frame they were asked for; the
  backdrop, which the pipeline fades rather than Slint, is simply there. The
  waits stay — the tip's delay and grace, the flash's dwell, the reset's
  confirmation, the idle clock — because a wait is not motion. A new
  animation takes its duration from `Motion`, never a number of its own, or
  it is the one thing left moving for someone who asked for nothing to.
- **Don't** substitute an icon library. The set in `public/icons/` is drawn on
  one grid as one family; extend it rather than replacing it.
- **Don't** show an icon at any size but the one it is drawn at (the bar's,
  `IconButton.glyph`) and three quarters of it (a row's,
  `MenuHeading.icon-size`). Those keep the drawings' line on whole and half
  pixels, and both stay legible; at two thirds the line falls a third of a
  pixel off the grid, and at a half every line lands on a pixel boundary and
  turns to two grey rows, so detailed icons blur into blobs. A smaller size
  needs its own drawing. There are **no** exceptions.
