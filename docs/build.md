# building from source

## build dependencies

  * rust (<https://www.rust-lang.org/tools/install>)
  * cmake (<https://cmake.org/download/>)

On **macOS**, `Xcode` and the `Xcode Command Line Tools` are required.  The
latter can be obtained by running `xcode-select --install`. CMake can be
installed with `brew install cmake`.

## build & installation

  Before proceeding, make sure the build dependencies have been installed.

  Then, run:

    $ cargo install rx

  This will download `rx` and install it under `~/.cargo/bin/rx`.
  If you prefer a different install location, you can specify it
  via the `--root <prefix>` flag, where `<prefix>` is for example
  `/usr/local`.
