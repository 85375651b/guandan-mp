// Optional migration audit tool. Cargo tests read the generated JSON and do not need Node.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const rules = require('../legacy/rules.js');
const cases = [];
const rank = r => ({ J:11,Q:12,K:13,A:14,small:15,big:16,'small joker':15,'big joker':16 }[r] || Number(r));
const kind = { SINGLE:'Single',PAIR:'Pair',TRIPLE:'Triple',TRIPLE_PAIR:'FullHouse',STRAIGHT:'Straight',TRACTOR:'Tractor',PLANE:'Plane',BOMB:'Bomb' };
const suit = { S:'Spades',H:'Hearts',D:'Diamonds',C:'Clubs' };
const wrapped = { ...rules, classifyCombo(cards,level) {
  const result = rules.classifyCombo(cards,level);
  cases.push({
    cards: cards.map((c,i) => ({uid:c.uid ?? i,rank:c.joker ? rank(c.joker) : rank(c.rank),suit:c.joker ? 'Joker' : suit[c.suit]})),
    level:rank(level),
    expected:result && {kind:kind[result.type],size:result.size,strength:result.strength,rank:rank(result.rank),bomb:result.isJokerBomb?'Jokers':result.isStraightFlush?'StraightFlush':result.isBomb?'Rank':'None'},
  });
  return result;
} };
vm.runInNewContext(fs.readFileSync(path.join(__dirname,'../legacy/test.js'),'utf8'), {
  require: name => { if(name === './rules.js') return wrapped; throw new Error(name); },
  console:{log(){}}, process:{exit(code){if(code)throw new Error('Legacy tests failed');}},
});
fs.mkdirSync(path.join(__dirname,'../tests/fixtures'),{recursive:true});
fs.writeFileSync(path.join(__dirname,'../tests/fixtures/combos.json'),JSON.stringify(cases,null,2)+'\n');
console.log(`Exported ${cases.length} combination fixtures.`);
