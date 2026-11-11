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
      if (name === 'http') return { createServer: () => ({ listen: (_port, _host, cb) => cb() }) };
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
  const host = table.connect('Host');
  const game = table.game();
  game.endHand();
  const friend = table.connect('Friend');
  table.advance(4000);
  assert.equal(game.handOver, false);
  if (game.tributePending()){
    const options = game.tributeReturnOptions(0);
    assert.ok(options.length > 0);
    host.act({ type:'returnTribute', uid:options[0].uid });
  }
  assert.deepEqual(friend.state().handCounts, [27,27,27,27]);
});

function prepareDoubleTribute(game){
  const deck = rules.buildDeck();
  game.hands = [deck.filter(c => c.joker === 'big'), [], [], []];
  for (const card of deck.filter(c => c.joker !== 'big')){
    game.hands.find(h => h.length < 27).push(card);
  }
  game.beginTribute({ finishedOrder:[0,2,1,3] });
  assert.equal(game.tributePending(), true);
}

test('human return choices use the socket seat, stay secret until both choose, and block normal play', () => {
  const table = createTable();
  const players = ['Host','Opponent','Partner','Other'].map(name => table.connect(name));
  const game = table.game();
  prepareDoubleTribute(game);
  players[0].act({ type:'join', name:'Host' });
  assert.equal(players[0].state().tribute.phase, 'return');
  assert.ok(players[0].state().tributeReturnUids.length > 0);
  assert.deepEqual(players[1].state().tributeReturnUids, []);
  const before = game.hands.map(h => h.length);
  players[0].act({ type:'play', uids:[players[0].state().yourHand[0].uid] });
  players[0].act({ type:'pass' });
  assert.deepEqual(game.hands.map(h => h.length), before);
  assert.equal(game.currentTrick, null);
  const selected = players[0].state().tributeReturnUids[0];
  players[1].act({ type:'returnTribute', uid:selected, seat:0 });
  assert.equal(players[1].state().error, 'not-awaiting-your-return');
  players[0].act({ type:'returnTribute', uid:999 });
  assert.equal(players[0].state().error, 'invalid-tribute-return');
  players[0].act({ type:'returnTribute', uid:selected });
  assert.equal(players[0].state().tributeReturnUids.length, 0);
  assert.equal(players[1].state().tribute.exchanges.find(e => e.to === 0).returned, true);
  assert.ok(players[1].state().tribute.exchanges.every(e => e.returnCard === null));
  assert.equal(players[1].state().log.some(line => /returns/.test(line)), false);
  players[0].act({ type:'returnTribute', uid:selected });
  assert.equal(players[0].state().error, 'not-awaiting-your-return');
  players[2].act({ type:'returnTribute', uid:players[2].state().tributeReturnUids[0] });
  assert.equal(game.tributePending(), false);
  assert.deepEqual(players[1].state().handCounts, [27,27,27,27]);
  assert.ok(players[1].state().tribute.exchanges.every(e => e.returnCard));
  assert.equal(new Set(game.hands.flat().map(c => c.uid)).size, 108);
  assert.equal(game.turn, game.tribute.exchanges[0].from);
});

test('bots choose returns automatically and wait for the human return', () => {
  const table = createTable();
  const host = table.connect('Host');
  const game = table.game();
  prepareDoubleTribute(game);
  table.connect('Opponent'); // Reschedule after setting up the next hand.
  table.advance(900);
  assert.equal(game.tribute.exchanges.find(e => e.to === 2).returnCard !== null, true);
  assert.equal(game.tributePending(), true);
  assert.equal(game.turn, null);
  host.act({ type:'returnTribute', uid:host.state().tributeReturnUids[0] });
  assert.equal(game.tributePending(), false);
  assert.deepEqual(host.state().handCounts, [27,27,27,27]);
});

test('a human taking a pending bot return chooses their own card', () => {
  const table = createTable();
  table.connect('Host');
  const game = table.game();
  prepareDoubleTribute(game);
  table.connect('Opponent');
  table.advance(450);
  const partner = table.connect('Partner');
  table.advance(60000);
  assert.equal(game.tribute.exchanges.find(e => e.to === 2).returnCard, null);
  assert.ok(partner.state().tributeReturnUids.length > 0);
  partner.act({ type:'returnTribute', uid:partner.state().tributeReturnUids[0] });
  assert.equal(game.tribute.exchanges.find(e => e.to === 2).returnCard !== null, true);
});

test('disconnecting during a return lets AI finish while another human remains', () => {
  const table = createTable();
  const host = table.connect('Host');
  const game = table.game();
  prepareDoubleTribute(game);
  const opponent = table.connect('Opponent');
  host.close();
  table.advance(1800);
  assert.equal(game.tributePending(), false);
  assert.deepEqual(opponent.state().handCounts, [27,27,27,27]);
});

test('an empty table cancels pending tribute work and a new arrival gets a fresh game', () => {
  const table = createTable();
  const host = table.connect('Host');
  const game = table.game();
  prepareDoubleTribute(game);
  const opponent = table.connect('Opponent');
  host.close();
  opponent.close();
  table.advance(60000);
  assert.ok(game.tribute.exchanges.every(e => !e.returnCard));
  const returning = table.connect('Returning');
  assertFresh(returning.state());
  assert.equal(returning.state().tribute, null);
});

test('level 7 trumps are neither advertised as return options nor accepted over the socket', () => {
  const table = createTable();
  const host = table.connect('Host');
  const game = table.game();
  game.teamLevel[0] = rules.LEVEL_ORDER.indexOf('7');
  game.startHand(0);
  prepareDoubleTribute(game);
  table.connect('Opponent');
  const state = host.state();
  const trumps = state.yourHand.filter(c => c.rank === '7');
  assert.ok(trumps.some(c => c.suit !== 'H'));
  for (const card of trumps){
    assert.equal(state.tributeReturnUids.includes(card.uid), false);
    host.act({ type:'returnTribute', uid:card.uid });
    assert.equal(host.state().error, 'invalid-tribute-return');
  }
  assert.equal(game.tribute.exchanges.find(e => e.to === 0).returnCard, null);
  table.advance(900);
  assert.notEqual(game.tribute.exchanges.find(e => e.to === 2).returnCard.rank, '7');
  host.act({ type:'returnTribute', uid:host.state().tributeReturnUids[0] });
  assert.equal(game.tributePending(), false);
  assert.deepEqual(host.state().handCounts, [27,27,27,27]);
});
