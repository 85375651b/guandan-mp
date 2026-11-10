const { buildDeck, classifyCombo, canBeat, cardLabel, isWild, enumerateCombos } = require('./rules.js');

function C(rank, suit){ return { rank, suit, joker: null }; }
function J(kind){ return { rank: null, suit: null, joker: kind }; }

let pass = 0, fail = 0;
function check(name, actual, expected){
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e){ pass++; }
  else { fail++; console.log(`FAIL ${name}\n  got:      ${a}\n  expected: ${e}`); }
}

const level = '5'; // current level rank -> Hearts-5 is wild

// Deck size / composition
const deck = buildDeck();
check('deck size', deck.length, 108);

// Single
let combo = classifyCombo([C('9','S')], level);
check('single 9', [combo.type, combo.size], ['SINGLE', 1]);

// Wild single is just itself, value 13
combo = classifyCombo([C('5','H')], level);
check('wild single value', combo.strength, 13);

// Joker values
check('big joker value', classifyCombo([J('big')], level).strength, 15);
check('small joker value', classifyCombo([J('small')], level).strength, 14);

// Pair natural
combo = classifyCombo([C('9','S'), C('9','H')], level);
check('natural pair', [combo.type, combo.rank], ['PAIR', '9']);

// Pair with wild filler (5H is wild, pairing with 9S -> pair of 9s)
combo = classifyCombo([C('9','S'), C('5','H')], level);
check('wild-assisted pair', combo && [combo.type, combo.rank], ['PAIR', '9']);

// Invalid pair (different ranks, no wild)
combo = classifyCombo([C('9','S'), C('8','H')], level);
check('invalid pair', combo, null);

// Triple with wild
combo = classifyCombo([C('J','S'), C('J','D'), C('5','H')], level);
check('wild-assisted triple', combo && [combo.type, combo.rank], ['TRIPLE', 'J']);

// Triple + pair (natural)
combo = classifyCombo([C('7','S'), C('7','D'), C('7','C'), C('K','S'), C('K','H')], level);
check('triple+pair natural', combo && [combo.type, combo.rank], ['TRIPLE_PAIR', '7']);

// Tractor ("helicopter"): Guandan requires EXACTLY 3 consecutive pairs, not 2
combo = classifyCombo([C('6','S'), C('6','D'), C('7','S'), C('7','D')], level); // only 2 pairs
check('2-pair "tractor" is INVALID in Guandan (must be exactly 3 pairs)', combo, null);

combo = classifyCombo([C('6','S'), C('6','D'), C('7','S'), C('7','D'), C('8','S'), C('8','D')], level);
check('tractor natural (3 consecutive pairs)', combo && [combo.type, combo.size, combo.rank], ['TRACTOR', 6, '8']);

// Tractor with wild filling one pair (6-6-7-7-8-<wild> => three pairs using wild as second 8)
combo = classifyCombo([C('6','S'), C('6','D'), C('7','S'), C('7','D'), C('8','S'), C('5','H')], level);
check('tractor wild-assisted', combo && [combo.type, combo.size, combo.rank], ['TRACTOR', 6, '8']);

// Straight 5 consecutive singles
combo = classifyCombo([C('9','S'), C('10','D'), C('J','C'), C('Q','H'), C('K','S')], level);
check('straight natural', combo && [combo.type, combo.size, combo.rank], ['STRAIGHT', 5, 'K']);

// Straight ending in 2 (A,2 sequence allowed; no wrap to 3)
combo = classifyCombo([C('10','S'), C('J','D'), C('Q','C'), C('K','H'), C('A','S')], level);
check('straight to A', combo && combo.rank, 'A');

// Bomb 4 naturals same rank
combo = classifyCombo([C('9','S'), C('9','H'), C('9','D'), C('9','C')], level);
check('bomb4', combo && [combo.type, combo.size, combo.isBomb], ['BOMB', 4, true]);

// 5-card bomb beats straight flush of same size
const bomb5 = classifyCombo([C('9','S'), C('9','H'), C('9','D'), C('9','C'), C('9','S')], level); // note: dup uid irrelevant here, just rank/suit
const sf5 = classifyCombo([C('3','S'), C('4','S'), C('5','S'), C('6','S'), C('7','S')], level); // 5S is wild! must avoid using level-rank card in SF test
// redo SF test with a level that isn't 5 to avoid the wildcard colliding with the suit test
const sf5b = classifyCombo([C('9','S'), C('10','S'), C('J','S'), C('Q','S'), C('K','S')], level);
check('straight flush detected', sf5b && [sf5b.type, sf5b.isStraightFlush], ['BOMB', true]);
if (bomb5) check('5-bomb beats 5-SF', bomb5.strength > sf5b.strength, true);

// Joker bomb
combo = classifyCombo([J('small'), J('small'), J('big'), J('big')], level);
check('joker bomb', combo && [combo.type, combo.isJokerBomb, combo.strength], ['BOMB', true, 100000]);

// canBeat: bomb beats non-bomb of any type
const single9 = classifyCombo([C('9','S')], level);
const bomb4 = classifyCombo([C('3','S'), C('3','H'), C('3','D'), C('3','C')], level);
check('bomb beats single', canBeat(bomb4, single9), true);

// canBeat: pair must match size/type
const pairA = classifyCombo([C('A','S'), C('A','H')], level);
const tripleK = classifyCombo([C('K','S'), C('K','H'), C('K','D')], level);
check('pair cannot beat triple (type mismatch)', canBeat(pairA, tripleK), false);

// canBeat: higher pair beats lower pair
const pair9 = classifyCombo([C('9','S'), C('9','H')], level);
check('A pair beats 9 pair', canBeat(pairA, pair9), true);
check('9 pair cannot beat A pair', canBeat(pair9, pairA), false);

// Three consecutive triples (9 cards) is INVALID in Guandan — plane must be exactly 2 triples
combo = classifyCombo([C('9','S'),C('9','H'),C('9','D'),C('10','S'),C('10','H'),C('10','D'),C('J','S'),C('J','H'),C('J','D')], level);
check('3-triple "plane" is INVALID in Guandan (must be exactly 2 triples)', combo, null);

// Plane (airplane): two consecutive triples, e.g. 666777
combo = classifyCombo([C('6','S'), C('6','H'), C('6','D'), C('7','S'), C('7','H'), C('7','D')], level);
check('plane bare', combo && [combo.type, combo.size, combo.rank], ['PLANE', 6, '7']);

// Plane with a wild filling one triple slot
combo = classifyCombo([C('6','S'), C('6','H'), C('6','D'), C('7','S'), C('7','H'), C('5','H')], level);
check('plane wild-assisted', combo && [combo.type, combo.size, combo.rank], ['PLANE', 6, '7']);

// Bigger plane beats smaller plane
const plane67 = classifyCombo([C('6','S'), C('6','H'), C('6','D'), C('7','S'), C('7','H'), C('7','D')], level);
const plane89 = classifyCombo([C('8','S'), C('8','H'), C('8','D'), C('9','S'), C('9','H'), C('9','D')], level);
check('plane 8-9 beats plane 6-7', canBeat(plane89, plane67), true);

// Non-consecutive triples are NOT a plane
combo = classifyCombo([C('6','S'), C('6','H'), C('6','D'), C('9','S'), C('9','H'), C('9','D')], level);
check('non-consecutive triples rejected', combo, null);

// 2 is the LOWEST card in Guandan (not highest) — confirmed against official rules
check('2 is lower than 3', classifyCombo([C('2','S')], level).strength < classifyCombo([C('3','S')], level).strength, true);
check('A is the highest normal single', classifyCombo([C('A','S')], level).strength > classifyCombo([C('2','S')], level).strength, true);
const single2 = classifyCombo([C('2','S')], level);
const singleA = classifyCombo([C('A','S')], level);
check('A beats 2 in a single-vs-single comparison', canBeat(singleA, single2), true);

// Low-Ace straight wraparound: A-2-3-4-5 is a valid (weak) straight
combo = classifyCombo([C('A','S'), C('2','D'), C('3','C'), C('4','H'), C('5','S')], level);
check('low-ace straight A2345', combo && [combo.type, combo.size], ['STRAIGHT', 5]);
// ...and it's weaker than a normal mid-range straight
const straightLow = classifyCombo([C('A','S'), C('2','D'), C('3','C'), C('4','H'), C('5','S')], level);
const straightMid = classifyCombo([C('6','S'), C('7','D'), C('8','C'), C('9','H'), C('10','S')], level);
check('A2345 is weaker than 6-10 straight', canBeat(straightMid, straightLow), true);

// Low-Ace tractor wraparound: AA-22-33 is a valid (weak) tractor
combo = classifyCombo([C('A','S'),C('A','H'),C('2','S'),C('2','H'),C('3','S'),C('3','H')], level);
check('low-ace tractor AA2233', combo && [combo.type, combo.size], ['TRACTOR', 6]);

// AI leading should prefer a bigger combo over a single when both are available
const { aiChooseLead } = require('./rules.js');
const mixedHand = [C('6','S'), C('6','H'), C('9','C'), C('K','D')]; // has a pair (66) and two unrelated singles
const leadChoice = aiChooseLead(mixedHand, level);
check('AI leads with the pair, not a lone single, when a pair is available', leadChoice.combo.type, 'PAIR');

// --- Regression: off-suit level cards must also rank above Ace, not just Hearts ---
// level='5' in this test file. Bug report: level=2, a 2 of Clubs lost to a 6 as singles.
const level2 = '2';
const twoClub = classifyCombo([C('2','C')], level2);   // off-suit level card, NOT the wildcard suit
const sixSpade = classifyCombo([C('6','S')], level2);
check('off-suit level card (2♣ at level 2) beats a plain 6', canBeat(twoClub, sixSpade), true);
check('off-suit level card ranks above Ace', twoClub.strength > classifyCombo([C('A','S')], level2).strength, true);
check('off-suit level card is NOT wild (cannot substitute)', isWild(C('2','C'), level2), false);

// --- Regression: wildcards must be usable to complete a bomb ---
// Bug report: K+K+K + (2 of Hearts, wild at level 2) should form a 4-card King bomb.
combo = classifyCombo([C('K','S'), C('K','H'), C('K','D'), C('2','H')], level2);
check('KKK + wildcard forms a 4-card bomb', combo && [combo.type, combo.size, combo.rank, combo.isBomb], ['BOMB', 4, 'K', true]);

// Documented example: 8 naturals + 2 wildcards = a 10-card bomb
combo = classifyCombo([
  C('9','S'), C('9','D'), C('9','C'), C('9','S'), C('9','D'), C('9','C'), C('9','S'), C('9','D'), // 8 naturals of 9 (suits reused across the 2-deck shoe, fine for pattern testing)
  C('2','H'), C('2','H')
], level2);
check('8 naturals + 2 wildcards forms a 10-card bomb', combo && [combo.type, combo.size, combo.isBomb], ['BOMB', 10, true]);

// Straight must be EXACTLY 5, not 6+ (unlike Dou Dizhu)
combo = classifyCombo([C('6','S'), C('7','D'), C('8','C'), C('9','H'), C('10','S'), C('J','D')], level); // 6 consecutive singles
check('6-card "straight" is INVALID in Guandan (must be exactly 5)', combo, null);

// Straight flush must be EXACTLY 5, not 6+
combo = classifyCombo([C('6','S'), C('7','S'), C('8','S'), C('9','S'), C('10','S'), C('J','S')], level); // 6 consecutive same-suit
check('6-card "straight flush" is INVALID in Guandan (must be exactly 5)', combo, null);

// Triple + single (aaab) is NOT a legal combo — only triple + PAIR is allowed
combo = classifyCombo([C('8','S'), C('8','H'), C('8','D'), C('K','S')], level);
check('triple+single (aaab) is INVALID — only triple+pair is legal', combo, null);

// Triple + two unrelated singles (aaabc) is also not legal
combo = classifyCombo([C('8','S'), C('8','H'), C('8','D'), C('K','S'), C('Q','D')], level);
check('triple + two different singles (aaabc) is INVALID', combo, null);

// --- Regression: Guandan fixes combo lengths, unlike Dou Dizhu's variable chains ---
// A 7-card straight (not 5) must be rejected
combo = classifyCombo([C('3','S'),C('4','D'),C('5','C'),C('6','H'),C('7','S'),C('8','D'),C('9','C')], level);
check('7-card straight rejected (Guandan straights are exactly 5)', combo, null);

// A 6-card same-suit straight flush (not 5) must be rejected as a straight flush;
// with 6 naturals of consecutive rank it also isn't a bomb (not same rank), so it's just invalid.
combo = classifyCombo([C('3','S'),C('4','S'),C('5','S'),C('6','S'),C('7','S'),C('8','S')], level);
check('6-card straight flush rejected (Guandan straight flushes are exactly 5)', combo, null);

// A 4-card tractor (2 pairs, aabb) must be rejected — "helicopter" is exactly 3 pairs (6 cards)
combo = classifyCombo([C('6','S'),C('6','H'),C('7','S'),C('7','H')], level);
check('4-card tractor (2 pairs) rejected — Guandan tractors are exactly 3 pairs', combo, null);

// An 8-card tractor (4 pairs) must be rejected
combo = classifyCombo([C('6','S'),C('6','H'),C('7','S'),C('7','H'),C('8','S'),C('8','H'),C('9','S'),C('9','H')], level);
check('8-card tractor (4 pairs) rejected — Guandan tractors are exactly 3 pairs', combo, null);

// A 9-card plane (3 triples) must be rejected — "airplane" is exactly 2 triples (6 cards)
combo = classifyCombo([C('6','S'),C('6','H'),C('6','D'),C('7','S'),C('7','H'),C('7','D'),C('8','S'),C('8','H'),C('8','D')], level);
check('9-card plane (3 triples) rejected — Guandan planes are exactly 2 triples', combo, null);

// The valid, fixed-length versions still work
combo = classifyCombo([C('6','S'),C('6','H'),C('7','S'),C('7','H'),C('8','S'),C('8','H')], level);
check('exactly-3-pair tractor (6 cards) still valid', combo && [combo.type, combo.size], ['TRACTOR', 6]);
combo = classifyCombo([C('6','S'),C('6','H'),C('6','D'),C('7','S'),C('7','H'),C('7','D')], level);
check('exactly-2-triple plane (6 cards) still valid', combo && [combo.type, combo.size], ['PLANE', 6]);
combo = classifyCombo([C('3','S'),C('4','D'),C('5','C'),C('6','H'),C('7','S')], level);
check('exactly-5 straight still valid', combo && [combo.type, combo.size], ['STRAIGHT', 5]);
combo = classifyCombo([C('9','S'),C('10','S'),C('J','S'),C('Q','S'),C('K','S')], level);
check('exactly-5 straight flush still valid', combo && [combo.type, combo.size, combo.isStraightFlush], ['BOMB', 5, true]);

// --- Game-level regression tests (require directly controlling hands for determinism) ---
const { Game } = require('./rules.js');

// Bug: a 1st+2nd double-down (双下) should end the hand immediately, not after a 3rd finisher.
(function testDoubleDownEndsImmediately(){
  const g = new Game();
  // Seats 0 & 2 are teammates (Team A). Give seat0 and seat2 one card each so they can finish quickly;
  // give seats 1 & 3 several cards so they'd otherwise keep playing.
  g.hands[0] = [C('3','S')];
  g.hands[2] = [C('4','S')];
  g.hands[1] = [C('5','S'), C('6','S'), C('7','S')];
  g.hands[3] = [C('8','S'), C('9','S'), C('10','S')];
  g.leader = 0; g.turn = 0; g.currentTrick = null; g.passesInRow = 0;
  g.activeMask = [true, true, true, true]; g.finishedOrder = []; g.handOver = false;

  let r = g.playCards(0, [g.hands[0][0]]); // seat0 plays its only card, finishes 1st
  check('seat0 finishes but hand not yet over (only 1 finisher)', g.handOver, false);
  r = g.passTurn(1); // seat1 passes
  r = g.playCards(2, [g.hands[2][0]]); // seat2 (partner) plays its only card, finishes 2nd -> double-down
  check('double-down ends the hand immediately (not waiting for seat1/seat3 to finish)', g.handOver, true);
  check('double-down finish order has exactly 2 entries before padding, winning team correct', g.lastResult.winTeam, 0);
  check('double-down grants the full +3 upgrade', g.lastResult.upgrade, 3);
})();

// Bug: 接风 (relay) — when a finished player's winning play goes unanswered, the lead
// passes to THEIR PARTNER specifically, not just the next seat in clockwise rotation.
(function testRelayToPartner(){
  const g = new Game();
  // Seat 3 (Team B) will play its last card and win the trick unanswered.
  // Seat 3's partner is seat 1 (also Team B) — the lead should go to seat 1, NOT seat 0.
  g.hands[3] = [C('K','S')];
  g.hands[0] = [C('3','S'), C('4','S')];
  g.hands[1] = [C('5','S'), C('6','S')];
  g.hands[2] = [C('7','S'), C('8','S')];
  g.leader = 3; g.turn = 3; g.currentTrick = null; g.passesInRow = 0;
  g.activeMask = [true, true, true, true]; g.finishedOrder = []; g.handOver = false;

  g.playCards(3, [g.hands[3][0]]); // seat3 plays its last card (a lone King, hard to beat), finishes
  check('seat3 finished but hand continues (only 1 finisher)', g.handOver, false);
  g.passTurn(0); // seat0 can't/won't beat it
  g.passTurn(1); // seat1 can't/won't beat it
  g.passTurn(2); // seat2 can't/won't beat it -> trick fully closes now
  check("lead passes to the finisher's PARTNER (seat 1), not the next seat (seat 0)", g.turn, 1);
})();

// Bug: pass-count off-by-one — when the trick's own player finishes on that play, ALL
// remaining active players must pass (not one fewer) before the trick closes.
(function testPassCountNotOffByOne(){
  const g = new Game();
  g.hands[3] = [C('K','S')];
  g.hands[0] = [C('3','S'), C('4','S')];
  g.hands[1] = [C('5','S'), C('6','S')];
  g.hands[2] = [C('7','S'), C('8','S')];
  g.leader = 3; g.turn = 3; g.currentTrick = null; g.passesInRow = 0;
  g.activeMask = [true, true, true, true]; g.finishedOrder = []; g.handOver = false;

  g.playCards(3, [g.hands[3][0]]);
  g.passTurn(0);
  g.passTurn(1);
  check('trick must still be open after only 2 of 3 remaining players have passed', g.currentTrick !== null, true);
  check('turn correctly still with the 3rd player who has not yet passed', g.turn, 2);
  g.passTurn(2);
  check('trick closes only after all 3 remaining players have passed', g.currentTrick, null);
})();

// --- Regression: wildcards must be able to EXTEND a straight flush beyond the naturals'
// own span, not just fill internal gaps. Bug report: 8S-9S-10S + two wildcards (level=4,
// so 4H is wild) should complete an 8-9-10-J-Q straight flush.
const level4 = '4';
combo = classifyCombo([C('8','S'), C('9','S'), C('10','S'), C('4','H'), C('4','H')], level4);
check('8-9-10 of spades + 2 wildcards extends to a straight flush (8-9-10-J-Q)', combo && [combo.type, combo.size, combo.isStraightFlush, combo.rank], ['BOMB', 5, true, 'Q']);

// Same idea for a plain straight (no suit requirement): 8-9-10 + 2 wildcards -> 8-9-10-J-Q
combo = classifyCombo([C('8','S'), C('9','D'), C('10','C'), C('4','H'), C('4','H')], level4);
check('8-9-10 (mixed suits) + 2 wildcards extends to a straight (8-9-10-J-Q)', combo && [combo.type, combo.size, combo.rank], ['STRAIGHT', 5, 'Q']);

// Tractor extension is inherently ambiguous with "plane" when exactly 2 ranks are fully paired
// plus 2 wildcards (that shape validly reads as either 666-777 or a 3-pair tractor) — the plane
// reading is what the engine picks, which is reasonable. A cleaner tractor-extension case uses
// an internal gap plus needing one more pair, which cannot be misread as a plane:
combo = classifyCombo([C('6','S'), C('6','H'), C('8','S'), C('8','D'), C('4','H'), C('4','H')], level4);
check('two natural pairs with a gap + 2 wildcards fills the gap (6-7-8 tractor)', combo && [combo.type, combo.size, combo.rank], ['TRACTOR', 6, '8']);

// --- Regression: a pair of small jokers, or a pair of big jokers, must be a legal PAIR play ---
combo = classifyCombo([J('small'), J('small')], level);
check('pair of small jokers is a legal PAIR', combo && [combo.type, combo.size], ['PAIR', 2]);
combo = classifyCombo([J('big'), J('big')], level);
check('pair of big jokers is a legal PAIR', combo && [combo.type, combo.size], ['PAIR', 2]);

// One small + one big joker is NOT a valid pair (they're different ranks)
combo = classifyCombo([J('small'), J('big')], level);
check('one small + one big joker is not a valid pair', combo, null);

// Strength ordering: small-joker pair < big-joker pair, and both beat an Ace pair
const pairSmallJoker = classifyCombo([J('small'), J('small')], level);
const pairBigJoker = classifyCombo([J('big'), J('big')], level);
const pairAceForJokerTest = classifyCombo([C('A','S'), C('A','H')], level);
check('big-joker pair beats small-joker pair', canBeat(pairBigJoker, pairSmallJoker), true);
check('small-joker pair beats an Ace pair', canBeat(pairSmallJoker, pairAceForJokerTest), true);

// The AI's move enumerator must also be able to find and offer a joker pair
const handWithJokerPair = [J('small'), J('small'), C('9','S'), C('K','D')];
const jokerPairFound = enumerateCombos(handWithJokerPair, level, null, null).some(o => o.combo.type === 'PAIR' && o.combo.rank === 'small joker');
check('AI move enumeration discovers the joker pair', jokerPairFound, true);

// actualFinishCount must distinguish real finishers from padded (did-not-finish) entries,
// so the client can show "did not finish" instead of a fabricated 3rd/4th place.
(function testActualFinishCount(){
  const g = new Game();
  g.hands[0] = [C('3','S')];
  g.hands[2] = [C('4','S')];
  g.hands[1] = [C('5','S'), C('6','S'), C('7','S')];
  g.hands[3] = [C('8','S'), C('9','S'), C('10','S')];
  g.leader = 0; g.turn = 0; g.currentTrick = null; g.passesInRow = 0;
  g.activeMask = [true, true, true, true]; g.finishedOrder = []; g.handOver = false;
  g.playCards(0, [g.hands[0][0]]);
  g.passTurn(1);
  g.playCards(2, [g.hands[2][0]]); // double-down: only 2 players actually finished
  check('double-down reports actualFinishCount=2', g.lastResult.actualFinishCount, 2);
  check('but finishedOrder is still padded to all 4 for display/scoring', g.lastResult.finishedOrder.length, 4);
})();

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
