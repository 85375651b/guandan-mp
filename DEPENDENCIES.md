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
in that directory. `DroidSansFallbackFull.ttf` is also embedded for Simplified
Chinese and Chinese player names, with its original Apache 2.0 notice/license.
There is no runtime font download or required system font
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

---

# 中文依赖说明

本清单适用于仓库根目录的 Cargo 原生应用；`legacy/` 中的旧 Node 应用不属于原生编译。

## Rust 工具链

- 需要当前稳定版 Rust 编译器 `rustc` 与 Cargo。
- 已验证版本为 Fedora 44 x86-64 上的 `rustc 1.99.1` 与 `cargo 1.99.1`。
- `rustfmt` 和 `clippy` 仅供开发时格式检查与静态检查，普通编译和运行无需这两个组件。

早期验证使用过 `/tmp/guandan-cargo` 和 `/tmp/guandan-rustup` 临时工具链，
没有加入正常 shell 的 PATH；自行安装时不要依赖这些临时目录。
如尚未安装 Rust，可使用官方 rustup 安装器：

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
rustc --version
cargo --version
```

开发组件可选安装：

```sh
rustup component add rustfmt clippy
```

已有 rustup 的用户可用 `rustup update stable` 更新稳定工具链，
并用 `cargo +stable build` 明确选择它。参考：<https://rust-lang.github.io/rustup/installation/>。

## Fedora 编译工具

```sh
sudo dnf install gcc glibc-devel binutils pkgconf-pkg-config curl ca-certificates
```

- `gcc`：Cargo Linux 目标使用的 C 编译器与链接驱动。
- `glibc-devel`：C 运行时启动对象及开发用链接文件。
- `binutils`：链接器和 `strip`，release 编译会剥离符号。
- `pkgconf-pkg-config`：依赖的构建脚本用于发现本机库。
- `curl`、`ca-certificates`：下载工具链与验证 HTTPS 证书。

首次编译时 Cargo 需要访问 crates.io 下载依赖。以上是系统软件包，不是 Rust crate。

## 直接 Rust 依赖

| Crate | Cargo.lock 锁定版本 | 用途 |
| --- | --- | --- |
| `eframe` | `0.31.1` | 原生 egui 窗口、渲染、输入与剪贴板 |
| `rand` | `0.8.8` | 洗牌和固定种子模拟 |
| `serde` | `1.0.229` | 消息与设置的序列化派生 |
| `serde_json` | `1.0.154` | JSON 网络消息与设置 |

`eframe` 关闭默认功能，启用 `default_fonts`、`glow`、`x11`、`wayland`；
`serde` 启用 `derive`。可选 `screenshot` 功能用于 eframe 截图验证。
Cargo 会自动下载直接和间接依赖，无需手动 `cargo install eframe` 或逐个安装这些库。

完整的默认 Linux 依赖清单见 [DEPENDENCIES-linux-x86_64.txt](DEPENDENCIES-linux-x86_64.txt)。
权威锁定文件 [Cargo.lock](Cargo.lock) 也包含其他平台与未启用功能的依赖，
并不表示在 Linux 上编译需要 Android、Windows、macOS、Vulkan 或 WebAssembly SDK。
查看实际依赖图：

```sh
cargo tree --locked --target x86_64-unknown-linux-gnu
```

## Linux 图形运行环境

需要图形桌面会话和可用的 OpenGL/EGL 驱动。Fedora KDE 通常已有大部分库，
以下软件包覆盖启用的两种窗口后端：

```sh
sudo dnf install \
  libgcc glibc \
  libX11 libX11-xcb libxcb libXcursor libXi libXrandr \
  libxkbcommon libxkbcommon-x11 \
  libwayland-client libwayland-cursor libwayland-egl \
  libglvnd-egl libglvnd-glx
```

| 组件 | Fedora 软件包 |
| --- | --- |
| 标准 C、数学及 GCC 运行库 | `glibc`、`libgcc` |
| X11 窗口、输入与光标 | `libX11`、`libX11-xcb`、`libxcb`、`libXcursor`、`libXi`、`libXrandr` |
| 键盘布局 | `libxkbcommon`、`libxkbcommon-x11` |
| Wayland 窗口、光标及 EGL | `libwayland-client`、`libwayland-cursor`、`libwayland-egl` |
| OpenGL/EGL 分派库 | `libglvnd-egl`、`libglvnd-glx` |

请保留现有 GPU 驱动。截图验证使用过 Mesa 软件渲染（`mesa-dri-drivers` 和
`LIBGL_ALWAYS_SOFTWARE=1`）；如果现有驱动已提供正常 OpenGL，就不需要额外安装 Mesa 驱动。

图形后端会在运行时动态载入部分库，因此仅用 `ldd` 无法列出全部依赖。
在当前已验证的功能配置中，不需要图形库的 `*-devel` 软件包。
eframe 自带默认后备字体；程序另嵌入 Cinzel 与 Droid Sans Fallback，
分别提供主题英文字体与中文显示，版权及 SIL OFL 1.1、Apache 2.0 许可证均在
`assets/fonts/`。不需要运行时下载或安装系统字体。
编译无需 GTK、Qt、WebKit、SDL、OpenSSL 开发头文件、CMake、Node.js 或 npm。

如 Wayland 或驱动有问题，可使用已有 X11/Xwayland 显示：

```sh
env -u WAYLAND_DISPLAY ./target/release/guandan
```

可选无界面房主与 `cargo test --no-default-features --locked` 不需要图形库或桌面显示会话。

## Tailscale

Tailscale 是跨网络 tailnet 联机的外部运行时依赖，不是 Cargo 依赖。
验证时使用过 Tailscale CLI `1.104.5`，但应用没有强制该最低版本。
所有人须连接同一 tailnet，房主须放行所选 TCP 端口（默认 3000）。
`tailscale ip -4` 用于自动检测地址，也可手动输入 IP。
单人游戏与普通局域网游戏不需要 Tailscale。

## 仅迁移时使用的工具

Node.js 曾用于将旧规则样例导出到已提交的 `tests/fixtures/combos.json`。
正常 Cargo 编译和测试不会调用 Node、Python、浏览器或 Playwright。
只有重新生成样例时，`scripts/export-legacy-fixtures.cjs` 才需要 Node。
单独测试归档 Web 应用则需要 Node/npm 及其自身依赖。

## 编译

请保留 Cargo.lock 并使用 `--locked`：

```sh
cargo build --release --locked --bin guandan
./target/release/guandan
```

生成的是动态链接 Linux ELF，不是通用全静态程序。
移到其他 Linux 电脑时需要兼容的运行库；在目标电脑本机编译可避免较新构建环境导致的 glibc 不兼容。
