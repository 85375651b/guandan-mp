// ===== Guandan core rules engine =====
const RANKS = ['2','3','4','5','6','7','8','9','10','J','Q','K','A']; // trick-rank order, low->high (A is highest normal card; level card and jokers rank above it)
const SUITS = ['S','H','D','C'];
const SUIT_SYMBOL = { S:'♠', H:'♥', D:'♦', C:'♣' };
const SUIT_COLOR = { S:'black', H:'red', D:'red', C:'black' };
const LEVEL_ORDER = ['2','3','4','5','6','7','8','9','10','J','Q','K','A']; // order teams climb through

function rankIdx(rank){ return RANKS.indexOf(rank); }

function buildDeck(){
  const deck = [];
  let uid = 0;
  for (let d = 0; d < 2; d++){
    for (const suit of SUITS){
      for (const rank of RANKS){
        deck.push({ uid: uid++, rank, suit, joker: null });
      }
    }
    deck.push({ uid: uid++, rank: null, suit: null, joker: 'small' });
    deck.push({ uid: uid++, rank: null, suit: null, joker: 'big' });
  }
  return deck;
}

function shuffle(arr){
  for (let i = arr.length - 1; i > 0; i--){
    const j = Math.floor(Math.random() * (i + 1));
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
  return arr;
}

function isWild(card, levelRank){
  return !card.joker && card.suit === 'H' && card.rank === levelRank;
}

// A card's rank "power" for comparison purposes: normally its position in RANKS (0..12),
// but ALL FOUR SUITS of the current level rank sit in an elevated tier above Ace (only the
// Hearts copy additionally acts as a wildcard substitute — that's a separate property).
function effectiveRankValue(rank, levelRank){
  if (rank === levelRank) return 13;
  return rankIdx(rank);
}

// Base comparison value for a SINGLE card (used for sorting/hand display and single-card plays)
function singleValue(card, levelRank){
  if (card.joker === 'big') return 15;
  if (card.joker === 'small') return 14;
  return effectiveRankValue(card.rank, levelRank); // 0..13 (13 = any-suit level card)
}

function cardLabel(card){
  if (card.joker === 'big') return 'BIG JOKER';
  if (card.joker === 'small') return 'small joker';
  return card.rank + SUIT_SYMBOL[card.suit];
}

// Find a consecutive-rank window of exactly `windowLen` ranks that fits the given natural
// rank counts (each slot needs `perSlot` copies, filled by wilds where short). Wildcards can
// extend the run in either direction, not just fill gaps between existing naturals — e.g.
// 8-9-10 of spades + 2 wildcards can become 8-9-10-J-Q, not just plug an internal hole.
// Tries normal order (2..A) and also the documented low-Ace wrap (A-2-3-4-5 style runs),
// since official Guandan rules allow A to sit either after K or immediately before 2.
function findConsecutiveWindow(rankCounts, windowLen, wildCount, perSlot){
  const ranks = Object.keys(rankCounts);
  let best = null;
  for (const lowAce of [false, true]){
    const idxOf = (r) => (lowAce && r === 'A') ? -1 : rankIdx(r);
    const idxToRank = (i) => (lowAce && i === -1) ? 'A' : RANKS[i];
    const lowerBound = lowAce ? -1 : 0;
    const upperBound = 12;
    const idxs = ranks.map(idxOf);
    const minIdx = Math.min(...idxs), maxIdx = Math.max(...idxs);
    // Try every window position that fully contains [minIdx, maxIdx]; if several are valid
    // (wildcards can extend the run in more than one direction), prefer the strongest result.
    const earliestStart = Math.max(lowerBound, maxIdx - windowLen + 1);
    const latestStart = Math.min(minIdx, upperBound - windowLen + 1);
    for (let start = earliestStart; start <= latestStart; start++){
      const end = start + windowLen - 1;
      let ok = true, wildNeed = 0;
      for (let i = start; i <= end; i++){
        const rk = idxToRank(i);
        const have = rankCounts[rk] || 0;
        if (have > perSlot){ ok = false; break; }
        wildNeed += (perSlot - have);
      }
      if (ok && wildNeed === wildCount){
        if (!best || end > best.strength) best = { valid: true, strength: end, topRank: idxToRank(end) };
      }
    }
  }
  return best || { valid: false };
}

// Classify a set of selected cards into a playable combo, or return null if not a valid shape.
// Returns { type, size, strength, rank, isBomb, isJokerBomb, isStraightFlush }
function classifyCombo(cards, levelRank){
  const n = cards.length;
  if (n === 0) return null;

  const jokers = cards.filter(c => c.joker);
  const nonJokers = cards.filter(c => !c.joker);

  // Single (covers a lone joker too)
  if (n === 1){
    return { type: 'SINGLE', size: 1, strength: singleValue(cards[0], levelRank), rank: cards[0].joker || cards[0].rank, isBomb: false };
  }

  // Joker bomb: exactly all 4 jokers (2 small + 2 big), nothing else
  if (jokers.length > 0){
    if (nonJokers.length > 0) return null; // jokers never mix with other cards
    if (jokers.length === 2){
      // A pair of like jokers (two smalls, or two bigs) is a legal natural PAIR.
      const smalls = jokers.filter(j => j.joker === 'small').length;
      const bigs = jokers.filter(j => j.joker === 'big').length;
      if (smalls === 2){
        return { type: 'PAIR', size: 2, strength: 14, rank: 'small joker', isBomb: false };
      }
      if (bigs === 2){
        return { type: 'PAIR', size: 2, strength: 15, rank: 'big joker', isBomb: false };
      }
      return null; // one small + one big is not a valid pair
    }
    if (jokers.length === 4){
      const smalls = jokers.filter(j => j.joker === 'small').length;
      const bigs = jokers.filter(j => j.joker === 'big').length;
      if (smalls === 2 && bigs === 2){
        return { type: 'BOMB', size: 4, strength: 100000, rank: '2', isBomb: true, isJokerBomb: true, isStraightFlush: false, label: 'Joker Bomb' };
      }
    }
    return null; // partial joker groups aren't a legal combo
  }

  const wilds = cards.filter(c => isWild(c, levelRank));
  const naturals = cards.filter(c => !isWild(c, levelRank));
  const wildCount = wilds.length;

  // Group naturals by rank
  const byRank = {};
  for (const c of naturals){
    byRank[c.rank] = byRank[c.rank] || [];
    byRank[c.rank].push(c);
  }
  const distinctRanks = Object.keys(byRank);

  // ---- BOMB (same rank, size >= 4; wildcards may fill remaining slots) ----
  if (distinctRanks.length === 1 && n >= 4 && wildCount <= 2){
    const rank = distinctRanks[0];
    const have = byRank[rank].length;
    if (have + wildCount === n){
      return { type: 'BOMB', size: n, strength: 1000 + n * 100 + 50 + effectiveRankValue(rank, levelRank), rank, isBomb: true, isJokerBomb: false, isStraightFlush: false };
    }
  }

  // ---- STRAIGHT FLUSH (same suit, exactly 5 consecutive ranks; wildcards may fill gaps — Guandan fixes this at exactly 5) ----
  if (n === 5){
    const suits = new Set(naturals.map(c => c.suit));
    const ranksSet = new Set(naturals.map(c => c.rank));
    if (suits.size <= 1 && ranksSet.size === naturals.length){
      const counts = {};
      for (const r of ranksSet) counts[r] = 1;
      const win = findConsecutiveWindow(counts, n, wildCount, 1);
      if (win.valid){
        return { type: 'BOMB', size: n, strength: 1000 + n * 100 + win.strength, rank: win.topRank, isBomb: true, isJokerBomb: false, isStraightFlush: true };
      }
    }
  }

  // ---- PAIR ----
  if (n === 2){
    if (wildCount === 2){
      return { type: 'PAIR', size: 2, strength: 13, rank: levelRank, isBomb: false };
    }
    if (wildCount === 1 && distinctRanks.length === 1 && byRank[distinctRanks[0]].length === 1){
      const rank = distinctRanks[0];
      return { type: 'PAIR', size: 2, strength: effectiveRankValue(rank, levelRank), rank, isBomb: false };
    }
    if (wildCount === 0 && distinctRanks.length === 1 && byRank[distinctRanks[0]].length === 2){
      const rank = distinctRanks[0];
      return { type: 'PAIR', size: 2, strength: effectiveRankValue(rank, levelRank), rank, isBomb: false };
    }
    return null;
  }

  // ---- TRIPLE ----
  if (n === 3){
    if (distinctRanks.length === 1){
      const rank = distinctRanks[0];
      const have = byRank[rank].length;
      if (have + wildCount === 3 && wildCount <= 2){
        return { type: 'TRIPLE', size: 3, strength: effectiveRankValue(rank, levelRank), rank, isBomb: false };
      }
    }
    return null;
  }

  // ---- TRIPLE + PAIR (3+2), simplified: requires natural 3 and natural 2 with at most wild filling either group ----
  if (n === 5 && distinctRanks.length === 2){
    const [r1, r2] = distinctRanks;
    const c1 = byRank[r1].length, c2 = byRank[r2].length;
    // try r1=triple, r2=pair
    for (const [tripleRank, tripleCount, pairRank, pairCount] of [[r1,c1,r2,c2],[r2,c2,r1,c1]]){
      const needTriple = Math.max(0, 3 - tripleCount);
      const needPair = Math.max(0, 2 - pairCount);
      if (tripleCount <= 3 && pairCount <= 2 && needTriple + needPair === wildCount){
        return { type: 'TRIPLE_PAIR', size: 5, strength: effectiveRankValue(tripleRank, levelRank), rank: tripleRank, isBomb: false };
      }
    }
    return null;
  }
  if (n === 5 && distinctRanks.length === 1 && wildCount === 2){
    // e.g. 3 naturals of one rank + 2 wilds -> triple + wild-pair(levelRank) -- rare but allow
    const rank = distinctRanks[0];
    if (byRank[rank].length === 3){
      return { type: 'TRIPLE_PAIR', size: 5, strength: effectiveRankValue(rank, levelRank), rank, isBomb: false };
    }
  }

  // ---- PLANE / "airplane" (exactly 2 consecutive triples, e.g. 666777 — Guandan caps this at exactly 2) ----
  if (n === 6){
    if (distinctRanks.length > 0){
      const counts = {};
      for (const r of distinctRanks) counts[r] = byRank[r].length;
      const win = findConsecutiveWindow(counts, 2, wildCount, 3);
      if (win.valid){
        return { type: 'PLANE', size: n, strength: win.strength, rank: win.topRank, isBomb: false, span: 2 };
      }
    }
  }

  // ---- TRACTOR / "helicopter" (exactly 3 consecutive pairs, e.g. AA2233 — Guandan caps this at exactly 3) ----
  if (n === 6){
    if (distinctRanks.length > 0){
      const counts = {};
      for (const r of distinctRanks) counts[r] = byRank[r].length;
      const win = findConsecutiveWindow(counts, 3, wildCount, 2);
      if (win.valid){
        return { type: 'TRACTOR', size: n, strength: win.strength, rank: win.topRank, isBomb: false, span: 3 };
      }
    }
  }

  // ---- STRAIGHT (exactly 5 consecutive singles — Guandan caps this at exactly 5, unlike DDZ) ----
  if (n === 5){
    const counts = {};
    for (const r of distinctRanks) counts[r] = byRank[r].length > 1 ? 99 : 1; // disqualify natural duplicates within a straight
    if (!Object.values(counts).includes(99)){
      const win = findConsecutiveWindow(counts, n, wildCount, 1);
      if (win.valid){
        return { type: 'STRAIGHT', size: n, strength: win.strength, rank: win.topRank, isBomb: false };
      }
    }
  }

  return null;
}

// Can `combo` legally beat `current` (current may be null if leading a fresh trick)?
function canBeat(combo, current){
  if (!combo) return false;
  if (!current) return true; // leading, any valid combo is fine
  if (combo.isBomb && !current.isBomb) return true;
  if (combo.isBomb && current.isBomb) return combo.strength > current.strength;
  if (!combo.isBomb && current.isBomb) return false;
  // neither is a bomb: must match type and size
  if (combo.type !== current.type) return false;
  if (combo.size !== current.size) return false;
  return combo.strength > current.strength;
}


// ============================================================

// Enumerate a reasonable set of candidate combos from a hand.
// If reqType/reqSize given, only return combos of that shape (plus bombs); used when following.
function enumerateCombos(hand, levelRank, reqType, reqSize){
  const wilds = hand.filter(c => isWild(c, levelRank));
  const naturals = hand.filter(c => !isWild(c, levelRank) && !c.joker);
  const jokers = hand.filter(c => c.joker);
  const byRank = {};
  for (const c of naturals){ (byRank[c.rank] = byRank[c.rank] || []).push(c); }

  const out = [];

  function add(cards){
    const combo = classifyCombo(cards, levelRank);
    if (combo) out.push({ combo, cards });
  }

  // Singles
  for (const c of hand) add([c]);

  // Pairs
  for (const rank of Object.keys(byRank)){
    const g = byRank[rank];
    if (g.length >= 2) add([g[0], g[1]]);
    else if (g.length === 1 && wilds.length >= 1) add([g[0], wilds[0]]);
  }
  if (wilds.length >= 2) add([wilds[0], wilds[1]]);
  // Joker pairs (two smalls, or two bigs)
  const smallJokers = jokers.filter(j => j.joker === 'small');
  const bigJokers = jokers.filter(j => j.joker === 'big');
  if (smallJokers.length >= 2) add([smallJokers[0], smallJokers[1]]);
  if (bigJokers.length >= 2) add([bigJokers[0], bigJokers[1]]);

  // Triples
  for (const rank of Object.keys(byRank)){
    const g = byRank[rank];
    if (g.length >= 3) add([g[0], g[1], g[2]]);
    else if (g.length === 2 && wilds.length >= 1) add([g[0], g[1], wilds[0]]);
    else if (g.length === 1 && wilds.length >= 2) add([g[0], wilds[0], wilds[1]]);
  }

  // Triple + pair (natural only, simple)
  const ranksWith3 = Object.keys(byRank).filter(r => byRank[r].length >= 3);
  const ranksWith2plus = Object.keys(byRank).filter(r => byRank[r].length >= 2);
  for (const t of ranksWith3){
    for (const p of ranksWith2plus){
      if (p === t) continue;
      add([byRank[t][0], byRank[t][1], byRank[t][2], byRank[p][0], byRank[p][1]]);
    }
  }

  // Bombs (natural, full count per rank, size>=4)
  for (const rank of Object.keys(byRank)){
    if (byRank[rank].length >= 4) add(byRank[rank].slice());
  }
  // Joker bomb
  if (jokers.length === 4) add(jokers.slice());

  // Planes: exactly 2 consecutive triples (Guandan's fixed length)
  const tripleableIdxs = Object.keys(byRank).filter(r => byRank[r].length >= 3).map(rankIdx).sort((a,b)=>a-b);
  for (let i = 0; i < tripleableIdxs.length; i++){
    let run = [tripleableIdxs[i]];
    let j = i + 1;
    while (j < tripleableIdxs.length && tripleableIdxs[j] === run[run.length-1] + 1){ run.push(tripleableIdxs[j]); j++; }
    if (run.length >= 2){
      for (let start = 0; start + 2 <= run.length; start++){
        const window = run.slice(start, start + 2);
        const cards = [];
        for (const idx of window){ const rk = RANKS[idx]; cards.push(byRank[rk][0], byRank[rk][1], byRank[rk][2]); }
        add(cards);
      }
    }
    i = j - 1;
  }

  // Tractors ("helicopters"): exactly 3 consecutive pairs (Guandan's fixed length)
  const pairableIdxs = Object.keys(byRank).filter(r => byRank[r].length >= 2).map(rankIdx).sort((a,b)=>a-b);
  for (let i = 0; i < pairableIdxs.length; i++){
    let run = [pairableIdxs[i]];
    let j = i + 1;
    while (j < pairableIdxs.length && pairableIdxs[j] === run[run.length-1] + 1){ run.push(pairableIdxs[j]); j++; }
    if (run.length >= 3){
      for (let start = 0; start + 3 <= run.length; start++){
        const window = run.slice(start, start + 3);
        const cards = [];
        for (const idx of window){ const rk = RANKS[idx]; cards.push(byRank[rk][0], byRank[rk][1]); }
        add(cards);
      }
    }
    i = j - 1;
  }

  // Straights: exactly 5 consecutive singles (Guandan's fixed length)
  const singleIdxs = Object.keys(byRank).map(rankIdx).sort((a,b)=>a-b);
  for (let i = 0; i < singleIdxs.length; i++){
    let run = [singleIdxs[i]];
    let j = i + 1;
    while (j < singleIdxs.length && singleIdxs[j] === run[run.length-1] + 1){ run.push(singleIdxs[j]); j++; }
    if (run.length >= 5){
      for (let start = 0; start + 5 <= run.length; start++){
        const window = run.slice(start, start + 5);
        const cards = window.map(idx => byRank[RANKS[idx]][0]);
        add(cards);
      }
    }
    i = j - 1;
  }

  let results = out;
  if (reqType){
    results = out.filter(o => o.combo.isBomb || (o.combo.type === reqType && o.combo.size === reqSize));
  }
  return results;
}

// ---- AI decision logic ----
function aiChooseLead(hand, levelRank){
  const candidates = enumerateCombos(hand, levelRank, null, null)
    .filter(o => !o.combo.isBomb); // don't lead with bombs unless nothing else
  let pool = candidates.length ? candidates : enumerateCombos(hand, levelRank, null, null);
  if (!pool.length) return null;
  // Prefer shedding the most cards per play (tractors/planes/straights before pairs before singles),
  // since that's how a player actually trying to empty their hand quickly would lead.
  // Within the same combo size, prefer the type least likely to be useful later, then play weakest first.
  const typeTiebreak = { TRACTOR: 0, PLANE: 1, STRAIGHT: 2, TRIPLE_PAIR: 3, TRIPLE: 4, PAIR: 5, SINGLE: 6 };
  pool.sort((a, b) => (b.combo.size - a.combo.size) || (typeTiebreak[a.combo.type] - typeTiebreak[b.combo.type]) || (a.combo.strength - b.combo.strength));
  return pool[0];
}

function aiChooseFollow(hand, levelRank, current, teammateWinning){
  if (teammateWinning) return null; // let partner keep the trick
  const candidates = enumerateCombos(hand, levelRank, current.type, current.size)
    .filter(o => canBeat(o.combo, current));
  if (!candidates.length) return null; // must pass
  candidates.sort((a, b) => a.combo.strength - b.combo.strength);
  // Avoid burning a bomb unless it's the only option
  const nonBomb = candidates.filter(o => !o.combo.isBomb);
  return nonBomb.length ? nonBomb[0] : candidates[0];
}

// ---- Game class ----
class Game {
  constructor(){
    this.teamLevel = [0, 0]; // index into LEVEL_ORDER for team 0 (you+partner) and team 1 (opponents)
    this.bankerTeam = 0;
    this.gameOver = false;
    this.winner = null;
    this.log = [];
    this.startHand(null);
  }

  teamOf(playerIdx){ return playerIdx % 2 === 0 ? 0 : 1; } // seats 0,2 = team0 ; 1,3 = team1

  levelRank(){ return LEVEL_ORDER[this.teamLevel[this.bankerTeam]]; }

  startHand(leaderIdx){
    const deck = shuffle(buildDeck());
    this.hands = [[], [], [], []];
    for (let i = 0; i < deck.length; i++) this.hands[i % 4].push(deck[i]);
    for (const h of this.hands) h.sort((a,b) => singleValue(a, this.levelRank()) - singleValue(b, this.levelRank()));
    this.finishedOrder = [];
    this.activeMask = [true, true, true, true];
    this.currentTrick = null; // { combo, cards, playerIdx }
    this.passesInRow = 0;
    this.leader = leaderIdx === null ? Math.floor(Math.random() * 4) : leaderIdx;
    this.turn = this.leader;
    this.handOver = false;
  }

  legalTurn(playerIdx){ return !this.handOver && this.turn === playerIdx && this.activeMask[playerIdx]; }

  playCards(playerIdx, cards){
    const level = this.levelRank();
    const combo = classifyCombo(cards, level);
    if (!combo) return { ok: false, reason: 'not-a-valid-combo' };
    if (this.currentTrick && !canBeat(combo, this.currentTrick.combo)) return { ok: false, reason: 'does-not-beat-current' };
    // remove cards from hand
    const hand = this.hands[playerIdx];
    for (const c of cards){
      const idx = hand.findIndex(h => h.uid === c.uid);
      if (idx === -1) return { ok: false, reason: 'card-not-in-hand' };
      hand.splice(idx, 1);
    }
    this.currentTrick = { combo, cards, playerIdx };
    this.passesInRow = 0;
    this.log.push(`P${playerIdx} plays ${cards.map(cardLabel).join(' ')} (${combo.type})`);
    if (hand.length === 0){
      this.activeMask[playerIdx] = false;
      this.finishedOrder.push(playerIdx);
      this.log.push(`P${playerIdx} is out!`);
      if (this.finishedOrder.length >= 2 && this.teamOf(this.finishedOrder[0]) === this.teamOf(this.finishedOrder[1])){
        // Double-down (双下): both members of one team finished 1st and 2nd — the hand ends immediately.
        this.endHand();
        return { ok: true, handOver: true };
      }
      if (this.finishedOrder.length >= 3){
        this.endHand();
        return { ok: true, handOver: true };
      }
    }
    this.advanceAfterPlay(playerIdx);
    return { ok: true };
  }

  passTurn(playerIdx){
    if (!this.currentTrick) return { ok: false, reason: 'cannot-pass-when-leading' };
    this.passesInRow++;
    this.log.push(`P${playerIdx} passes`);
    this.advanceAfterPass(playerIdx);
    return { ok: true };
  }

  nextActive(fromIdx){
    let i = fromIdx;
    for (let k = 0; k < 4; k++){
      i = (i + 1) % 4;
      if (this.activeMask[i]) return i;
    }
    return -1;
  }

  advanceAfterPlay(playerIdx){
    if (this.handOver) return;
    const activeCount = this.activeMask.filter(Boolean).length;
    if (activeCount <= 1){ this.endHand(); return; }
    this.turn = this.nextActive(playerIdx);
  }

  partnerOf(seat){ return (seat + 2) % 4; }

  advanceAfterPass(playerIdx){
    const activeCount = this.activeMask.filter(Boolean).length;
    const leaderStillActive = this.activeMask[this.currentTrick.playerIdx];
    const neededPasses = leaderStillActive ? activeCount - 1 : activeCount;
    if (this.passesInRow >= neededPasses){
      // trick closes; winner leads next
      const winner = this.currentTrick.playerIdx;
      this.log.push(`Trick won by P${winner}`);
      this.currentTrick = null;
      this.passesInRow = 0;
      if (!this.activeMask[winner]){
        // 接风 (relay): the winner already finished their hand — the lead passes to
        // their PARTNER specifically, not just the next seat in rotation.
        const partner = this.partnerOf(winner);
        this.turn = this.activeMask[partner] ? partner : this.nextActive(winner);
      } else {
        this.turn = winner;
      }
      this.leader = this.turn;
    } else {
      this.turn = this.nextActive(playerIdx);
    }
  }

  endHand(){
    this.handOver = true;
    const actualFinishCount = this.finishedOrder.length; // before padding
    // whoever hasn't finished is last place (padded on for scoring/display purposes only)
    for (let i = 0; i < 4; i++){ if (this.activeMask[i] && !this.finishedOrder.includes(i)) this.finishedOrder.push(i); }
    const [p1, p2, p3] = this.finishedOrder;
    const t1 = this.teamOf(p1), t2 = this.teamOf(p2);
    const winTeam = t1;
    const loseTeam = 1 - t1;
    let upgrade;
    if (t2 === winTeam) upgrade = 3; // teammate finished 2nd (double-down)
    else {
      const p3team = this.teamOf(p3);
      if (p3team === winTeam) upgrade = 2; // teammate finished 3rd
      else upgrade = 1; // teammate finished last
    }
    this.bankerTeam = winTeam;
    this.teamLevel[winTeam] = this.teamLevel[winTeam] + upgrade;
    this.lastResult = { finishedOrder: this.finishedOrder.slice(), actualFinishCount, winTeam, upgrade };
    this.log.push(`Hand over. Finish order: ${this.finishedOrder.join(',')}. Team ${winTeam} upgrades by ${upgrade}.`);
    if (this.teamLevel[winTeam] >= LEVEL_ORDER.length){
      this.gameOver = true;
      this.winner = winTeam;
      this.log.push(`Team ${winTeam} wins the game!`);
    }
  }
}


module.exports = { RANKS, SUITS, SUIT_SYMBOL, SUIT_COLOR, LEVEL_ORDER, rankIdx, buildDeck, shuffle, isWild, singleValue, cardLabel, classifyCombo, canBeat, enumerateCombos, aiChooseLead, aiChooseFollow, Game };
