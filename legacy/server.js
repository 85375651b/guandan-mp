const http = require('http');
const fs = require('fs');
const path = require('path');
const { WebSocketServer } = require('ws');
const { Game, aiChooseLead, aiChooseFollow, cardLabel } = require('./rules.js');

const PORT = process.env.PORT || 3000;
// Replace the placeholder below with this computer's Tailscale IPv4 address.
// Alternatively, set the TAILSCALE_IP environment variable when starting the server.
const TAILSCALE_IP = process.env.TAILSCALE_IP || '100.109.134.112';
const PUBLIC_DIR = path.join(__dirname, 'public');

// ---- Static file server ----
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' };
const server = http.createServer((req, res) => {
  let filePath = req.url.split('?')[0];
  if (filePath === '/') filePath = '/index.html';
  const full = path.join(PUBLIC_DIR, filePath);
  if (!full.startsWith(PUBLIC_DIR)){ res.writeHead(403); res.end('forbidden'); return; }
  fs.readFile(full, (err, data) => {
    if (err){ res.writeHead(404); res.end('not found'); return; }
    const ext = path.extname(full);
    res.writeHead(200, { 'Content-Type': MIME[ext] || 'application/octet-stream' });
    res.end(data);
  });
});

const wss = new WebSocketServer({ server });

// ---- Table state ----
// seats[i] = { ws: WebSocket|null, name: string, isAI: bool }
const seats = [0,1,2,3].map(i => ({ ws: null, name: `Bot ${i}`, isAI: true }));
let game = new Game();
let lastPlays = {}; // seat -> {cards, combo}, cleared when a trick closes
let aiTimer = null;
let handOverTimer = null;

function teamOf(seat){ return seat % 2 === 0 ? 0 : 1; }

function seatOfWs(ws){ return seats.findIndex(s => s.ws === ws); }

function findOpenSeat(){ return seats.findIndex(s => s.isAI); }

function hasHumanPlayers(){
  return seats.some(s => !s.isAI && s.ws && s.ws.readyState === 1);
}

function broadcast(){
  for (const s of seats){
    if (s.ws && s.ws.readyState === 1) sendState(s.ws);
  }
}

function sendState(ws, error = null){
  const yourSeat = seatOfWs(ws);
  const payload = {
    type: 'state',
    yourSeat: yourSeat === -1 ? null : yourSeat,
    seatNames: seats.map(s => s.name),
    seatIsAI: seats.map(s => s.isAI),
    handCounts: game.hands.map(h => h.length),
    handNumber: game.handNumber,
    handPlayCount: game.handPlayCount,
    teamLevel: game.teamLevel,
    bankerTeam: game.bankerTeam,
    levelRank: game.levelRank(),
    turn: game.turn,
    handOver: game.handOver,
    gameOver: game.gameOver,
    winner: game.winner,
    currentTrick: game.currentTrick ? { playerIdx: game.currentTrick.playerIdx, combo: game.currentTrick.combo, cards: game.currentTrick.cards } : null,
    lastPlays,
    lastResult: game.lastResult || null,
    ace14Streak: game.ace14Streak,
    tribute: game.tribute ? {
      kind: game.tribute.kind, phase: game.tribute.phase, rebellion: game.tribute.rebellion,
      leader: game.tribute.leader,
      exchanges: game.tribute.exchanges.map(e => ({
        from: e.from, to: e.to, tributeCard: e.tributeCard, returned: !!e.returnCard,
        returnCard: game.tribute.phase === 'complete' ? e.returnCard : null,
      })),
    } : null,
    tributeReturnUids: yourSeat === -1 ? [] : game.tributeReturnOptions(yourSeat).map(c => c.uid),
    error,
    log: game.log.slice(-60),
  };
  if (yourSeat !== -1) payload.yourHand = game.hands[yourSeat];
  try { ws.send(JSON.stringify(payload)); } catch(e){}
}

function doPlay(seat, cards){
  const res = game.playCards(seat, cards);
  if (res.ok){
    lastPlays[seat] = { cards, combo: game.currentTrick ? game.currentTrick.combo : null };
    if (game.trickJustCleared){ lastPlays = {}; game.trickJustCleared = false; }
  }
  return res;
}

function doPass(seat){
  const res = game.passTurn(seat);
  if (res.ok && game.trickJustCleared){ lastPlays = {}; game.trickJustCleared = false; }
  return res;
}

function scheduleNext(){
  clearTimeout(aiTimer);
  clearTimeout(handOverTimer);
  // An empty table must not play itself while people are opening the join screen.
  if (!hasHumanPlayers()) { broadcast(); return; }
  if (game.gameOver) { broadcast(); return; }
  if (game.handOver){
    broadcast();
    handOverTimer = setTimeout(() => {
      const nextLeader = game.lastResult.finishedOrder[0];
      game.startHand(nextLeader);
      lastPlays = {};
      broadcast();
      scheduleNext();
    }, 4000);
    return;
  }
  broadcast();
  if (game.tributePending()){
    const exchange = game.tribute.exchanges.find(e => !e.returnCard && seats[e.to].isAI);
    if (exchange){
      const scheduledGame = game;
      aiTimer = setTimeout(() => {
        if (game !== scheduledGame || !hasHumanPlayers() || !seats[exchange.to].isAI || !game.tributePending()) return;
        const options = game.tributeReturnOptions(exchange.to);
        options.sort((a,b) => game.hands[exchange.to].filter(c => c.rank === a.rank).length - game.hands[exchange.to].filter(c => c.rank === b.rank).length);
        if (options.length) game.returnTribute(exchange.to, options[0].uid);
        scheduleNext();
      }, 900);
    }
    return;
  }
  if (seats[game.turn].isAI){
    const scheduledGame = game;
    const seat = game.turn;
    aiTimer = setTimeout(() => {
      // Seat ownership may change between scheduling and running this callback.
      if (game !== scheduledGame || !hasHumanPlayers() || !seats[seat].isAI || !game.legalTurn(seat)) return;
      const hand = game.hands[seat];
      const level = game.levelRank();
      if (game.currentTrick && game.currentTrick.playerIdx !== seat){
        const teammateWinning = teamOf(game.currentTrick.playerIdx) === teamOf(seat);
        const choice = aiChooseFollow(hand, level, game.currentTrick.combo, teammateWinning);
        if (choice) doPlay(seat, choice.cards); else doPass(seat);
      } else {
        const choice = aiChooseLead(hand, level);
        if (choice) doPlay(seat, choice.cards);
      }
      scheduleNext();
    }, 900);
  }
}

wss.on('connection', (ws) => {
  ws.on('message', (raw) => {
    let msg;
    try { msg = JSON.parse(raw); } catch(e){ return; }

    if (msg.type === 'join'){
      let seatIdx = seatOfWs(ws);
      if (seatIdx === -1){
        seatIdx = findOpenSeat();
        if (seatIdx === -1){ sendState(ws); return; } // table full, spectator
        if (!hasHumanPlayers()){
          // The first player at an empty table always gets a fresh game.
          game = new Game();
          lastPlays = {};
        }
        seats[seatIdx].ws = ws;
        seats[seatIdx].isAI = false;
        seats[seatIdx].name = (msg.name || 'Guest').slice(0, 20);
        game.log.push(`${seats[seatIdx].name} joins seat ${seatIdx}.`);
        // Cancel a pending bot move for this seat and start any remaining bot turn.
        scheduleNext();
        return;
      }
      broadcast();
      return;
    }

    const seat = seatOfWs(ws);
    if (seat === -1) return; // spectators can't act

    if (msg.type === 'returnTribute'){
      const res = game.returnTribute(seat, msg.uid);
      if (res.ok) scheduleNext(); else sendState(ws, res.reason);
    } else if (msg.type === 'play'){
      if (!game.legalTurn(seat)) return;
      const uidSet = new Set(msg.uids || []);
      const cards = game.hands[seat].filter(c => uidSet.has(c.uid));
      if (cards.length !== uidSet.size) return;
      const res = doPlay(seat, cards);
      if (res.ok) scheduleNext(); else sendState(ws);
    } else if (msg.type === 'pass'){
      if (!game.legalTurn(seat) || !game.currentTrick) return;
      const res = doPass(seat);
      if (res.ok) scheduleNext(); else sendState(ws);
    } else if (msg.type === 'newgame'){
      if (!game.gameOver) return;
      game = new Game();
      lastPlays = {};
      game.log.push('New game started.');
      scheduleNext();
    }
  });

  ws.on('close', () => {
    const seatIdx = seatOfWs(ws);
    if (seatIdx !== -1){
      game.log.push(`${seats[seatIdx].name} disconnected — AI takes over seat ${seatIdx}.`);
      seats[seatIdx].ws = null;
      seats[seatIdx].isAI = true;
      broadcast();
      scheduleNext();
    }
  });

  sendState(ws);
});

// Listen on every IPv4 interface, including Tailscale and localhost.
server.listen(PORT, '0.0.0.0', () => {
  console.log(`\nGuandan table running.`);
  console.log(`  On this machine: http://localhost:${PORT}`);
  console.log(`  On your tailnet: http://${TAILSCALE_IP}:${PORT}`);
  if (TAILSCALE_IP === 'XXX.XXX.XXX.XXX'){
    console.log('  Replace XXX.XXX.XXX.XXX in server.js (TAILSCALE_IP), or set the TAILSCALE_IP environment variable.');
  }
  console.log(`\nShare the "on your tailnet" link with friends connected to your tailnet.\nKeep this computer and Tailscale running while you play.\nPress Ctrl+C to stop.\n`);
  scheduleNext();
});
