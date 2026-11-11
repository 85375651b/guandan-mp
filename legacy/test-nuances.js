const assert = require('node:assert/strict');
const { test } = require('node:test');
const { Game, buildDeck, classifyCombo, isWild, singleValue, LEVEL_ORDER } = require('./rules.js');

function cards(...specs){
  const deck = buildDeck();
  return specs.map(spec => {
    const i = deck.findIndex(c => spec === c.joker || spec === c.rank + c.suit);
    assert.ok(i >= 0, `Available card: ${spec}`);
    return deck.splice(i, 1)[0];
  });
}

function gameAt(level = '5', team = 0){
  const g = new Game();
  g.teamLevel[team] = LEVEL_ORDER.indexOf(level);
  g.bankerTeam = team;
  g.startHand(0);
  return g;
}

function finish(g, order){
  g.finishedOrder = order.slice();
  g.activeMask = [0,1,2,3].map(s => !order.includes(s));
  g.endHand();
}

function exchange(g, order){
  g.beginTribute({ finishedOrder: order });
  return g.tribute;
}

function resolve(g){
  while (g.tributePending()){
    const e = g.tribute.exchanges.find(e => !e.returnCard);
    assert.equal(g.returnTribute(e.to, g.tributeReturnOptions(e.to)[0].uid).ok, true);
  }
}

for (const level of LEVEL_ORDER){
  test(`wildcards never replace jokers at level ${level}`, () => {
    const wild = level + 'H';
    for (const specs of [['small', wild], ['big', wild], ['small','big',wild,wild], ['small','small','big',wild]]){
      assert.equal(classifyCombo(cards(...specs), level), null);
    }
    assert.equal(classifyCombo(cards('small','small'), level).type, 'PAIR');
    assert.equal(classifyCombo(cards('big','big'), level).type, 'PAIR');
    assert.equal(classifyCombo(cards('small','small','big','big'), level).isJokerBomb, true);
  });
}

test('reaching or overshooting Ace does not finish the game', () => {
  const g = gameAt('K');
  finish(g, [0,2]);
  assert.equal(g.teamLevel[0], 12);
  assert.equal(g.gameOver, false);
  assert.equal(g.lastResult.levelAfter, 'A');
  assert.equal(g.levelRank(), 'K', 'the completed hand keeps its original level');
  g.startHand(0);
  assert.equal(g.levelRank(), 'A');
});

for (const team of [0,1]){
  const first = team, partner = team + 2, enemy = 1 - team, otherEnemy = enemy + 2;
  for (const order of [[first,partner], [first,enemy,partner]]){
    test(`team ${team} wins the game with ${order.length === 2 ? '1–2' : '1–3'} on its own Ace`, () => {
      const g = gameAt('A', team);
      finish(g, order);
      assert.equal(g.gameOver, true);
      assert.equal(g.winner, team);
      assert.equal(g.lastResult.passedAce, true);
    });
  }
  test(`team ${team} drops to 2 after three consecutive 1–4 Ace wins`, () => {
    const g = gameAt('A', team);
    for (let n = 1; n <= 3; n++){
      finish(g, [first,enemy,otherEnemy]);
      assert.equal(g.gameOver, false);
      assert.equal(g.lastResult.aceFailureCount, n);
      assert.equal(g.teamLevel[team], n < 3 ? 12 : 0);
      if (n < 3) { g.startHand(first); resolve(g); }
    }
    assert.equal(g.lastResult.resetToTwo, true);
    assert.equal(g.ace14Streak[team], 0);
    assert.equal(g.levelRank(), 'A');
    g.startHand(first);
    assert.equal(g.levelRank(), '2');
  });
}

test('an intervening loss breaks the consecutive Ace streak', () => {
  const g = gameAt('A');
  finish(g, [0,1,3]);
  assert.equal(g.ace14Streak[0], 1);
  g.startHand(0); resolve(g);
  finish(g, [1,0,3]);
  assert.equal(g.ace14Streak[0], 0);
  assert.equal(g.teamLevel[0], 12);
});

test('winning while the other team is playing a lower level does not pass your Ace', () => {
  const g = gameAt('5', 1);
  g.teamLevel[0] = 12;
  finish(g, [0,2]);
  assert.equal(g.gameOver, false);
  assert.equal(g.ace14Streak[0], 0);
  assert.equal(g.lastResult.passedAce, false);
});

test('single tribute gives the largest non-wild card and returns to the exact giver', () => {
  const g = gameAt();
  g.hands[3] = cards('5H','AS','KD');
  g.hands[0] = cards('2S','9D','10C','big');
  const t = exchange(g, [0,1,2,3]);
  assert.deepEqual(t.exchanges.map(e => [e.from,e.to,e.tributeCard.rank]), [[3,0,'A']]);
  assert.ok(g.hands[3].some(c => isWild(c,'5')));
  assert.deepEqual(g.tributeReturnOptions(0).map(c => c.rank), ['2','9']);
  assert.equal(g.turn, null);
  assert.equal(g.legalTurn(0), false);
  assert.equal(g.playCards(0,[g.hands[0][0]]).reason, 'tribute-pending');
  assert.equal(g.passTurn(0).reason, 'tribute-pending');
  const low = g.hands[0].find(c => c.rank === '9');
  assert.equal(g.returnTribute(0, low.uid).ok, true);
  assert.ok(g.hands[3].some(c => c.uid === low.uid));
  assert.equal(g.turn, 3);
  assert.equal(g.legalTurn(3), true);
});

test('1–4 finishes use a single tribute from the winner’s partner', () => {
  const g = gameAt();
  g.hands[2] = cards('small','AS','5H');
  g.hands[0] = cards('3S','9D');
  const t = exchange(g, [0,1,3,2]);
  assert.equal(t.kind, 'single');
  assert.equal(t.exchanges[0].from, 2);
  assert.equal(t.exchanges[0].to, 0);
  assert.equal(t.exchanges[0].tributeCard.joker, 'small');
  resolve(g);
  assert.equal(g.turn, 2);
});

test('double tribute routes the higher offer to first and collects both returns before transferring', () => {
  const g = gameAt();
  g.hands[1] = cards('KS','9S');
  g.hands[3] = cards('AS','8S');
  g.hands[0] = cards('2S','3S');
  g.hands[2] = cards('4S','6S');
  const t = exchange(g, [0,2,1,3]);
  assert.deepEqual(t.exchanges.map(e => [e.from,e.to,e.tributeCard.rank]), [[3,0,'A'],[1,2,'K']]);
  const firstReturn = g.tributeReturnOptions(0)[0];
  const secondReturn = g.tributeReturnOptions(2)[0];
  const before = g.hands.map(h => h.length);
  assert.equal(g.returnTribute(0, firstReturn.uid).ok, true);
  assert.equal(g.tributePending(), true);
  assert.deepEqual(g.hands.map(h => h.length), before);
  assert.ok(g.hands[0].some(c => c.uid === firstReturn.uid));
  assert.equal(g.returnTribute(2, secondReturn.uid).ok, true);
  assert.equal(g.tributePending(), false);
  assert.ok(g.hands[3].some(c => c.uid === firstReturn.uid));
  assert.ok(g.hands[1].some(c => c.uid === secondReturn.uid));
  assert.equal(g.turn, 3);
});

test('equal tribute offers still have unique reciprocal pairings and a deterministic leader', () => {
  const g = gameAt();
  g.hands[1] = cards('AS','9S');
  g.hands[3] = cards('AD','8S');
  g.hands[0] = cards('2S');
  g.hands[2] = cards('3S');
  const t = exchange(g, [0,2,1,3]);
  assert.deepEqual(t.exchanges.map(e => e.from).sort(), [1,3]);
  assert.deepEqual(t.exchanges.map(e => e.to).sort(), [0,2]);
  resolve(g);
  assert.equal(g.turn, t.exchanges.find(e => e.to === 0).from);
});

test('tribute ranking uses the new level, including off-suit level cards', () => {
  const g = gameAt('8');
  g.hands[3] = cards('8H','AS','8C','big');
  g.hands[0] = cards('2S');
  exchange(g, [0,1,2,3]);
  assert.equal(g.tribute.exchanges[0].tributeCard.joker, 'big');
  const h = gameAt('8');
  h.hands[3] = cards('8H','AS','8C');
  h.hands[0] = cards('2S');
  exchange(h, [0,1,2,3]);
  assert.equal(h.tribute.exchanges[0].tributeCard.rank, '8');
  assert.equal(h.tribute.exchanges[0].tributeCard.suit, 'C');
});

for (const split of [true,false]){
  test(`double tribute rebellion with both big jokers ${split ? 'split between donors' : 'in one donor’s hand'}`, () => {
    const g = gameAt();
    g.hands[1] = cards(...(split ? ['big','3S'] : ['big','big','3S']));
    g.hands[3] = cards(...(split ? ['big','4S'] : ['4S']));
    const before = g.hands.map(h => h.map(c => c.uid));
    const t = exchange(g, [0,2,1,3]);
    assert.equal(t.rebellion, true);
    assert.deepEqual(t.exchanges, []);
    assert.deepEqual(g.hands.map(h => h.map(c => c.uid)), before);
    assert.equal(g.turn, 0);
  });
}

test('single tribute rebellion requires both big jokers in last place, not just their team', () => {
  const g = gameAt();
  g.hands[3] = cards('big','big','AS');
  exchange(g, [0,1,2,3]);
  assert.equal(g.tribute.rebellion, true);
  assert.equal(g.turn, 0);
  const h = gameAt();
  const jokers = cards('big','big');
  h.hands[1] = [jokers[0]];
  h.hands[3] = [jokers[1]];
  h.hands[0] = cards('2S');
  exchange(h, [0,1,2,3]);
  assert.equal(h.tribute.rebellion, false);
  assert.equal(h.tributePending(), true);
});

test('returns reject wildcards, high cards, foreign cards, invalid seats, and duplicate submissions', () => {
  const g = gameAt();
  g.hands[1] = cards('AS');
  g.hands[3] = cards('KS');
  g.hands[0] = cards('5H','3S','QC');
  g.hands[2] = cards('4S');
  exchange(g, [0,2,1,3]);
  for (const card of g.hands[0].filter(c => c.rank !== '3')) assert.equal(g.returnTribute(0,card.uid).ok, false);
  assert.equal(g.returnTribute(0,999).ok, false);
  assert.equal(g.returnTribute(1,g.hands[0][0].uid).ok, false);
  const valid = g.tributeReturnOptions(0)[0];
  assert.equal(g.returnTribute(0, valid.uid).ok, true);
  assert.equal(g.returnTribute(0, valid.uid).ok, false);
});

test('without a natural 2–9, return only the lowest non-wild rank, preserving wildcards', () => {
  const g = gameAt();
  g.hands[0] = cards('5H','10S','10D','QC','big');
  g.hands[3] = cards('AS');
  exchange(g, [0,1,2,3]);
  assert.deepEqual(g.tributeReturnOptions(0).map(c => c.rank), ['10','10']);
  const queen = g.hands[0].find(c => c.rank === 'Q');
  assert.equal(g.returnTribute(0, queen.uid).ok, false);
  resolve(g);
  assert.ok(g.hands[3].some(c => c.rank === '10'));
  assert.ok(g.hands[0].some(c => isWild(c,'5')));
});

for (const level of ['2','3','4','5','6','7','8','9']){
  test(`all suits of level ${level} are excluded from ordinary 2–9 returns`, () => {
    const g = gameAt(level);
    const low = level === '2' ? '3S' : '2S';
    const high = level === '9' ? '8D' : '9D';
    g.hands[0] = cards(...['S','H','D','C'].map(suit => level + suit), low, high, '10D');
    g.hands[3] = cards('AS');
    exchange(g, [0,1,2,3]);
    assert.deepEqual(g.tributeReturnOptions(0).map(c => c.rank + c.suit), [low,high]);
    const before = g.hands.map(h => h.map(c => c.uid));
    for (const card of g.hands[0].filter(c => c.rank === level)){
      assert.equal(g.returnTribute(0, card.uid).reason, 'invalid-tribute-return');
    }
    assert.deepEqual(g.hands.map(h => h.map(c => c.uid)), before);
    assert.equal(g.tributePending(), true);
  });
}

test('trumps do not count as low cards when applying the lowest non-wild fallback', () => {
  const g = gameAt('7');
  g.hands[0] = cards('7H','7S','7D','7C','10S','JD','QC');
  g.hands[3] = cards('AS');
  exchange(g, [0,1,2,3]);
  assert.deepEqual(g.tributeReturnOptions(0).map(c => c.rank + c.suit), ['10S']);
  assert.equal(g.returnTribute(0, g.hands[0].find(c => c.rank === '7').uid).ok, false);
  resolve(g);
  assert.ok(g.hands[3].some(c => c.rank === '10'));
  assert.equal(g.hands[0].filter(c => c.rank === '7').length, 4);
});

test('a complete exchange preserves all 108 unique cards and 27 cards per hand', () => {
  for (const order of [[0,2,1,3],[1,3,0,2],[0,1,2,3],[0,1,3,2]]){
    const g = gameAt();
    const before = g.hands.flat().map(c => c.uid).sort((a,b) => a-b);
    exchange(g, order);
    resolve(g);
    assert.deepEqual(g.hands.map(h => h.length), [27,27,27,27]);
    assert.deepEqual(g.hands.flat().map(c => c.uid).sort((a,b) => a-b), before);
    for (const hand of g.hands) assert.deepEqual(hand.map(c => singleValue(c,g.levelRank())), hand.map(c => singleValue(c,g.levelRank())).sort((a,b) => a-b));
  }
});
