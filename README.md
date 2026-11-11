# Guandan Table (掼蛋)

Guandan for solo, LAN, and Tailscale multiplayer. The game runs on your machine.
You and up to 3 friends on your tailnet can join from a browser, even on different
WiFi networks. Empty seats are played by AI, so it also works solo. Solo and LAN
play work offline; remote tailnet play needs network connectivity and Tailscale.

## Setup (one-time, on Fedora)

You need Node.js. If you don't have it:

```
sudo dnf install nodejs
```

Then, inside this folder:

```
npm install
```

## Run it

In `server.js`, replace `XXX.XXX.XXX.XXX` in this line with **the hosting
computer's Tailscale IPv4 address**, keeping the quotes:

```js
const TAILSCALE_IP = process.env.TAILSCALE_IP || 'XXX.XXX.XXX.XXX';
```

You can find that address by running `tailscale ip -4` on the hosting computer.
Use just the IP address here; the server adds `http://` and the port automatically.
Then start the game:

```
npm start
```

Alternatively, leave `server.js` unchanged and supply the address when starting:

```sh
TAILSCALE_IP=XXX.XXX.XXX.XXX npm start
```

Replace the placeholder in that command with your actual Tailscale IPv4 address.
An environment variable takes precedence over the value in `server.js`.

You'll see something like:

```
Guandan table running.
  On this machine: http://localhost:3000
  On your tailnet: http://XXX.XXX.XXX.XXX:3000

Share the "on your tailnet" link with friends connected to your tailnet.
Keep this computer and Tailscale running while you play.
```

- Open the `localhost` link yourself.
- Make sure the host and your friends' devices are connected to Tailscale and
  that your tailnet access rules allow them to reach the host on TCP port 3000
  (or your configured `PORT`). The host firewall must also allow that connection.
- Send the `on your tailnet` link to friends — they open it in any browser
  (phone or computer) and pick a name to sit down. They can be on different WiFi
  networks. No game installation is needed on their devices.
- Play waits until the first player clicks **Sit Down**, then starts a fresh game.
- Empty seats play themselves (AI), so you can start solo and have friends
  join mid-session — they'll fill open seats.

Seats 0 & 2 are Team A, seats 1 & 3 are Team B. Press Ctrl+C in the terminal
to shut the table down.

## Notes / limitations

- The tailnet link works for devices with Tailscale connectivity and permission
  to access the host. The server uses ordinary HTTP and WebSockets over the
  Tailscale connection; no Tailscale Serve or Funnel setup is needed.
- For solo play, use `http://localhost:3000`. For friends on your LAN without
  Tailscale, use the host's LAN IP with the same port. The server listens on all
  IPv4 interfaces; setting `TAILSCALE_IP` changes the advertised link, not access
  permissions or which interfaces accept connections.
- If someone's browser tab closes while others remain seated, AI takes over
  their seat; rejoining takes whatever seat is open (not necessarily their old one).
- When everyone leaves, play stops. The next player to sit down starts a fresh game.
- Hearts of the current level are wild, but cannot replace either joker.
- Levels stop at Ace. A 1–2 or 1–3 victory while playing your own Ace level wins
  the game. Three consecutive 1–4 wins at your own Ace level reset your team to 2;
  an intervening loss breaks the streak.

## Tribute between hands

After the next deal, a 1–2 victory requires each loser to give their largest
non-wild card: first place receives the higher card and second place the lower
(ties can be assigned either way). After a 1–3 or 1–4 victory, last place gives
their largest non-wild card to first place. Card strength uses the new hand's
level; heart wildcards are exempt, including when all other cards are lower.

Each recipient returns one non-trump 2–9 card to the exact player who gave them
tribute (two return cards total for a double tribute). Humans select a highlighted
card in their hand and click **Return Selected Card**; bots choose automatically.
All suits of the current level rank are trumps above Ace and are excluded from
the normal 2–9 return range. If there is no eligible 2–9, only a lowest non-wild
card may be returned. Returns are revealed together, and normal play waits until
the exchange is complete.

If both big jokers are held by the two losers in a double tribute, or by last
place in a single tribute, they rebel: the exchange is skipped and the previous
first-place player leads. Without rebellion, the largest-card giver leads; if
the tribute cards tie, the giver whose card went to first place leads.

## Files

- `server.js` — the game server (Node + WebSocket)
- `rules.js` — the Guandan rules engine and AI (shared logic, unit-tested)
- `public/index.html` — the browser client
- `test.js`, `test-nuances.js`, `test-server.js`, `simulate.js` — automated correctness checks (`npm test`)
