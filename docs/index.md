
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

`rx` currently supports Linux, macOS and Windows.

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

With [cargo][cargo] installed, it's as simple as:

    $ cargo install rx

See the [build][build] section for further details.

### Binaries

At the moment, only *Linux* binaries are available. You can download the
latest version here:

> [rx-0.3.0-x86_64-unknown-linux-gnu.AppImage][app]

This is an [AppImage][appimage], a self-contained application. Before opening
it, make it executable with `chmod +x`. You can then double-click it or execute
it directly from your terminal.

Feel free to rename it to `rx` and move it to your `PATH`. To uninstall,
simply delete the file.

[app]: https://github.com/cloudhead/rx/releases/download/v0.3.0/rx-0.3.0-x86_64-unknown-linux-gnu.AppImage
[appimage]: https://appimage.org/

For *macOS* and *Windows*, official builds are not yet available. See the
[build][build] section to build and install *rx* from source.

*If you'd like to help the project by offering macOS or Windows builds, please
contact the author at <rx@cloudhead.io>.*

<a id="build"></a>

## build & install from source

If you have [cargo][cargo] and [cmake][cmake] installed, simply run:

    $ cargo install rx

This will download the latest *stable* release of `rx` and install it under
`~/.cargo/bin/rx`.  If you prefer a different install location, you can specify
it via the `--root <prefix>` flag, where `<prefix>` is for example
`/usr/local`.

[cargo]: https://crates.io/install
[cmake]: https://cmake.org/download/

On **macOS**, `Xcode` and the `Xcode Command Line Tools` are required.  The
latter can be obtained by running `xcode-select --install`. CMake can be
installed with `brew install cmake`.

## usage

The [guide][guide] is the best place to learn how to use `rx`.

You can also use the `:help` command (shortcut <kbd>?</kbd>) to show the
active set of key mappings and commands inside of `rx`.

## support the project

If you find this project useful, consider supporting it by sending ₿ (Bitcoin) to
the following address:

    1MpF7p9A8LJabZn7ehHpGbLcN5PCXRdGqm

or by [sponsoring][sponsor] the author on GitHub.

[sponsor]: https://github.com/sponsors/cloudhead

## bugs

If you encounter a bug, please open an issue on the [tracker][tracker], or
send an email to <rx@cloudhead.io>.

[tracker]: https://github.com/cloudhead/rx/issues

## license & copyright

*rx* is free software, licensed under the **GPL**. See the `LICENSE` file in
the code repository for more details.

&copy; 2019 Alexis Sellier

[guide]: guide.html
[build]: #build
