//! Offline heuristics with bounded hand planning and public information tracking.
//! No opponent hand enters the policy. Level 2 is retained for comparisons.
mod knowledge;

use crate::cards::Card;
use crate::game::PublicAction;
use crate::rules::{candidates, classify, Bomb, Combo, Kind, Move};
use knowledge::Knowledge;
use std::collections::{HashMap, HashSet};

const PLANNED_ALTERNATIVES: usize = 24;
const SEARCH_WORK_PER_ALTERNATIVE: usize = 8_000;

/// Counts include finished players (zero). Seats opposite one another partner.
pub struct Context<'a> {
    pub seat: usize,
    pub counts: [usize; 4],
    pub trick: Option<(usize, &'a Combo)>,
}

#[derive(Clone)]
struct OptionMove {
    mask: u32,
    play: Move,
}

/// Enumerate suit/copy alternatives as well as substitutions that keep a natural
/// card in hand. The basic rules generator deliberately emits fewer alternatives;
/// that is insufficient for partitioning (two plays may need the same first copy).
fn options(hand: &[Card], level: u8) -> Vec<OptionMove> {
    let wilds: Vec<_> = hand
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_wild(level))
        .map(|(i, _)| i)
        .collect();
    let groups: Vec<Vec<usize>> = (2..=16)
        .map(|rank| {
            hand.iter()
                .enumerate()
                .filter(|(_, c)| c.rank == rank && !c.is_wild(level))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut shapes = HashSet::new();
    for seed in candidates(hand, level) {
        let mut needs = [0u8; 15];
        let mut used_wilds = 0;
        for c in &seed.cards {
            if c.is_wild(level) {
                used_wilds += 1;
            } else {
                needs[(c.rank - 2) as usize] += 1;
            }
        }
        let suit = (seed.combo.bomb == Bomb::StraightFlush)
            .then(|| seed.cards.iter().find(|c| !c.is_wild(level)).unwrap().suit);
        if !shapes.insert((needs, used_wilds, suit)) {
            continue;
        }
        let mut partial = vec![(0u32, used_wilds)];
        for (r, &need) in needs.iter().enumerate().filter(|(_, n)| **n > 0) {
            let group: Vec<_> = groups[r]
                .iter()
                .copied()
                .filter(|&i| suit.is_none_or(|s| hand[i].suit == s))
                .collect();
            let mut next = Vec::new();
            for (mask, used) in partial {
                let replace = if r >= 13 {
                    0
                } else {
                    (need as usize).min(wilds.len() - used)
                };
                for extra in 0..=replace {
                    let mut subsets = Vec::new();
                    subsets_of(hand, &group, need as usize - extra, 0, 0, &mut subsets);
                    next.extend(subsets.into_iter().map(|m| (mask | m, used + extra)));
                }
            }
            partial = next;
        }
        for (mut mask, used) in partial {
            for &i in &wilds[..used] {
                mask |= 1 << i;
            }
            if !seen.insert(mask) {
                continue;
            }
            // Keep authoritative hand order for ambiguous wildcard full houses.
            let cards: Vec<_> = hand
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, c)| *c)
                .collect();
            if let Some(combo) = classify(&cards, level) {
                out.push(OptionMove {
                    mask,
                    play: Move { cards, combo },
                });
            }
        }
    }
    out.sort_by_key(|m| {
        (
            std::cmp::Reverse(m.play.cards.len()),
            weakness(&m.play.combo),
            m.play.combo.strength,
            m.mask,
        )
    });
    out
}

fn subsets_of(
    hand: &[Card],
    group: &[usize],
    need: usize,
    start: usize,
    mask: u32,
    out: &mut Vec<u32>,
) {
    if need == 0 {
        out.push(mask);
        return;
    }
    if group.len() - start < need {
        return;
    }
    let mut suits = HashSet::new();
    for j in start..=group.len() - need {
        // Identical copies are interchangeable. Always keep a canonical prefix.
        if suits.insert(hand[group[j]].suit) {
            subsets_of(hand, group, need - 1, j + 1, mask | (1 << group[j]), out);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Plan {
    plays: u8,
    weakness: u16,
}

fn weakness(c: &Combo) -> u16 {
    if c.bomb != Bomb::None {
        return 0;
    }
    let weight = match c.kind {
        Kind::Single => 8,
        Kind::Pair => 4,
        _ => 2,
    };
    (15 - c.strength.clamp(0, 15)) as u16 * weight
}

struct Planner {
    moves: Vec<OptionMove>,
    by_card: Vec<Vec<usize>>,
    copies: Vec<u32>,
    cache: HashMap<u32, Plan>,
    greedy_cache: HashMap<u32, Plan>,
    max_size: u32,
    single_costs: Vec<u16>,
}

impl Planner {
    fn new(hand: &[Card], level: u8) -> Self {
        let moves = options(hand, level);
        let mut by_card = vec![Vec::new(); hand.len()];
        for (j, m) in moves.iter().enumerate() {
            for (i, list) in by_card.iter_mut().enumerate() {
                if m.mask & (1 << i) != 0 {
                    list.push(j);
                }
            }
        }
        let mut copies: Vec<u32> = Vec::new();
        for (i, c) in hand.iter().enumerate() {
            if hand[..i]
                .iter()
                .any(|p| p.rank == c.rank && p.suit == c.suit)
            {
                continue;
            }
            copies.push(
                hand.iter()
                    .enumerate()
                    .filter(|(_, p)| p.rank == c.rank && p.suit == c.suit)
                    .fold(0, |mask, (j, _)| mask | (1 << j)),
            );
        }
        let max_size = moves.iter().map(|m| m.mask.count_ones()).max().unwrap_or(1);
        let single_costs = hand
            .iter()
            .map(|c| (15 - c.power(level)) as u16 * 8)
            .collect();
        Self {
            moves,
            by_card,
            copies,
            cache: HashMap::new(),
            greedy_cache: HashMap::new(),
            max_size,
            single_costs,
        }
    }

    fn canonical(&self, mask: u32) -> u32 {
        let mut out = 0;
        for &group in &self.copies {
            let mut available = group;
            for _ in 0..(mask & group).count_ones() {
                let bit = available.isolate_lowest_one();
                out |= bit;
                available ^= bit;
            }
        }
        out
    }

    /// A feasible upper bound, never an optimistic guess about unseen hands.
    fn greedy(&mut self, mask: u32) -> Plan {
        if let Some(&p) = self.greedy_cache.get(&mask) {
            return p;
        }
        let mut remaining = mask;
        let mut plan = Plan {
            plays: 0,
            weakness: 0,
        };
        for m in &self.moves {
            if m.play.cards.len() == 1 {
                break;
            }
            if remaining & m.mask == m.mask {
                remaining = self.canonical(remaining ^ m.mask);
                plan.plays += 1;
                plan.weakness += weakness(&m.play.combo);
                if remaining == 0 {
                    break;
                }
            }
        }
        for (i, &cost) in self.single_costs.iter().enumerate() {
            if remaining & (1 << i) != 0 {
                plan.plays += 1;
                plan.weakness += cost;
            }
        }
        self.greedy_cache.insert(mask, plan);
        plan
    }

    /// Minimum set cover, branching on a card that must occur in some play.
    /// A deterministic work budget bounds cost on large, wildcard-rich hands.
    /// Cached interrupted searches retain feasible partitions, not exact proofs.
    fn search(&mut self, mask: u32, work: &mut usize) -> Plan {
        if let Some(&p) = self.cache.get(&mask) {
            return p;
        }
        let mut best = self.greedy(mask);
        if best.plays <= 1 || *work == 0 {
            return best;
        }
        let pivot = self
            .copies
            .iter()
            .map(|g| g & mask)
            .filter(|g| *g != 0)
            .map(|g| g.trailing_zeros() as usize)
            .min_by_key(|&i| self.by_card[i].len())
            .unwrap();
        for j in self.by_card[pivot].clone() {
            if *work == 0 {
                break;
            }
            *work -= 1;
            let m = &self.moves[j];
            if m.mask & mask != m.mask {
                continue;
            }
            let rest = self.canonical(mask ^ m.mask);
            if 1 + rest.count_ones().div_ceil(self.max_size) > u32::from(best.plays) {
                continue;
            }
            let cost = weakness(&m.play.combo);
            let mut plan = self.search(rest, work);
            plan.plays += 1;
            plan.weakness += cost;
            best = best.min(plan);
        }
        self.cache.insert(mask, best);
        best
    }
}

/// Level 2 policy, retained for reproducible strength comparisons.
pub fn choose_move(hand: &[Card], level: u8, context: &Context<'_>) -> Option<Move> {
    choose(hand, level, context, None)
}

/// Level 3 policy: own cards, public counts, and this deal's public actions only.
pub fn choose_move_with_history(
    hand: &[Card],
    level: u8,
    context: &Context<'_>,
    history: &[PublicAction],
) -> Option<Move> {
    let knowledge = Knowledge::new(hand, level, context, history);
    choose(hand, level, context, Some(&knowledge))
}

fn choose(
    hand: &[Card],
    level: u8,
    context: &Context<'_>,
    knowledge: Option<&Knowledge>,
) -> Option<Move> {
    if hand.is_empty() {
        return None;
    }
    // The physical game has at most 27 cards per hand, including during tribute.
    assert!(hand.len() <= 32);
    let partner = (context.seat + 2) % 4;
    let partner_count = context.counts[partner];
    let opponent_count = (0..4)
        .filter(|s| s % 2 != context.seat % 2)
        .map(|s| context.counts[s])
        .filter(|n| *n > 0)
        .min()
        .unwrap_or(27);
    let current = context.trick.map(|(_, c)| c);
    let partner_winning = context.trick.is_some_and(|(s, _)| s == partner);
    let mut planner = Planner::new(hand, level);
    let legal: Vec<_> = planner
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.play.combo.beats(current))
        .map(|(j, _)| j)
        .collect();
    // Going out is decisive even when our partner currently holds the trick.
    if let Some(&j) = legal
        .iter()
        .find(|&&j| planner.moves[j].play.cards.len() == hand.len())
    {
        return Some(planner.moves[j].play.clone());
    }
    if legal.is_empty() {
        return None;
    }
    let mut threat = current.is_some_and(|c| {
        (0..4).any(|s| {
            s % 2 != context.seat % 2
                && (1..=c.size.max(2)).contains(&context.counts[s])
                && context.counts[s] >= c.size
        })
    });
    if let (Some(info), Some((owner, c))) = (knowledge, context.trick) {
        let next_leader = if context.counts[owner] == 0 {
            (owner + 2) % 4
        } else {
            owner
        };
        // Even a one-card opponent who cannot beat this pair can go out after
        // winning the lead. Include the relay from a finished opponent as well.
        threat = info.opponents(c, true) > 0.1
            || (next_leader % 2 != context.seat % 2
                && (1..=2).contains(&context.counts[next_leader]));
    }
    let leave_partner = partner_count == 0
        || !threat
        || knowledge.map_or_else(
            || current.is_some_and(|c| c.strength >= 13),
            |info| current.is_some_and(|c| info.opponents(c, false) < 0.1),
        );
    if partner_winning && leave_partner {
        return None;
    }
    let full = if hand.len() == 32 {
        u32::MAX
    } else {
        (1 << hand.len()) - 1
    };
    // Identify disjoint bombs to protect; do not count overlapping alternatives
    // repeatedly when, for instance, five of a rank also contain four-card bombs.
    let mut reserve = 0;
    let mut bombs = Vec::new();
    for m in &planner.moves {
        if m.play.combo.bomb != Bomb::None && m.mask & reserve == 0 {
            reserve |= m.mask;
            bombs.push(m.mask);
        }
    }
    let mut ranked: Vec<_> = legal
        .into_iter()
        .map(|j| {
            let rest = planner.canonical(full ^ planner.moves[j].mask);
            let p = planner.greedy(rest);
            (j, rest, p)
        })
        .collect();
    let evaluate = |m: &OptionMove, p: Plan| {
        let mut result = score(m, p, &bombs, level, context, opponent_count, partner_count);
        if let Some(info) = knowledge {
            result.0 += information_score(m, p, info, context);
        }
        result
    };
    ranked.sort_by_key(|&(j, _, p)| evaluate(&planner.moves[j], p));
    // Refine the most promising alternatives within a work budget. Enumeration and
    // partitioning share no randomness, so identical positions give the same move.
    for item in ranked.iter_mut().take(PLANNED_ALTERNATIVES) {
        let mut work = SEARCH_WORK_PER_ALTERNATIVE;
        item.2 = planner.search(item.1, &mut work);
    }
    ranked.sort_by_key(|&(j, _, p)| evaluate(&planner.moves[j], p));
    for (j, _, plan) in ranked {
        let m = &planner.moves[j];
        // Do not spend a bomb simply because it is the only legal response.
        // Stop an imminent finish or establish a short route out of our hand.
        if current.is_some() && m.play.combo.bomb != Bomb::None && !threat && plan.plays > 1 {
            continue;
        }
        return Some(m.play.clone());
    }
    None
}

fn information_score(m: &OptionMove, plan: Plan, info: &Knowledge, context: &Context<'_>) -> i32 {
    let risk = info.opponents(&m.play.combo, false);
    let finish = info.opponents(&m.play.combo, true);
    let mut cost = risk * 120.0 + finish * 1800.0;
    if plan.plays == 1 {
        // Establish control and then go out. Counting exhausted high cards can
        // prove a modest pair is sufficient; a joker need not be spent instead.
        cost += (2.0 * risk - 1.0) * 800.0;
    }
    if context.trick.is_none() {
        let partner = (context.seat + 2) % 4;
        if let Some(last) = info.known_finish(partner) {
            if last.beats(Some(&m.play.combo)) {
                cost -= 600.0;
            } else {
                cost += 350.0;
            }
        }
        if m.play.combo.kind == Kind::Single
            && risk < 1e-12
            && (0..4).any(|s| s % 2 != context.seat % 2 && context.counts[s] == 1)
        {
            // A proven winning nine is sufficient; keep a joker for later.
            cost -= 1400.0 - f64::from(m.play.combo.strength) * 30.0;
        }
    }
    cost.round() as i32
}

fn score(
    m: &OptionMove,
    plan: Plan,
    bombs: &[u32],
    level: u8,
    context: &Context<'_>,
    opponent: usize,
    partner: usize,
) -> (i32, i32, u32) {
    let c = &m.play.combo;
    let bomb = c.bomb != Bomb::None;
    let current = context.trick.map(|(_, c)| c);
    let urgent = opponent <= 2;
    let mut cost = i32::from(plan.plays) * 1000 + i32::from(plan.weakness);
    if bomb {
        cost += if urgent || plan.plays <= 1 { 100 } else { 1200 };
    }
    if !bomb {
        let broken = bombs
            .iter()
            .filter(|&&b| b & m.mask != 0 && b & m.mask != b)
            .count();
        cost += broken as i32 * if urgent { 40 } else { 220 };
    }
    for card in &m.play.cards {
        cost += if card.is_wild(level) {
            90
        } else if card.is_joker() {
            45
        } else {
            0
        };
    }
    if current.is_none() {
        cost += c.strength.min(15) * 5;
        // A small, low lead gives a nearly-finished partner a chance to go out.
        if (1..=2).contains(&partner) && opponent > 2 && c.size == partner && !bomb {
            cost -= 650;
        }
        if opponent == 1 && c.kind == Kind::Single {
            cost += 1400 - c.strength * 30;
        }
    } else if urgent || (1..=2).contains(&partner) {
        // Secure the lead to block an opponent or feed our partner next turn.
        cost -= c.strength.min(15) * 15;
    } else {
        cost += c.strength.min(15);
    }
    (cost, c.strength, m.mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::deck;
    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    fn exhaustive_minimum(hand: &[Card], level: u8) -> u8 {
        let end = 1usize << hand.len();
        let legal: Vec<_> = (1..end)
            .filter(|mask| {
                let cards: Vec<_> = hand
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, c)| *c)
                    .collect();
                classify(&cards, level).is_some()
            })
            .collect();
        let mut dp = vec![hand.len() as u8; end];
        dp[0] = 0;
        for mask in 1..end {
            let pivot = 1 << mask.trailing_zeros();
            for &play in &legal {
                if play & pivot != 0 && play & mask == play {
                    dp[mask] = dp[mask].min(1 + dp[mask ^ play]);
                }
            }
        }
        dp[end - 1]
    }

    #[test]
    fn partition_search_matches_exhaustive_small_hand_solver() {
        let mut rng = StdRng::seed_from_u64(6801);
        for case in 0..130 {
            let level = case % 13 + 2;
            let low = if case % 2 == 0 { 2 } else { 9 };
            let mut pool: Vec<_> = deck()
                .into_iter()
                .filter(|c| (low..low + 6).contains(&c.rank) || c.is_joker())
                .filter(|c| !c.is_wild(level))
                .collect();
            pool.shuffle(&mut rng);
            let wild_count = (case % 3) as usize;
            let mut hand = pool[..10 - wild_count].to_vec();
            hand.extend(
                deck()
                    .into_iter()
                    .filter(|c| c.is_wild(level))
                    .take(wild_count),
            );
            hand.sort_by_key(|c| (c.power(level), c.rank, c.uid));
            let expected = exhaustive_minimum(&hand, level);
            let mut planner = Planner::new(&hand, level);
            let actual = planner.search((1 << hand.len()) - 1, &mut 1_000_000);
            assert_eq!(actual.plays, expected, "hand: {hand:?}");
        }
    }

    #[test]
    fn partition_can_use_identical_copies_in_separate_straights() {
        let mut hand: Vec<_> = deck()
            .into_iter()
            .filter(|c| c.suit == crate::cards::Suit::Spades && (3..=7).contains(&c.rank))
            .collect();
        hand.sort_by_key(|c| (c.rank, c.uid));
        let mut planner = Planner::new(&hand, 2);
        assert_eq!(
            planner.search((1 << hand.len()) - 1, &mut 1_000_000).plays,
            2
        );
    }

    #[test]
    fn alternatives_include_spending_wild_to_keep_a_natural_straight_flush() {
        let hand: Vec<_> = deck()
            .into_iter()
            .filter(|c| {
                (c.suit == crate::cards::Suit::Spades && (3..=8).contains(&c.rank)) || c.is_wild(2)
            })
            .take(7)
            .collect();
        let wild = hand.iter().find(|c| c.is_wild(2)).unwrap();
        assert!(options(&hand, 2)
            .iter()
            .any(|m| m.play.combo.bomb == Bomb::StraightFlush && m.play.cards.contains(wild)));
    }
}
