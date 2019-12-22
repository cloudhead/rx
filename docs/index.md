
![rx](gifs/demo.gif)

# *rx* is a modern and minimalist pixel editor.

*rx* is an extensible, modern and minimalist pixel editor, designed with great
care and love with pixel artists and animators in mind. It is designed to have
as little UI as possible, and takes inspiration from [vi][0]'s modal nature and
scriptability.

It was created in 2019 by [cloudhead](https://cloudhead.io) and implemented in
the [rust][1] programming language, with the goal of creating a new kind of
tool for artists and hackers.

[0]: https://en.wikipedia.org/wiki/Vi
[1]: https://rust-lang.org

---

* [Download](#download) rx
* [Browse](https://github.com/cloudhead/rx) the source code
* [Read][guide] the guide
* [Build][build] from source


## goals

* **Minimal.** Small, hackable codebase and dependency footprint.

* **Beautiful.** Clean, modern aesthetics with an attention to detail.

* **Snappy.** No perceptible input lag. Update and paint time should be
  less than `8ms` on commodity hardware at 1920x1080.

* **Configurable & extensible.** Artists should be in control of their workflow.

* **Cross-platform.** First-class Linux, macOS and Windows support.

* **Efficient.** Battery drain should be minimized. Memory footprint should
  be small.

## features

  * Built-in sprite animation support, with live preview.
  * Work with multiple files simultaneously.
  * Extensible command system.
  * Configurable with a simple text-based language.
  * HiDPI support.
  * UI scaling.
  * Undo/redo any edit.
  * Animated GIF output.
  * Multi-brush / synchronous editing.
  * Brush filtering a.k.a. "pixel-perfect" mode.
  * Visual mode for pixel manipulation.

## system requirements

### linux

On Linux, [Vulkan][vulkan] support is required.

* On debian-based systems, the drivers can be installed via the
`mesa-vulkan-drivers` package. In addition, `vulkan-tools` may be required.
* On Arch Linux, `vulkan-icd-loader` is required in addition to the drivers. See
[here][arch] for more information.

[arch]: https://wiki.archlinux.org/index.php/Vulkan
[vulkan]: https://www.khronos.org/vulkan/

### macOS

On macOS, [Metal][metal] support is required. This usually requires installing Xcode
and the Xcode Command Line Tools.

[metal]: https://developer.apple.com/metal/

### windows

On Windows, *Vulkan* support is required.

<a id="download"></a>

## download

On Linux, the simplest way to get rx is to download the AppImage from here:

> [rx-0.3.0-x86_64-unknown-linux-gnu.AppImage][app]

Then make it executable with eg. `chmod +x`, rename it to `rx` and move it
somewhere in your `PATH`.

[app]: https://github.com/cloudhead/rx/releases/download/v0.3.0/rx-0.3.0-x86_64-unknown-linux-gnu.AppImage

For macOS and Windows, see the [build][build] section.

## usage

The [guide][guide] is the best place to learn how to use `rx`.

You can also use the `:help` command (shortcut <kbd>?</kbd>) to show the
active set of key mappings and commands inside of `rx`.

## support the project

If you find this project useful, consider supporting it by sending ₿ (Bitcoin) to
the following address:

    1MpF7p9A8LJabZn7ehHpGbLcN5PCXRdGqm

or by [sponsoring][sponsor] the author.

[sponsor]: https://github.com/sponsors/cloudhead

## license & copyright

*rx* is free software, licensed under the **GPL**. See the `LICENSE` file in
the code repository for more details.

&copy; 2019 Alexis Sellier

[guide]: guide.html
[build]: build.html
