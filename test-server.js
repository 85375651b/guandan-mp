const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');
const rules = require('./rules.js');

// Exercise the real server handlers with an in-memory transport and virtual clock.
function createTable(leader = 0){
  let now = 0, nextTimer = 0, wss;
  const timers = new Map();
  class WebSocketServer extends EventEmitter {
    constructor(){ super(); wss = this; }
  }
  class Game extends rules.Game {
    constructor(){ super(); this.turn = this.leader = leader; }
  }
  const context = vm.createContext({
    __dirname, process: { env: {} }, console: { log(){} },
    require(name){
      if (name === 'http') return { createServer: () => ({ listen: (_port, cb) => cb() }) };
      if (name === 'os') return { networkInterfaces: () => ({}) };
      if (name === 'ws') return { WebSocketServer };
      if (name === './rules.js') return { ...rules, Game };
      return require(name);
    },
    setTimeout(fn, delay){
      timers.set(++nextTimer, { fn, due: now + delay });
      return nextTimer;
    },
    clearTimeout(id){ timers.delete(id); },
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, 'server.js'), 'utf8'), context);
  return {
    game: () => vm.runInContext('game', context),
    advance(ms){
      const target = now + ms;
      let steps = 0;
      while (true){
        const entry = [...timers].sort((a,b) => a[1].due - b[1].due)[0];
        if (!entry || entry[1].due > target) break;
        assert.ok(++steps < 10000, 'server timers must settle');
        timers.delete(entry[0]);
        now = entry[1].due;
        entry[1].fn();
      }
      now = target;
    },
    connect(name){
      const ws = new EventEmitter();
      ws.readyState = 1;
      ws.messages = [];
      ws.send = raw => ws.messages.push(JSON.parse(raw));
      ws.state = () => ws.messages.at(-1);
      ws.act = msg => ws.emit('message', JSON.stringify(msg));
      ws.close = () => { ws.readyState = 3; ws.emit('close'); };
      wss.emit('connection', ws);
      if (name) ws.act({ type: 'join', name });
      return ws;
    },
  };
}

function assertFresh(state){
  assert.deepEqual(state.handCounts, [27,27,27,27]);
  assert.deepEqual(state.teamLevel, [0,0]);
  assert.equal(state.currentTrick, null);
  assert.deepEqual(state.lastPlays, {});
  assert.equal(state.handOver, false);
  assert.equal(state.gameOver, false);
  assert.equal(state.log.some(line => /^P\d (plays|passes)/.test(line)), false);
}

test('an unattended table stays fresh, even after a browser connects without sitting down', () => {
  const table = createTable(1);
  table.advance(60000);
  const browser = table.connect();
  table.advance(60000);
  assert.equal(browser.state().yourSeat, null);
  assertFresh(browser.state());
  browser.act({ type: 'join', name: 'First player' });
  assertFresh(browser.state());
});

test('a human opening leader keeps all 27 cards until they play', () => {
  const table = createTable();
  const player = table.connect('Human');
  table.advance(60000);
  assertFresh(player.state());
  const uid = player.state().yourHand[0].uid;
  player.act({ type: 'play', uids: [uid] });
  assert.equal(player.state().yourHand.length, 26);
  assert.equal(player.state().currentTrick.playerIdx, 0);
  table.advance(900);
  assert.ok(player.state().log.some(line => /^P1 (plays|passes)/.test(line)));
});

test('a bot opening leader starts only after the first human sits down', () => {
  const table = createTable(1);
  table.advance(60000);
  const player = table.connect('Human');
  assertFresh(player.state());
  table.advance(899);
  assertFresh(player.state());
  table.advance(1);
  assert.ok(player.state().handCounts[1] < 27);
  assert.equal(player.state().yourHand.length, 27);
});

test('taking a bot seat with a pending turn leaves that turn to the human', () => {
  const table = createTable(1);
  table.connect('Host');
  table.advance(450);
  const friend = table.connect('Friend');
  assert.equal(friend.state().yourSeat, 1);
  table.advance(60000);
  assertFresh(friend.state());
  assert.equal(friend.state().turn, 1);
  friend.act({ type: 'play', uids: [friend.state().yourHand[0].uid] });
  assert.equal(friend.state().yourHand.length, 26);
});

test('friends joining an occupied table preserve the current hand and team levels', () => {
  const table = createTable(1);
  const host = table.connect('Host');
  table.advance(900);
  const before = host.state();
  const game = table.game();
  game.teamLevel[0] = 3;
  const friend = table.connect('Friend');
  assert.equal(table.game(), game);
  assert.deepEqual(friend.state().handCounts, before.handCounts);
  assert.deepEqual(friend.state().currentTrick, before.currentTrick);
  assert.deepEqual(friend.state().teamLevel, [3,0]);
  assert.deepEqual(host.state().yourHand, before.yourHand);
});

test('AI takes over a disconnected seat while another human remains', () => {
  const table = createTable();
  const host = table.connect('Host');
  const friend = table.connect('Friend');
  host.close();
  table.advance(900);
  assert.ok(friend.state().handCounts[0] < 27);
  assert.equal(friend.state().seatIsAI[0], true);
  assert.equal(friend.state().turn, 1);
});

test('the last human leaving stops pending bot turns and the next arrival starts fresh', () => {
  const table = createTable(1);
  const host = table.connect('Host');
  table.advance(900);
  const oldGame = table.game();
  oldGame.teamLevel[0] = 3;
  host.close();
  const counts = oldGame.hands.map(h => h.length);
  table.advance(60000);
  assert.deepEqual(oldGame.hands.map(h => h.length), counts);
  const returning = table.connect('Returning');
  assert.notEqual(table.game(), oldGame);
  assertFresh(returning.state());
});

test('the last human leaving cancels an automatic next-hand deal', () => {
  const table = createTable();
  const host = table.connect('Host');
  const oldGame = table.game();
  oldGame.endHand();
  // Joining another seat makes the scheduler observe the finished hand.
  const friend = table.connect('Friend');
  host.close();
  friend.close();
  table.advance(60000);
  assert.equal(oldGame.handOver, true);
  assertFresh(table.connect('Returning').state());
});

test('an automatic next hand still starts while a human remains seated', () => {
  const table = createTable();
  table.connect('Host');
  const game = table.game();
  game.endHand();
  const friend = table.connect('Friend');
  table.advance(4000);
  assert.equal(game.handOver, false);
  assert.deepEqual(friend.state().handCounts, [27,27,27,27]);
});
