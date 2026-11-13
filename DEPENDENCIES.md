# Dependencies for Guandan Native

This inventory describes the Cargo application at the repository root. The old
Node application under `legacy/` is not part of the native build.

## Rust toolchain

- Rust compiler (`rustc`) and Cargo, from a current stable Rust toolchain.
- Verified versions: `rustc 1.99.1` and `cargo 1.99.1` on Fedora 44 x86-64.
- `rustfmt` and `clippy` are optional development components, needed only for
  the formatting/lint commands in the README. They are not needed to build/run.

The verification toolchain was installed temporarily in `/tmp/guandan-cargo`
and `/tmp/guandan-rustup`. It was not installed into your normal shell PATH.
Do not rely on those temporary directories for your own installation.

If Rust is not already installed, use the official rustup installer:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
rustc --version
cargo --version
```

Optional development components:

```sh
rustup component add rustfmt clippy
```

Existing rustup users can use `rustup update stable` to update their stable
toolchain and `cargo +stable build` to explicitly select it.

Reference: <https://rust-lang.github.io/rustup/installation/>

## Fedora build tools

```sh
sudo dnf install gcc glibc-devel binutils pkgconf-pkg-config curl ca-certificates
```

- `gcc`: the C compiler and linker driver used by Cargo's Linux target.
- `glibc-devel`: C runtime startup objects and development linker files.
- `binutils`: the linker and `strip` (release builds enable stripping).
- `pkgconf-pkg-config`: native-library discovery used by dependency build scripts.
- `curl`, `ca-certificates`: toolchain download and HTTPS certificate support.

Cargo downloads dependencies during the first build, so network access to
crates.io is needed then. These are system packages, not Rust crates.

## Direct Rust dependencies

| Crate | Version resolved in Cargo.lock | Purpose |
| --- | --- | --- |
| `eframe` | `0.31.1` | Native egui window, rendering, input, clipboard |
| `rand` | `0.8.8` | Deck shuffling and seeded simulations |
| `serde` | `1.0.229` | Serialization derives for messages/settings |
| `serde_json` | `1.0.154` | JSON messages and settings |

`eframe` enables `default_fonts`, `glow`, `x11`, and `wayland`; its default
features are disabled. `serde` enables `derive`. The `screenshot` feature is
optional and enables eframe's screenshot verification support.

Cargo installs these crates and every transitive dependency automatically.
Do not run `cargo install eframe` or install these individually. The complete
default Linux dependency inventory is in
[DEPENDENCIES-linux-x86_64.txt](DEPENDENCIES-linux-x86_64.txt). The authoritative
[Cargo.lock](Cargo.lock) also includes dependencies for other platforms and
inactive optional configurations; those entries do not mean you need an Android,
Windows, macOS, Vulkan, or WebAssembly SDK to build this application on Linux.

Inspect the actual graph with:

```sh
cargo tree --locked --target x86_64-unknown-linux-gnu
```

## Linux graphical runtime

The app needs a graphical desktop session and a working OpenGL/EGL driver.
On Fedora KDE most of these libraries will already be installed. The following
packages cover both enabled window backends:

```sh
sudo dnf install \
  libgcc glibc \
  libX11 libX11-xcb libxcb libXcursor libXi libXrandr \
  libxkbcommon libxkbcommon-x11 \
  libwayland-client libwayland-cursor libwayland-egl \
  libglvnd-egl libglvnd-glx
```

| Component | Fedora packages |
| --- | --- |
| Standard C/math/GCC runtime | `glibc`, `libgcc` |
| X11 window/input/cursor support | `libX11`, `libX11-xcb`, `libxcb`, `libXcursor`, `libXi`, `libXrandr` |
| Keyboard layout support | `libxkbcommon`, `libxkbcommon-x11` |
| Wayland window/cursor/EGL support | `libwayland-client`, `libwayland-cursor`, `libwayland-egl` |
| OpenGL/EGL dispatch libraries | `libglvnd-egl`, `libglvnd-glx` |

Keep your existing GPU driver. The verification used Mesa software rendering
(`mesa-dri-drivers`, with `LIBGL_ALWAYS_SOFTWARE=1`) for screenshots. Mesa's driver
package is not an additional requirement if your installed GPU driver already
provides working OpenGL.

The enabled backends load display/graphics libraries at runtime, so `ldd` alone
does not list all of them. GUI `*-devel` packages were not needed for the verified
build with the current feature selection. Default font fallbacks are bundled by
eframe; Cinzel is embedded from `assets/fonts/Cinzel-Variable.ttf` for
the Diamond Vault interface, with its SIL Open Font License 1.1 and attribution included
in that directory. There is no runtime font download or required system font
installation. The build does not require GTK, Qt, WebKit, SDL,
OpenSSL development headers, CMake, Node.js, or npm.

If a Wayland session/driver has trouble, run through an available X11/Xwayland
display:

```sh
env -u WAYLAND_DISPLAY ./target/release/guandan
```

The optional headless server and `cargo test --no-default-features --locked`
do not require the GUI libraries or a display session.

## Tailscale

Tailscale is an external runtime dependency for remote tailnet play, not a Cargo
dependency. Verification used the installed Tailscale CLI `1.104.5`; this is not
an application-enforced minimum version. Everyone must connect their computer to
the tailnet, and the host must allow the chosen TCP port (3000 by default).
The CLI's `tailscale ip -4` command supplies automatic address detection; an IP
can also be entered manually. Solo and ordinary LAN play do not need Tailscale.

## Migration-only tools

Node.js was used to export the legacy rule examples into
`tests/fixtures/combos.json`. That file is checked into the source. Normal Cargo
builds and tests do not invoke Node, Python, a browser, or Playwright. The script
`scripts/export-legacy-fixtures.cjs` needs Node only if you choose to regenerate
the fixture file. Testing the archived web application separately also needs
Node/npm and its own dependencies.

## Build

Keep Cargo.lock and build with `--locked`:

```sh
cargo build --release --locked --bin guandan
./target/release/guandan
```

The output is dynamically linked Linux ELF, not a universal static binary.
If moving an executable to another Linux machine, that machine needs compatible
runtime libraries. Building on the destination machine avoids a newer-host
glibc compatibility mismatch.
