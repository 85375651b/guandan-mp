const { Game, aiChooseLead, aiChooseFollow } = require('./rules.js');

function simulateOneGame(gameIdx){
  const g = new Game();
  let handCount = 0;
  let turnGuard = 0;
  while (!g.gameOver){
    handCount++;
    if (handCount > 200) throw new Error('too many hands, likely stuck');
    while (!g.handOver){
      turnGuard++;
      if (turnGuard > 20000) throw new Error('turn guard tripped, likely infinite loop');
      const p = g.turn;
      const hand = g.hands[p];
      const level = g.levelRank();
      if (g.currentTrick && g.currentTrick.playerIdx !== p){
        const teammateWinning = g.teamOf(g.currentTrick.playerIdx) === g.teamOf(p);
        const choice = aiChooseFollow(hand, level, g.currentTrick.combo, teammateWinning);
        if (choice){
          const res = g.playCards(p, choice.cards);
          if (!res.ok) throw new Error('AI produced illegal follow play: ' + res.reason);
        } else {
          const res = g.passTurn(p);
          if (!res.ok) throw new Error('AI produced illegal pass: ' + res.reason);
        }
      } else {
        // leading a fresh trick (either no current trick, or trick already came back to the opener - shouldn't happen but guard)
        const choice = aiChooseLead(hand, level);
        if (!choice) throw new Error('AI had no legal lead move with a non-empty hand: ' + hand.length);
        const res = g.playCards(p, choice.cards);
        if (!res.ok) throw new Error('AI produced illegal lead play: ' + res.reason);
      }
    }
    if (!g.gameOver){
      g.startHand(g.lastResult.finishedOrder[0]);
    }
  }
  return { handCount, winner: g.winner, teamLevel: g.teamLevel.slice() };
}

let ok = 0, failCount = 0;
const N = 30;
for (let i = 0; i < N; i++){
  try {
    const r = simulateOneGame(i);
    ok++;
    console.log(`game ${i}: OK, ${r.handCount} hands, winner team ${r.winner}, levels ${r.teamLevel}`);
  } catch (e){
    failCount++;
    console.log(`game ${i}: FAILED - ${e.message}`);
  }
}
console.log(`\n${ok}/${N} simulated games completed successfully`);
process.exit(failCount ? 1 : 0);
