use crate::cards::{Card, Suit, BIG_JOKER, SMALL_JOKER};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Single,
    Pair,
    Triple,
    FullHouse,
    Straight,
    Tractor,
    Plane,
    Bomb,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Pair => "Pair",
            Self::Triple => "Triple",
            Self::FullHouse => "Triple + pair",
            Self::Straight => "Straight",
            Self::Tractor => "Three consecutive pairs",
            Self::Plane => "Two consecutive triples",
            Self::Bomb => "Bomb",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bomb {
    None,
    Rank,
    StraightFlush,
    Jokers,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combo {
    pub kind: Kind,
    pub size: usize,
    pub strength: i32,
    pub rank: u8,
    pub bomb: Bomb,
}

impl Combo {
    pub fn label(&self) -> &'static str {
        match self.bomb {
            Bomb::StraightFlush => "Straight flush",
            Bomb::Jokers => "Joker bomb",
            _ => self.kind.label(),
        }
    }
    pub fn beats(&self, other: Option<&Combo>) -> bool {
        let Some(other) = other else {
            return true;
        };
        match (self.bomb != Bomb::None, other.bomb != Bomb::None) {
            (true, false) => true,
            (false, true) => false,
            (true, true) => self.strength > other.strength,
            (false, false) => {
                self.kind == other.kind && self.size == other.size && self.strength > other.strength
            }
        }
    }
}

fn value(rank: u8, level: u8) -> i32 {
    if rank == level {
        13
    } else {
        i32::from(rank) - 2
    }
}
fn combo(kind: Kind, size: usize, strength: i32, rank: u8) -> Combo {
    Combo {
        kind,
        size,
        strength,
        rank,
        bomb: Bomb::None,
    }
}

// A may be low or high, but runs cannot wrap through K-A-2. Wildcards fill gaps
// or either end. Preserve the existing table's strongest-run interpretation.
fn window(counts: &[usize; 17], span: i32, per: usize, wilds: usize) -> Option<(i32, u8)> {
    let mut best = None;
    for low_ace in [false, true] {
        let idx = |r: u8| {
            if low_ace && r == 14 {
                -1
            } else {
                i32::from(r) - 2
            }
        };
        for start in (if low_ace { -1 } else { 0 })..=(13 - span) {
            let end = start + span - 1;
            if (2..=14).any(|r| counts[r] > 0 && !(start..=end).contains(&idx(r as u8))) {
                continue;
            }
            let mut need = 0;
            let mut valid = true;
            for i in start..=end {
                let rank = if i == -1 { 14 } else { (i + 2) as usize };
                if counts[rank] > per {
                    valid = false;
                    break;
                }
                need += per - counts[rank];
            }
            if valid && need == wilds && best.is_none_or(|(strength, _)| end > strength) {
                best = Some((end, if end == -1 { 14 } else { (end + 2) as u8 }));
            }
        }
    }
    best
}

pub fn classify(cards: &[Card], level: u8) -> Option<Combo> {
    let n = cards.len();
    if n == 0 {
        return None;
    }
    if n == 1 {
        return Some(combo(Kind::Single, 1, cards[0].power(level), cards[0].rank));
    }
    let jokers = cards.iter().filter(|c| c.is_joker()).count();
    if jokers > 0 {
        // A heart wildcard can never impersonate either joker.
        if jokers != n {
            return None;
        }
        if n == 2 && cards[0].rank == cards[1].rank {
            return Some(combo(Kind::Pair, 2, cards[0].power(level), cards[0].rank));
        }
        if n == 4
            && cards.iter().filter(|c| c.rank == SMALL_JOKER).count() == 2
            && cards.iter().filter(|c| c.rank == BIG_JOKER).count() == 2
        {
            return Some(Combo {
                kind: Kind::Bomb,
                size: 4,
                strength: 100000,
                rank: 2,
                bomb: Bomb::Jokers,
            });
        }
        return None;
    }
    let wilds = cards.iter().filter(|c| c.is_wild(level)).count();
    let naturals: Vec<_> = cards
        .iter()
        .copied()
        .filter(|c| !c.is_wild(level))
        .collect();
    let mut counts = [0; 17];
    for c in &naturals {
        counts[c.rank as usize] += 1;
    }
    // JS object keys sorted numeric ranks first, then face ranks in hand order.
    let mut ranks: Vec<u8> = (2..=10).filter(|r| counts[*r as usize] > 0).collect();
    for c in &naturals {
        if c.rank >= 11 && !ranks.contains(&c.rank) {
            ranks.push(c.rank);
        }
    }
    if ranks.len() == 1 && n >= 4 && wilds <= 2 {
        let rank = ranks[0];
        return Some(Combo {
            kind: Kind::Bomb,
            size: n,
            strength: 1050 + n as i32 * 100 + value(rank, level),
            rank,
            bomb: Bomb::Rank,
        });
    }
    if n == 5
        && naturals
            .iter()
            .map(|c| c.suit)
            .collect::<HashSet<_>>()
            .len()
            <= 1
        && counts.iter().all(|n| *n <= 1)
    {
        if let Some((strength, rank)) = window(&counts, 5, 1, wilds) {
            return Some(Combo {
                kind: Kind::Bomb,
                size: 5,
                strength: 1500 + strength,
                rank,
                bomb: Bomb::StraightFlush,
            });
        }
    }
    if n == 2 {
        if wilds == 2 {
            return Some(combo(Kind::Pair, 2, 13, level));
        }
        if ranks.len() == 1 {
            return Some(combo(Kind::Pair, 2, value(ranks[0], level), ranks[0]));
        }
        return None;
    }
    if n == 3 {
        return if ranks.len() == 1 {
            Some(combo(Kind::Triple, 3, value(ranks[0], level), ranks[0]))
        } else {
            None
        };
    }
    if n == 5 && ranks.len() == 2 {
        for (triple, pair) in [(ranks[0], ranks[1]), (ranks[1], ranks[0])] {
            let a = counts[triple as usize];
            let b = counts[pair as usize];
            if a <= 3 && b <= 2 && (3 - a) + (2 - b) == wilds {
                return Some(combo(Kind::FullHouse, 5, value(triple, level), triple));
            }
        }
        return None;
    }
    if n == 6 {
        if let Some((strength, rank)) = window(&counts, 2, 3, wilds) {
            return Some(combo(Kind::Plane, 6, strength, rank));
        }
        if let Some((strength, rank)) = window(&counts, 3, 2, wilds) {
            return Some(combo(Kind::Tractor, 6, strength, rank));
        }
    }
    if n == 5 && counts.iter().all(|n| *n <= 1) {
        if let Some((strength, rank)) = window(&counts, 5, 1, wilds) {
            return Some(combo(Kind::Straight, 5, strength, rank));
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct Move {
    pub cards: Vec<Card>,
    pub combo: Combo,
}

pub fn candidates(hand: &[Card], level: u8) -> Vec<Move> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |cards: Vec<Card>| {
        let mut key: Vec<_> = cards.iter().map(|c| c.uid).collect();
        key.sort_unstable();
        if seen.insert(key) {
            // Resolve ambiguous wildcard shapes in the same hand order as Game::play.
            let ids: HashSet<_> = cards.iter().map(|c| c.uid).collect();
            let cards: Vec<_> = hand
                .iter()
                .copied()
                .filter(|c| ids.contains(&c.uid))
                .collect();
            if let Some(combo) = classify(&cards, level) {
                out.push(Move { cards, combo });
            }
        }
    };
    let wilds: Vec<_> = hand.iter().copied().filter(|c| c.is_wild(level)).collect();
    let groups: Vec<Vec<Card>> = (2..=16)
        .map(|rank| {
            hand.iter()
                .copied()
                .filter(|c| c.rank == rank && !c.is_wild(level))
                .collect()
        })
        .collect();
    for c in hand {
        add(vec![*c]);
    }
    for group in &groups {
        for n in 2..=group.len() + wilds.len() {
            if group.is_empty() || (group[0].is_joker() && n != 2) {
                continue;
            }
            let natural_n = n.min(group.len());
            let mut cards = group[..natural_n].to_vec();
            cards.extend_from_slice(&wilds[..n - natural_n]);
            add(cards);
        }
    }
    if wilds.len() == 2 {
        add(wilds.clone());
    }
    let jokers: Vec<_> = hand.iter().copied().filter(|c| c.is_joker()).collect();
    if jokers.len() == 4 {
        add(jokers);
    }
    for triple in 2..=14 {
        for pair in 2..=14 {
            if triple == pair {
                continue;
            }
            let t = &groups[triple - 2];
            let p = &groups[pair - 2];
            let tn = t.len().min(3);
            let pn = p.len().min(2);
            if tn == 0 || pn == 0 || 5 - tn - pn > wilds.len() {
                continue;
            }
            let mut cards = t[..tn].to_vec();
            cards.extend_from_slice(&p[..pn]);
            cards.extend_from_slice(&wilds[..5 - tn - pn]);
            add(cards);
        }
    }
    // Include low-Ace windows, wildcard-assisted sequences, and each SF suit.
    for (span, per) in [(5, 1), (3, 2), (2, 3)] {
        for start in 1..=15 - span {
            for suit in [
                None,
                Some(Suit::Spades),
                Some(Suit::Hearts),
                Some(Suit::Diamonds),
                Some(Suit::Clubs),
            ] {
                if per != 1 && suit.is_some() {
                    continue;
                }
                let mut cards = Vec::new();
                let mut need = 0;
                for r in start..start + span {
                    let rank = if r == 1 { 14 } else { r as u8 };
                    let group: Vec<_> = groups[(rank - 2) as usize]
                        .iter()
                        .copied()
                        .filter(|c| suit.is_none_or(|s| s == c.suit))
                        .take(per)
                        .collect();
                    need += per - group.len();
                    cards.extend(group);
                }
                if need <= wilds.len() {
                    cards.extend_from_slice(&wilds[..need]);
                    add(cards);
                }
            }
        }
    }
    out
}

/// Original simple policy, retained for simulations and strength comparisons.
/// Live table bots use `crate::bot::choose_move_with_history` with public facts.
pub fn choose_move(
    hand: &[Card],
    level: u8,
    current: Option<&Combo>,
    partner_winning: bool,
) -> Option<Move> {
    if current.is_some() && partner_winning {
        return None;
    }
    let mut moves = candidates(hand, level);
    moves.retain(|m| m.combo.beats(current));
    if current.is_some() {
        moves.sort_by_key(|m| (m.combo.bomb != Bomb::None, m.combo.strength));
    } else {
        moves.sort_by_key(|m| {
            (
                m.combo.bomb != Bomb::None,
                std::cmp::Reverse(m.cards.len()),
                m.combo.strength,
            )
        });
    }
    moves.into_iter().next()
}
