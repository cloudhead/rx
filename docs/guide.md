
# guide

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
user can enter commands, and on the left is the *color palette*.

At all times, there is one *active* view. Many of the commands in
`rx` operate on the currently active view. Views are activated by clicking
on them with a mouse or other pointing device, or cycling through them with
the <kbd>tab</kbd> key.

## modes

`rx` has three primary modes: *normal*, *visual*, and *command*.

**Normal mode** is where users spend most of their time - it is the default
mode, and the one in which pixels are painted onto a view with the **brush**
tool. Normal mode can be reached from any other mode by pressing
<kbd>esc</kbd>.

**Visual mode** is the mode in which pixels can be selected and manipulated
visually, but not painted. Visual mode is activated with the <kbd>v</kbd> key.

**Command mode** finally, allows the user to run commands on the session, the
views, or the visual selection. Command mode is activated with <kbd>:</kbd>.
For this reason, commands are described with a colon in front, for example
`:q!` is the command to quit without saving. Commands are submitted with the
<kbd>return</kbd> key, or loaded from a file with `:source <path>`. When
saved in a file, commands are not prefixed with a colon.

> **Tip:** Key bindings are *mode-specific*. This means some shortcuts will
> work accross all modes, while others only work in one or two modes.

## navigating

Navigating the `rx` session is easy, and involves the following commands:

* `:zoom +` and `:zoom -`, (shortcuts <kbd>.</kbd> and <kbd>,</kbd>) to zoom
  the active view.  This can also be accomplished with the *mouse wheel*. In
  addition, to set a specific zoom level, `:zoom` can take a multiplier, eg.
  `:zoom 1.0` sets the zoom to 100%.
* `:pan <x> <y>` to pan the work space - this can also be achieved by
  holding <kbd>space</kbd> and dragging with the pointing device, or
  by using the arrow keys.
* `:v/center` to center the active view (shortcut <kbd>z</kbd>).
* `:v/prev` and `:v/next` to cycle through the views (shortcut <kbd>tab</kbd>)

## painting

Painting in `rx` is done with the **brush** tool, which can be configured in
terms of *size* and *brush mode(s)*. Changing the brush size is done via <kbd>[</kbd>
and <kbd>]</kbd>, or via the `:brush/size -` and `:brush/size +` commands,
respectively. Brush *modes* are behaviors that can be activated on the tool:

* **erase**: erase pixels, by setting their alpha to `0`.
* **multi**: paint on multiple frames at a time.
* **xsym**: x-axis (horizontal) symmetry painting.
* **ysym**: y-axis (vertical) symmetry painting.
* **perfect**: "pixel-perfect" mode.
* **xray**: see the underlying pixel color at all times.

<video width="720" height="540" src="videos/painting.webm" type="video/webm"></video>

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

> **Tip:** Sometimes, it's useful to distinguish transparent pixels from
> *black* (`rx`'s default background color). In these cases, the **checker**
> can be activated by entering `:set checker = on` (or simply `:set checker`).
> Turning the checker off is a matter of calling `:set checker = off`.

### using the grid

When working with certain kinds of images, it may be helpful to work around a
pixel grid. `rx` can display a grid with the `:set grid` command. It's spacing
and color can be controlled with the `grid/spacing` and `grid/color`
settings. For example,

    :set grid/spacing 4 4
    :set grid/color #ff0000

will set the grid to *red*, and *4 by 4* pixels.

## animating work

`rx` was designed from the very beginning to create animated pixel work. All it
takes is adding frames to an existing view by pressing <kbd>return</kbd> or entering
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
by holding <kbd>shift</kbd>, or entering `:brush/set multi`. Drawing on a frame
now also draws on all subsequent frames.

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

<video width="720" height="540" src="videos/manipulating.webm" type="video/webm"></video>

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

### loading animations

When loading an animation from a `.png` file, frame information will not
be present. This is where the `:slice` command comes in. If you have
a six frame animation strip, simply enter `:slice 6`. This will convert
the image into a sequence of frames. In the future, this information
will be saved alongside the `.png`.

> **Tip:** To resize the animation frames, try the `:f/resize` command. Eg.
> `:f/resize 16 16`.

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

## configuring rx

There are three ways of configuring `rx`:

1. By entering commands in a running session or sourcing a command script with
   the `:source` command.

    Changes made in this fashion will not persist after the session is closed,
    but this may be useful for loading color configuration for example.

2. By creating an `init.rx` script in the user's configuration directory.

    This is typically `~/.config/rx` on Linux systems, and can be displayed to
    the user by entering `:echo config/dir` from inside `rx`.

3. By creating a file called `.rxrc` in the working directory from which `rx`
   is launched, or in a folder that is loaded through `rx`.

    This file, taking its name from the traditional *run command* scripts found in
    unix systems has the same syntax as `init.rx`, and can be useful when the
    user wants to load configuration specific to a set of files or project.

The *same* commands that are used in `rx` to change settings can be used inside
of scripts. This is not limited to the `:set` family of commands, but extends
to almost all commands available in `rx`. There is one important difference,
which is that commands entered in the editor start with '`:`', which is not
necessary when loading commands from a script.

> **Tip:** Comments are supported inside `rx` scripts by prefixing any line
> with a double dash, eg. `-- This is a comment`.

## working with colors

`rx` was designed to work with 32-bit sRGB images and colors. On the left
of the interface is a color palette that can be configured by the user
through `rx`'s command language: to add a color to the palette, the `:p/add`
command can be used by specifying a hexadecimal color code. To clear the
palette, `:p/clear` is used.

Palettes can be easily loaded from `rx` scripts, for example a three color
palette might be saved in the following script under the path `rgb.rx`:

    p/clear
    p/add #ff0000
    p/add #00ff00
    p/add #0000ff

and loaded with `:source rgb.rx`.

> **Tip:** Simply entering `#ff0000` as a command will expand to `p/add
> #ff0000`.

## creating and modifying key bindings

Key bindings or *shortcuts* in `rx` are configured like everything else:
in plain text with the command language. A shortcut is created with the
`:map` family of commands which has the form `map <key> <command>`. For
example:

    :map / :zoom 1.0

maps the `/` (slash) key to the `zoom` command. Printable characters can be
specified plainly, eg. `:map x :swap`, while non-printable characters have to
be enclosed in `<` `>`, eg. `:map <return> :f/add`.  The set of active key
bindings can be shown at all times with the `:help` command.

To clear all key bindings (including the default set), the `:map/clear!`
command may be used.

> **Tip:** Sometimes, it's useful to create a binding that has a different
> command associated with the *pressed* and *released* states. For example for
> tools that should only be active while a key is held down. This is done
> by adding an optional argument to `:map`, enclosed in braces. An example
> of this is the `erase` brush mode, which is mapped with:
>
>     map e :brush/set erase {:brush/unset erase}
>

## troubleshooting

If comes the need to debug performance problems, `rx` can be set to show
some runtime information with the `debug` setting. When *on*, frame update
and render time will be displayed in the upper left corner, as well
as memory consumption.

Alternatively, `rx` can be started with the `-v` command-line flag, which
turns on verbose logging.

If `rx` is crashing, run it with `RUST_BACKTRACE=1` set in your environment to
show a backtrace on crash.  It could be that the issue is related to your
configuration - in that case the program can be run without loading the
initialization script like so:

    rx -u -
