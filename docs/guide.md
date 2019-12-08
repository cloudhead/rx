# rx/guide

To use `rx` effectively, the user must understand a few basic properties of the
tool. First, it is a *modal* editor. This means that at any given time, the
user is in one of the supported *modes*. Each mode is designed to carry out a
certain type of task. Certain tasks can be accomplished in multiple modes, and
it is up to the user to choose which mode is most appropriate for that task.

Second, `rx` is designed to be *configurable* to the user's preferences. This
is done in a plain-text file format called an `rx` *script*. This file is meant
to be carried around by the user from machine to machine, and can be exchanged
with other users. `rx` provides a good default configuration, but this cannot
cater to all workflows.

Third, `rx` is focused on the *work*. The traditional user interface has been
stripped down to its core elements to reveal the work, and bring focus to the
creative process. A large part of the design of `rx` is tailored to making this
process more enjoyable and intuitive.

## overview

When the user starts `rx`, a new *session* is initialized with a blank *view*.
Views are usually associated with files, but can also be used as scratch pads.
Towards the bottom of the window is the status bar, which displays information
about the session and active view. Underneath is the *command line*, where the
user can enter commands, and on the left is the color *palette*.

At all times, there is one *active* view. Many of the commands in
`rx` operate on the currently active view. Views are activated by clicking
on them with the mouse, or cycling through them with <kbd>tab</kbd>.

## modes

`rx` has three primary modes: *normal*, *visual*, and *command*.

**Normal mode** is where users spend most of their time - it is the default
mode, and the one in which pixels are brushed onto a view. Normal mode
can always be reached by pressing <kbd>esc</kbd>.

**Visual mode** is the mode in which pixels can be selected and manipulated
visually, but not painted. Visual mode is activated with the <kbd>v</kbd> key.

**Command mode** finally, allows the user to run commands on the session,
the views, or the visual selection. Command mode is activated when
entering a command. Commands start with a colon (`:`), for example
`:q!` is the command to quit without saving.

> **Tip:** Key bindings are *mode-specific*. This means some shortcuts will
> work accross all modes, while others only work in one or two modes.

> **Tip:** For those users who have used `vi`-like editors, `rx` is a little
> different, by distinguishing between normal and command mode.

## settings

A large part of the functionality and tools within `rx` are configured with
*settings* that can be configured on the fly. Updating a setting is usually
done with the `:set` command, which has the form `:set <key> = <value>`,
for example, setting the session background to grey can be done with:

    :set background = #333333

Certain settings are on/off switches, such as the `vsync` setting. These can be
turned on with eg. `:set vsync = on` or `:set vsync`, and *off* with `:set
vsync = off` or `:unset vsync`. Alternatively, they can be toggled with the
`:toggle` command.

> **Tip:** The current value of a setting can be displayed with the `:echo`
> command.  For example: `:echo background` or `:echo grid/spacing`.

## saving & loading work

`rx` is built around the *PNG* image file format. Loading a file is as
simple as using the `:e` (edit) command followed by the file path, eg.:

    :e tao.png

and saving is done with the `:w` (write) command. To save under a
different path, the user can specify the file path explicitly, eg.:

    :w tao-v2.png

To close a file, the `:q` (quit) command can be used. Note that closing the
last remaining view will quit the session.

> **Tip:** The `:e` command can also be used to load entire directories of
> files. Simply specify the directory path, and `rx` will load all files under
> that path.

## animating work

`rx` was designed from the very beginning to create animated pixel work. All it
takes is adding frames to an existing view by pressing `<return>` or entering
the `:f/add` (add frame) command. The animation is displayed next to the view,
and continuously cycles between the frame.

To change the frame delay, the `animation/delay` setting can be used. For example,
to set a delay of *250 milliseconds* between frames, the user can enter the command:

    :set animation/delay = 250

This will set the animation to cycle at about 4 frames/s.

If there are too many frames in the animation, it's easy to remove frames by
pressing <kbd>backspace</kbd>, or entering the `:f/remove` command. Frames can
also be cloned from existing frames with the `:f/clone` command, which takes an
optional frame number to clone, and otherwise clones the last frame.

When working with animations, it can be useful to work on multiple frames
at a time. Here, the `multi` brush mode comes in handy. It can be activated
by holding `<shift>`, or entering `:brush/set multi`. Drawing on a frame
now also draws on all subsequent frames.

### loading animations

When loading an animation from a `.png` file, frame information will not
be present. This is where the `:slice` command comes in. If you have
a six frame animation strip, simply enter `:slice 6`. This will convert
the image into a sequence of frames. In the future, this information
will be saved alongside the `.png`.

> **Tip:** To resize the animation frames, try the `:f/resize` command. Eg.
> `:f/resize 16 16`.

## painting

Painting in `rx` is done with the **brush** tool, which can be configured in
terms of *size* and *mode(s)*. Changing the brush size is done via <kbd>[</kbd>
and <kbd>]</kbd>, or via the `:brush/size -` and `:brush/size +` commands,
respectively. Brush *modes* are behaviors that can be activated on the tool.
The following brush modes exist:

* **erase**: erase pixels.
* **multi**: paint on multiple frames at a time.
* **xsym**: x-axis (horizontal) symmetry painting.
* **ysym**: y-axis (vertical) symmetry painting.
* **perfect**: "pixel-perfect" mode (pixel filtering).
* **xray**: see the underlying pixel color at all times.

Brush modes are activated with the `:brush/set` command and deactivated with
`:brush/unset`. They can also be toggled with `:brush/toggle`. For example,
toggling *xray* mode would be `:brush/toggle xray`.

> **Tip:** Any number of brush modes can be combined together! For example,
> try combining `xsym` and `ysym` for radial symmetry.

Brush color can be set by picking a color from the palette, or using the
**sampler** tool (<kbd>ctrl</kbd>) and picking a color from the view.  `rx`
displays two colors at all times in the status bar: the *foreground* color,
used by the brush, and the *background* color, which keeps track of the last
color used.  The foreground and background colors can be swapped by pressing
<kbd>x</kbd>, or using the `:swap` command.

To undo an edit to the active view, press <kbd>u</kbd> (or `:undo`), and to
redo, press <kbd>r</kbd> (or `:redo`).

## navigating

Navigating the `rx` session is easy, and involves the following commands:

* `:zoom +` and `:zoom -`, (shortcuts <kbd>.</kbd> and <kbd>,</kbd>) to zoom
  the active view.  This can also be accomplished with the mouse wheel. In
  addition, to set a specific zoom level, `:zoom` can take a multiplier, eg.
  `:zoom 1.0` sets the zoom to 100%.
* `:pan <x> <y>` to pan the work space - this can also be achieved by
  holding <kbd>space</kbd> and dragging with the mouse.
* `:v/center` to center the active view (shortcut <kbd>z</kbd>).
* `:v/prev` and `:v/next` to cycle through the views (shortcut <kbd>tab</kbd>)

## manipulating pixels

Visual mode allows the user to manipulate pixels by operating on visual
selections.  To activate this mode, enter `:visual` into the command line, or
simply press <kbd>v</kbd>.  The view border turns red, indicating that visual
mode is active. Dragging with the mouse anywhere in the view creates a
selection that can be moved around. The selection can be expanded to the frame
with `:selection/expand`, or <kbd>\\</kbd>. It can be moved one whole frame at
a time forwards with <kbd>w</kbd>, and backwards with <kbd>b</kbd>
(`:selection/jump`).  It can also be nudged by one pixel in any direction with
<kbd>h</kbd>, <kbd>j</kbd>, <kbd>k</kbd> and <kbd>l</kbd> keys
(`:selection/move`).

With a selection in place, the `:selection/yank` (shortcut <kbd>y</kbd>)
command can be used to create a copy which can be then placed anywhere with the
`:selection/paste` command, by either left-clicking with the mouse, or pressing
<kbd>p</kbd>.

There are several other useful shortcuts that operate on the current selection,
such as:

* <kbd>[</kbd> and <kbd>]</kbd> to inset and offset the selection by one pixel.
* <kbd>f</kbd> to fill the selection with the foreground color.
* <kbd>d</kbd> to cut the selection contents.
* <kbd>e</kbd> to erase the selection contents.

## working with transparency

Sometimes, it's useful to distinguish transparent pixels from *black* (`rx`'s default
background color). In these cases, the **checker** can be activated by entering
`:set checker = on` (or simply `:set checker`). Turning the checker off is a matter
of calling `:set checker = off`.

## using the grid

When working with certain kinds of images, it may be helpful to work around a
pixel grid. `rx` can display a grid with the `:set grid` command. It's spacing
and color can be controlled with the `grid/spacing` and `grid/color`
settings. For example,

    :set grid/spacing 4 4
    :set grid/color #ff0000

will set the grid to *red*, and *4 by 4* pixels.

## debugging

If comes the need to debug performance problems, `rx` can be set to show
some runtime information with the `debug` setting. When *on*, frame update
and render time will be displayed in the upper left corner, as well
as memory consumption.

Alternatively, `rx` can be started with the `-v` command-line flag, which
turns on verbose logging.
