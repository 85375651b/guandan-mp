# Guandan Native

A Rust desktop application for Linux: a native window, a Rust rules engine,
three local bots, and multiplayer over Tailscale or a LAN. The application
does not use Node.js, a browser, a webview, HTML, or a JavaScript runtime.

## Build and run

See [DEPENDENCIES.md](DEPENDENCIES.md) for the complete setup checklist, Fedora
package names, toolchain details, and the full Cargo dependency inventory.

Use a current stable Rust toolchain. This project is built and tested with
Rust 1.99.1. On Fedora, install the C build tools if needed:

```sh
sudo dnf install gcc pkgconf-pkg-config
cargo build --release --locked --bin guandan
./target/release/guandan
```

Or run directly with `cargo run --release --locked`. Cargo downloads the Rust
dependencies on the first build. Subsequent solo play requires no internet.

The output is a Linux ELF executable. Rules, interface code, instructions, and
fonts are compiled into it; no `public/` directory or Node installation is
needed beside the executable. It uses the normal Linux desktop libraries and
OpenGL driver, rather than being a statically linked, universal Linux binary.
Both Wayland and X11 backends are enabled. If a desktop/driver has a Wayland
problem, try X11 with `env -u WAYLAND_DISPLAY ./target/release/guandan`.

## Solo

Enter your name and choose **Start solo game**. An internal host listens only
on loopback, on an available port. You receive a fresh hand with three bots.

Click cards to select them, then **Play selected**, or **Pass** when following.
The selected combination is previewed beside the controls. Played cards sit in
front of their owners; a silver outline marks the current winning play. The log
has a Close button, and Escape closes the log and rules windows.

On your turn, **help** selects a suggested play using the bots' level 3 strategy,
including public card counting, pass history, and partner awareness. The button
changes to **cancel**, which clears the selection or cancels a pending request.
You can adjust the selected cards, then confirm with **Play selected**. If the
strategy recommends passing, a message appears beside the controls; choose
**Pass** to confirm. Help works in both hand views, in solo and multiplayer.
The host computes suggestions in the background. Host and guests must all use
this updated build.

Choose **see straight flushes** to arrange your hand in four suit rows with
aligned columns from 2 through Ace. Empty slots make gaps easy to see; duplicate
cards remain individually selectable. Wild hearts stay in the hearts row and
can complete any suit. Jokers remain selectable in a separate column. Scroll the
grid in small windows; the action buttons stay visible. **restore regular view**
returns to the normal hand layout and keeps your selection.

The Diamond Vault interface pairs faceted sapphire artwork with blurred navy
glass panels, platinum borders, engraved corners, recessed inputs, and blue
gradient controls. Cinzel's engraved Roman serif typography is embedded in the executable.
Card indices retain a clear, simple face; hearts and diamonds use ice blue,
with brighter wildcard hearts. There are no warm or gold accents.
The palette and controls live in [src/app/theme.rs](src/app/theme.rs); procedural
artwork and cached glass blur are in [backdrop.rs](src/app/theme/backdrop.rs).
The bundled font and its SIL Open Font License are in [assets/fonts](assets/fonts).
No extra font installation or Rust dependency is required.

The interface adapts to window size, down to 420×340 logical pixels. Buttons
wrap, cards shrink to fit the hand, and the lobby stacks into one column in
narrow windows. Scroll the lobby to reach additional controls in short windows.
Long instructions scroll separately from the action buttons. The play log opens
as a floating window on smaller screens so it does not squeeze the hand. In very
short windows, the table can also scroll independently of the hand and controls.

## Play with friends on your tailnet

1. Everyone starts Tailscale and runs the same version of this desktop app.
2. The host chooses **Host game**. The default TCP port is **3000**.
3. Set **Your Tailscale IP** to the hosting computer's Tailscale IPv4 address.
   The app tries to detect it locally; **Detect Tailscale IP** retries. You can
   also find it with `tailscale ip -4` and type it into that field.
4. Copy the **On your tailnet** invitation from the table header. It looks like
   `guandan://XXX.XXX.XXX.XXX:3000`, using the real IP you entered.
5. Guests paste that invitation into **Host address** under **Join friends**.
   A plain `XXX.XXX.XXX.XXX:3000` or MagicDNS hostname also works. Replace the
   placeholder with the host's real Tailscale IP; there is no source edit needed.

These are invitations to paste into the desktop app, not browser links. Each
guest needs a native build on their machine. This source and verification target
Linux; the old browser client cannot join the native TCP protocol.

Allow the selected TCP port through the host's firewall and tailnet access rules.
No router port forwarding, Tailscale Serve, or Funnel setup is needed. Keep the
host application running: **Close table** shuts down the host and disconnects
everyone. Hosting listens on all IPv4 interfaces, so the same port can also be
used with the host's LAN IP. The advertised Tailscale address does not restrict
which interfaces can connect. Tailscale supplies the encrypted private transport;
the game itself does not implement a separate account or encryption system.

Empty seats are bots. A joining friend takes an open bot seat without restarting
the hand. A disconnected guest's seat becomes a bot while others remain. Use
**Reconnect** after a connection failure; you take an open seat and may not get
your original seat back. When everyone leaves, play stops; the next arrival
starts a fresh game. Bots cannot play a newly occupied human seat.

The app saves your name, invitation address, port, and host IP in
`$XDG_CONFIG_HOME/guandan/settings.json`, or `~/.config/guandan/settings.json`.
No game or hand history is persisted across host restarts.

## Rules carried over

- Two decks, 27 cards per player; seats 1 and 3 partner, as do seats 2 and 4.
- Hearts of the current level are wildcards, but cannot replace either joker.
  Pairs of matching small or big jokers are legal.
- All level-card suits rank above Ace. A level 7, including a non-heart 7,
  is excluded from ordinary 2–9 tribute returns.
- A 1–2 victory ends the hand immediately. Levels advance +3 for 1–2, +2 for
  1–3, and +1 for 1–4, stopping at Ace. A 1–2 or 1–3 win while playing your own
  Ace wins the game. Three consecutive 1–4 wins at your own Ace reset that team
  to 2; an intervening loss breaks the streak.
- The next deal includes single/double tribute, correct reciprocal returns,
  wildcard exemption, both-big-joker rebellion, and the corresponding leader.
  Human recipients choose a highlighted non-trump 2–9. If none exists, only
  their lowest non-wild card may be returned. Returns are revealed together.
- A finished player's unanswered winning play gives the lead to their partner.

The in-app **Rules** window and [RULES.txt](RULES.txt) explain the full table
conventions.

## Bots

All empty seats use the level 3 heuristic policy, entirely offline. No additional
dependencies, model downloads, or services are required.

- Plan the hand as a small number of legal plays, considering full houses, runs,
  bombs, wildcard substitutions, and alternative suits/copies of the same rank.
  Prefer combinations that leave a useful hand over simply playing the most cards.
- Preserve bombs and wildcards; spend a bomb to stop a possible opponent finish,
  finish immediately, or leave just one play. Lead low and use stronger responses
  when securing the lead helps the team.
- Use public hand sizes to help a nearly finished partner, let a partner's winning
  play stand, and avoid giving an opponent with one card an easy single.
- Count played cards by physical copy, rank, and suit, including jokers and
  wildcards. Remember revealed tribute/return cards until their owners play them.
  The action history survives trick cleanup and resets on each new deal.
- Treat passes against opponents as uncertain evidence about available responses.
  Ignore passes over a partner, weaken old evidence, and discard an inference
  contradicted by a later play. A pass never proves that a response is impossible.
- Estimate whether a play will retain control or allow an opponent to finish.
  Use exhausted high cards to conserve jokers, secure a route out in two plays,
  and stop a nearly finished opponent from regaining the lead, including by relay.

Hand planning uses memoization and a fixed work budget to keep turns responsive.
It seeks fewer plays but does not guarantee a globally optimal partition for every
27-card hand. The bot receives only its own hand, public counts, and public
actions. Return choices enter that history only when the exchange is revealed.
Individual card-count odds use exact finite-deck arithmetic; combination
probabilities and pass weights are heuristic estimates. There are no
sampled opponent hands or game rollouts; those belong to the future level 4 policy.

Repeatable comparisons use identical seeded deals twice, swapping policies
between teams, across levels 2 through Ace. Level 2 won **151/200 hands (75.5%)**
against the original policy. Level 3 won **518/1,000 hands (51.8%)** against level
2, across seeds 0–499. Its slowest decision in those release-build runs was about
23 ms. The measured gain over level 2 is modest; strength and timing vary by deal
and machine. The earlier policies remain available for reproducible comparisons:

```sh
cargo test --release --locked --no-default-features --test bots compare_against_baseline -- --ignored --nocapture
GUANDAN_BENCHMARK_DEALS=500 cargo test --release --locked --no-default-features --test bots compare_tracking_against_level_two -- --ignored --nocapture
```

The tracking comparison defaults to 100 deals (200 hands). Set
`GUANDAN_BENCHMARK_START` to select another starting seed.

## Verification

```sh
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo fmt --all --check
```

Tests include legacy combination fixtures, rules/tribute/Ace regressions,
30 seeded full-game simulations with card-conservation checks, bot tactical
regressions, small-hand planning checked against an exhaustive solver, complete
games using the new bots through scoring/tribute, card counting and cautious
pass inference, exhaustive checks of small endgames at every level, public-history
reset/reveal behavior, seat ownership and privacy,
and a real TCP test where four native clients finish a hand, exchange tribute,
resume play, and detect host shutdown.

The engine and networking can also be tested without graphical dependencies:

```sh
cargo test --no-default-features --locked
```

For interface previews, `cargo run --locked -- --demo` opens a static native
table. Optional screenshot verification uses:

```sh
cargo build --features screenshot --locked --bin guandan
EFRAME_SCREENSHOT_TO=/tmp/guandan.png ./target/debug/guandan --demo
```

For a compact screenshot, also set `GUANDAN_SCREENSHOT_SIZE=420x340` (available
only when building with the `screenshot` feature).

## Source layout

- `src/cards.rs`, `src/rules.rs`: cards, combination validation, basic move generation.
- `src/bot.rs`: hand planning, partnership/bomb heuristics, and endgame choices.
- `src/bot/knowledge.rs`: public card counts, pass evidence, and response estimates.
- `src/game.rs`: scoring, turns, tribute, rebellion, and Ace progression.
- `src/table.rs`: authoritative seats, bot timers, per-client public/private views.
- `src/net.rs`: background TCP hosting/client connections and versioned protocol.
- `src/app.rs`, `src/main.rs`: the native graphical application.
- `src/app/theme.rs`, `src/app/theme/backdrop.rs`: Diamond Vault styling, serif typography, sapphire facets, and cached glass blur.
- `assets/fonts/`: embedded Cinzel and its license/notice.
- `tests/`: game, migration parity, and real network tests.
- `legacy/`: the previous web application, preserved for reference. It is not
  compiled or launched by Cargo. Its Node tests remain runnable there.

An optional headless host is included:

```sh
cargo run --release --no-default-features --locked --bin guandan-server -- 0.0.0.0:3000
```

Guests still join it using the desktop app. The normal **Host game** button
already runs the same server internally; a separate server process is optional.

## Desktop launcher and navy spade icon

The window icon is embedded in the executable. Wayland also looks up the icon
through an installed desktop entry matching the application's ID. Build and
install the game, launcher, and navy spade icon for your user:

```sh
cargo build --release --locked --bin guandan
sh packaging/install-desktop.sh
```

Relaunch the game after installation. The installer copies the binary to
`~/.local/bin/guandan`, writes an absolute executable path into the launcher,
and installs the launcher and icons beneath `$XDG_DATA_HOME` (normally
`~/.local/share`). No root access or additional Rust dependency is needed.
Rerun it after rebuilding to update the installed executable. To install a
different build, pass its path, e.g. `sh packaging/install-desktop.sh target/debug/guandan`.
Some desktops keep pinned shortcuts cached; unpin and repin Guandan if an old
shortcut retains its previous icon. The original vector and bundled PNG are in
`assets/icons/`.
