# Guandan Table (掼蛋)

Offline/LAN multiplayer Guandan. Runs entirely on your machine — no internet
required. You and up to 3 friends on the same WiFi/network can join from a
browser; any empty seat is played by AI, so it also works solo.

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

```
npm start
```

You'll see something like:

```
Guandan table running.
  On this machine: http://localhost:3000
  On your network:  http://192.168.1.42:3000

Share the "on your network" link with friends on the same WiFi/LAN.
```

- Open the `localhost` link yourself.
- Send the `on your network` link to friends on the same WiFi — they open it
  in any browser (phone or computer) and pick a name to sit down.
- Empty seats play themselves (AI), so you can start solo and have friends
  join mid-session — they'll fill open seats.

Seats 0 & 2 are Team A, seats 1 & 3 are Team B. Press Ctrl+C in the terminal
to shut the table down.

## Notes / limitations

- This is a LAN game: everyone needs to be reachable on the same local
  network (same WiFi/router). It won't work over the open internet without
  additional setup (port forwarding, a cloud host, etc.) — happy to help
  with that separately if you want to open it up to remote friends.
- If someone's browser tab closes, AI takes over their seat immediately;
  rejoining takes whatever seat is open (not necessarily their old one).
- Rules are full Guandan minus the tribute-card exchange phase between
  hands (see the in-app "Rules" button for the full list of what's covered).

## Files

- `server.js` — the game server (Node + WebSocket)
- `rules.js` — the Guandan rules engine and AI (shared logic, unit-tested)
- `public/index.html` — the browser client
- `test.js`, `simulate.js` — automated correctness checks (`npm test`)
