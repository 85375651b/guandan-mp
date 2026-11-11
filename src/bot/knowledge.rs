//! Counts and approximate response probabilities from public facts. The finite
//! deck is known; the locations of undealt-to-us cards are never inspected.
use super::Context;
use crate::{
    cards::{deck, Card, Suit},
    game::PublicAction,
    rules::{candidates, classify, Bomb, Combo, Kind},
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    sync::OnceLock,
};

const CATEGORIES: usize = 69; // wildcard, ranks 2..16, then 13 ranks × 4 suits
type ProbabilityKey = (usize, u8, usize, i32, u8, bool);

struct Pattern {
    combo: Combo,
    needs: [u8; 17],
    wilds: u8,
    suit: Option<Suit>,
}

/// A catalogue of possible responses, not sampled or assigned opponent hands.
/// Validate each wildcard allocation with the same classifier as the game.
fn patterns(level: u8) -> &'static [Pattern] {
    static CACHE: OnceLock<[OnceLock<Vec<Pattern>>; 13]> = OnceLock::new();
    CACHE.get_or_init(|| std::array::from_fn(|_| OnceLock::new()))[(level - 2) as usize]
        .get_or_init(|| {
            let mut cards = deck();
            // Alternate the first suit at each rank so generic straight templates
            // are ordinary straights, alongside the explicit straight flushes.
            cards.sort_by_key(|c| (c.power(level), c.rank, c.suit as u8 != c.rank % 4, c.uid));
            let mut seen = HashSet::new();
            let mut out = Vec::new();
            for m in candidates(&cards, level) {
                let mut needs = [0u8; 17];
                let mut wilds = 0;
                for c in &m.cards {
                    if c.is_wild(level) {
                        wilds += 1;
                    } else {
                        needs[c.rank as usize] += 1;
                    }
                }
                let suit = (m.combo.bomb == Bomb::StraightFlush)
                    .then(|| m.cards.iter().find(|c| !c.is_wild(level)).unwrap().suit);
                let mut allocations = vec![(needs, wilds)];
                for rank in 2..=14 {
                    let mut next = Vec::new();
                    for (need, used) in allocations {
                        for replace in 0..=need[rank].min(2 - used) {
                            let mut n = need;
                            n[rank] -= replace;
                            next.push((n, used + replace));
                        }
                    }
                    allocations = next;
                }
                for (needs, wilds) in allocations {
                    if !seen.insert((needs, wilds, suit)) {
                        continue;
                    }
                    let mut selection = Vec::new();
                    for (rank, &need) in needs.iter().enumerate().skip(2) {
                        selection.extend(
                            cards
                                .iter()
                                .copied()
                                .filter(|c| {
                                    c.rank as usize == rank
                                        && !c.is_wild(level)
                                        && suit.is_none_or(|s| c.suit == s)
                                })
                                .take(need as usize),
                        );
                    }
                    selection.extend(
                        cards
                            .iter()
                            .copied()
                            .filter(|c| c.is_wild(level))
                            .take(wilds as usize),
                    );
                    selection.sort_by_key(|c| (c.power(level), c.rank, c.uid));
                    if let Some(combo) = classify(&selection, level) {
                        out.push(Pattern {
                            combo,
                            needs,
                            wilds,
                            suit,
                        });
                    }
                }
            }
            out
        })
}

fn category(rank: u8, suit: Suit) -> usize {
    17 + (rank as usize - 2) * 4 + suit as usize
}

fn count_card(counts: &mut [u8; CATEGORIES], c: Card, level: u8) {
    if c.is_wild(level) {
        counts[0] += 1;
    } else {
        counts[c.rank as usize] += 1;
        if !c.is_joker() {
            counts[category(c.rank, c.suit)] += 1;
        }
    }
}

fn choose(n: usize, k: usize) -> f64 {
    static COMBINATIONS: OnceLock<[[f64; 28]; 109]> = OnceLock::new();
    if k > n || k > 27 {
        return 0.0;
    }
    COMBINATIONS.get_or_init(|| {
        let mut table = [[0.0; 28]; 109];
        for n in 0..=108 {
            table[n][0] = 1.0;
            for k in 1..=n.min(27) {
                table[n][k] = table[n - 1][k - 1] + table[n - 1][k];
            }
        }
        table
    })[n][k]
}

/// Exact tail for one category drawn without replacement. Combining several
/// categories below is an approximation, never proof of where a card is held.
fn tail(population: usize, successes: usize, draws: usize, need: usize) -> f64 {
    if need == 0 {
        return 1.0;
    }
    if need > successes || need > draws || draws > population {
        return 0.0;
    }
    let denominator = choose(population, draws);
    (need..=successes.min(draws))
        .map(|k| choose(successes, k) * choose(population - successes, draws - k) / denominator)
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

struct Decline {
    seat: usize,
    combo: Combo,
    pressure: bool,
    at: usize,
    contradicted: bool,
}

pub(super) struct Knowledge {
    level: u8,
    seat: usize,
    counts: [usize; 4],
    known_cards: [Vec<Card>; 4],
    unknown: [u8; CATEGORIES],
    known: [[u8; CATEGORIES]; 4],
    population: usize,
    draws: [usize; 4],
    declines: Vec<Decline>,
    plays: usize,
    category_cache: RefCell<Vec<Vec<[f64; 11]>>>,
    holdings: RefCell<Option<Vec<[f64; 4]>>>,
    responses: RefCell<HashMap<ProbabilityKey, f64>>,
}

impl Knowledge {
    pub(super) fn new(
        hand: &[Card],
        level: u8,
        ctx: &Context<'_>,
        history: &[PublicAction],
    ) -> Self {
        let mut played = [false; 108];
        let mut owner = [None; 108];
        let mut declines: Vec<Decline> = Vec::new();
        let mut plays = 0;
        for action in history {
            match action {
                PublicAction::Play(p) => {
                    plays += 1;
                    for c in &p.cards {
                        played[c.uid as usize] = true;
                        owner[c.uid as usize] = None;
                    }
                    for decline in &mut declines {
                        if decline.seat == p.seat
                            && decline.combo.kind == p.combo.kind
                            && decline.combo.size == p.combo.size
                            && p.combo.beats(Some(&decline.combo))
                        {
                            decline.contradicted = true;
                        }
                    }
                }
                PublicAction::Pass {
                    seat,
                    winner,
                    combo,
                    remaining,
                } => {
                    // Partners routinely pass voluntarily. Bomb conservation is
                    // also too common to infer "no bombs" from an ordinary pass.
                    if seat % 2 != winner % 2 && combo.bomb == Bomb::None {
                        declines.push(Decline {
                            seat: *seat,
                            combo: combo.clone(),
                            pressure: *remaining <= 4,
                            at: plays,
                            contradicted: false,
                        });
                    }
                }
                PublicAction::Exchange {
                    from,
                    to,
                    given,
                    returned,
                } => {
                    owner[given.uid as usize] = Some(*to);
                    owner[returned.uid as usize] = Some(*from);
                }
            }
        }
        let own: HashSet<_> = hand.iter().map(|c| c.uid).collect();
        let mut unknown = [0; CATEGORIES];
        let mut known = [[0; CATEGORIES]; 4];
        let mut known_cards: [Vec<Card>; 4] = std::array::from_fn(|_| Vec::new());
        let mut population = 0;
        for c in deck() {
            if played[c.uid as usize] || own.contains(&c.uid) {
                continue;
            }
            if let Some(seat) = owner[c.uid as usize] {
                known_cards[seat].push(c);
                count_card(&mut known[seat], c, level);
            } else {
                count_card(&mut unknown, c, level);
                population += 1;
            }
        }
        let draws = std::array::from_fn(|s| {
            ctx.counts[s]
                .saturating_sub(known_cards[s].len())
                .min(population)
        });
        Self {
            level,
            seat: ctx.seat,
            counts: ctx.counts,
            known_cards,
            unknown,
            known,
            population,
            draws,
            declines,
            plays,
            category_cache: RefCell::new(vec![vec![[-1.0; 11]; CATEGORIES]; 4]),
            holdings: RefCell::new(None),
            responses: RefCell::new(HashMap::new()),
        }
    }

    fn has(&self, seat: usize, category: usize, need: u8) -> f64 {
        if need <= self.known[seat][category] {
            return 1.0;
        }
        let cached = self.category_cache.borrow()[seat][category][need as usize];
        if cached >= 0.0 {
            return cached;
        }
        let p = tail(
            self.population,
            self.unknown[category] as usize,
            self.draws[seat],
            (need - self.known[seat][category]) as usize,
        );
        self.category_cache.borrow_mut()[seat][category][need as usize] = p;
        p
    }

    fn holding(&self, seat: usize, pattern: &Pattern) -> f64 {
        if pattern.combo.size > self.counts[seat] {
            return 0.0;
        }
        let mut missing = pattern.wilds.saturating_sub(self.known[seat][0]) as usize;
        let mut p = self.has(seat, 0, pattern.wilds);
        for rank in 2..=16 {
            let need = pattern.needs[rank];
            if need == 0 {
                continue;
            }
            let category = pattern.suit.map_or(rank, |s| category(rank as u8, s));
            missing += need.saturating_sub(self.known[seat][category]) as usize;
            p *= self.has(seat, category, need);
        }
        if missing > self.draws[seat] {
            0.0
        } else {
            p
        }
    }

    fn pass_weight(&self, seat: usize, response: &Combo) -> f64 {
        if response.bomb != Bomb::None {
            return 1.0;
        }
        let mut weight = 1.0;
        for d in self.declines.iter().filter(|d| {
            d.seat == seat
                && !d.contradicted
                && d.combo.kind == response.kind
                && d.combo.size == response.size
                && response.strength > d.combo.strength
        }) {
            let freshness = 1.0 / (1.0 + (self.plays - d.at) as f64 * 0.08);
            weight *= 1.0 - if d.pressure { 0.4 } else { 0.22 } * freshness;
        }
        weight.max(0.4) // A voluntary pass can never rule a response out.
    }

    fn single_chance(&self, seat: usize, current: &Combo) -> f64 {
        if self.known_cards[seat]
            .iter()
            .any(|c| c.power(self.level) > current.strength)
        {
            return 1.0;
        }
        let mut good = 0;
        let mut weighted = 0.0;
        for rank in 2..=16 {
            let card = Card {
                uid: 0,
                rank,
                suit: Suit::Spades,
            };
            let power = card.power(self.level);
            if power > current.strength {
                let n = self.unknown[rank as usize] as usize
                    + if rank == self.level {
                        self.unknown[0] as usize
                    } else {
                        0
                    };
                good += n;
                let response = Combo {
                    kind: Kind::Single,
                    size: 1,
                    strength: power,
                    rank,
                    bomb: Bomb::None,
                };
                weighted += n as f64 * self.pass_weight(seat, &response);
            }
        }
        let p = tail(self.population, good, self.draws[seat], 1);
        if p >= 1.0 - 1e-12 || good == 0 {
            p
        } else {
            p * weighted / good as f64
        }
    }

    pub(super) fn chance(&self, seat: usize, current: &Combo, finish_only: bool) -> f64 {
        if self.counts[seat] == 0 {
            return 0.0;
        }
        let key = (
            seat,
            current.kind as u8,
            current.size,
            current.strength,
            current.bomb as u8,
            finish_only,
        );
        if let Some(&p) = self.responses.borrow().get(&key) {
            return p;
        }
        let singles = current.kind == Kind::Single && (!finish_only || self.counts[seat] == 1);
        let single = if singles {
            self.single_chance(seat, current)
        } else {
            0.0
        };
        if self.counts[seat] == 1 {
            return single;
        }
        let patterns = patterns(self.level);
        if self.holdings.borrow().is_none() {
            let holdings = patterns
                .iter()
                .map(|p| std::array::from_fn(|s| self.holding(s, p)))
                .collect();
            *self.holdings.borrow_mut() = Some(holdings);
        }
        let holdings = self.holdings.borrow();
        let mut groups = BTreeMap::<(u8, i32), f64>::new();
        for (pattern, chances) in patterns.iter().zip(holdings.as_ref().unwrap()) {
            let response = &pattern.combo;
            if (singles && response.bomb == Bomb::None)
                || !response.beats(Some(current))
                || (finish_only && response.size != self.counts[seat])
            {
                continue;
            }
            let p = chances[seat];
            if p == 0.0 {
                continue;
            }
            let p = if p >= 1.0 - 1e-12 {
                1.0
            } else {
                p * self.pass_weight(seat, response)
            };
            // Wild allocations / pair attachments overlap. Group by strength
            // instead of pretending every template is an independent response.
            groups
                .entry((response.bomb as u8, response.strength))
                .and_modify(|v| *v = v.max(p))
                .or_insert(p);
        }
        let p = 1.0 - (1.0 - single) * groups.values().fold(1.0, |no, p| no * (1.0 - p));
        self.responses.borrow_mut().insert(key, p);
        p
    }

    pub(super) fn opponents(&self, current: &Combo, finish_only: bool) -> f64 {
        1.0 - (0..4)
            .filter(|s| s % 2 != self.seat % 2)
            .fold(1.0, |no, s| {
                no * (1.0 - self.chance(s, current, finish_only))
            })
    }

    pub(super) fn known_finish(&self, seat: usize) -> Option<Combo> {
        if self.counts[seat] == 0 || self.known_cards[seat].len() != self.counts[seat] {
            return None;
        }
        let mut cards = self.known_cards[seat].clone();
        cards.sort_by_key(|c| (c.power(self.level), c.rank, c.uid));
        classify(&cards, self.level)
    }

    #[cfg(test)]
    fn unseen_rank(&self, rank: u8) -> usize {
        let count = self.unknown[rank as usize] as usize
            + self
                .known
                .iter()
                .map(|k| k[rank as usize] as usize)
                .sum::<usize>();
        count
            + if rank == self.level {
                self.unknown[0] as usize + self.known.iter().map(|k| k[0] as usize).sum::<usize>()
            } else {
                0
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Played;

    fn cards(specs: &[(u8, Suit)]) -> Vec<Card> {
        let mut pool = deck();
        specs
            .iter()
            .map(|&(rank, suit)| {
                let i = pool
                    .iter()
                    .position(|c| c.rank == rank && c.suit == suit)
                    .unwrap();
                pool.remove(i)
            })
            .collect()
    }

    fn public_play(c: Card) -> PublicAction {
        PublicAction::Play(Played {
            seat: 3,
            cards: vec![c],
            combo: classify(&[c], 2).unwrap(),
        })
    }

    fn exhausted_except(remaining: &[Card]) -> Vec<PublicAction> {
        deck()
            .into_iter()
            .filter(|c| !remaining.contains(c))
            .map(public_play)
            .collect()
    }

    #[test]
    fn counts_physical_copies_wildcards_and_jokers_once() {
        use Suit::*;
        let own = cards(&[(7, Hearts), (15, Joker)]);
        let played = cards(&[(7, Hearts), (7, Hearts), (7, Spades), (16, Joker)]);
        let ctx = Context {
            seat: 0,
            counts: [2, 27, 27, 27],
            trick: None,
        };
        let history: Vec<_> = played[1..]
            .iter()
            .copied()
            .chain([played[2]])
            .map(public_play)
            .collect();
        let info = Knowledge::new(&own, 7, &ctx, &history);
        assert_eq!(info.unseen_rank(7), 5);
        assert_eq!(info.unseen_rank(15), 1);
        assert_eq!(info.unseen_rank(16), 1);
        assert_eq!(info.unknown[0], 0);
    }

    #[test]
    fn pass_inference_is_soft_ignores_partner_passes_and_yields_to_facts() {
        use Suit::*;
        let own = cards(&[(3, Spades)]);
        let pair = classify(&cards(&[(9, Spades), (9, Diamonds)]), 2).unwrap();
        let high = classify(&cards(&[(14, Spades), (14, Diamonds)]), 2).unwrap();
        let ctx = Context {
            seat: 0,
            counts: [1, 27, 27, 27],
            trick: None,
        };
        let enemy_pass = PublicAction::Pass {
            seat: 1,
            winner: 0,
            combo: pair.clone(),
            remaining: 3,
        };
        let info = Knowledge::new(&own, 2, &ctx, std::slice::from_ref(&enemy_pass));
        assert!((0.4..1.0).contains(&info.pass_weight(1, &high)));
        assert!(info.chance(1, &pair, false) > 0.0);
        let before = Knowledge::new(&own, 2, &ctx, &[]);
        assert!(info.chance(1, &pair, false) < before.chance(1, &pair, false));
        let partner_pass = PublicAction::Pass {
            seat: 1,
            winner: 3,
            combo: pair.clone(),
            remaining: 3,
        };
        let info = Knowledge::new(&own, 2, &ctx, &[partner_pass]);
        assert_eq!(info.pass_weight(1, &high), 1.0);
        let bigs = cards(&[(16, Joker), (16, Joker)]);
        let mut history = vec![enemy_pass];
        for c in bigs {
            history.push(PublicAction::Exchange {
                from: 0,
                to: 1,
                given: c,
                returned: own[0],
            });
        }
        let info = Knowledge::new(&own, 2, &ctx, &history);
        assert_eq!(info.chance(1, &high, false), 1.0);
        history.push(PublicAction::Play(Played {
            seat: 1,
            cards: cards(&[(14, Spades), (14, Diamonds)]),
            combo: high.clone(),
        }));
        let info = Knowledge::new(&own, 2, &ctx, &history);
        assert_eq!(info.pass_weight(1, &high), 1.0);
    }

    #[test]
    fn exhausted_high_cards_prove_a_low_control_card_is_safe() {
        use Suit::*;
        let own = cards(&[(3, Spades), (9, Spades)]);
        let mut remaining = own.clone();
        remaining.extend(cards(&[(9, Hearts), (4, Diamonds)]));
        let ctx = Context {
            seat: 0,
            counts: [2, 1, 0, 1],
            trick: None,
        };
        let history = exhausted_except(&remaining);
        let info = Knowledge::new(&own, 2, &ctx, &history);
        assert_eq!(info.opponents(&classify(&[own[1]], 2).unwrap(), false), 0.0);
        assert!(info.opponents(&classify(&[own[0]], 2).unwrap(), false) > 0.0);
        let m = crate::bot::choose_move_with_history(&own, 2, &ctx, &history).unwrap();
        assert_eq!(m.combo.rank, 9);
    }

    #[test]
    fn counting_uses_an_adequate_control_card_instead_of_wasting_a_joker() {
        use Suit::*;
        let own = cards(&[(3, Spades), (9, Spades), (16, Joker)]);
        let mut remaining = own.clone();
        remaining.extend(cards(&[(8, Hearts), (4, Diamonds)]));
        let ctx = Context {
            seat: 0,
            counts: [3, 1, 0, 1],
            trick: None,
        };
        assert_eq!(
            crate::bot::choose_move(&own, 2, &ctx).unwrap().combo.rank,
            16
        );
        let m = crate::bot::choose_move_with_history(&own, 2, &ctx, &exhausted_except(&remaining))
            .unwrap();
        assert_eq!(m.combo.rank, 9);
    }

    #[test]
    fn wildcards_cannot_supply_a_missing_joker_in_a_response() {
        use Suit::*;
        let own = cards(&[(3, Spades), (3, Hearts)]);
        let mut remaining = own.clone();
        remaining.extend(cards(&[(16, Joker), (2, Hearts), (2, Hearts)]));
        let ctx = Context {
            seat: 0,
            counts: [2, 2, 0, 1],
            trick: None,
        };
        let info = Knowledge::new(&own, 2, &ctx, &exhausted_except(&remaining));
        let trump_pair = classify(&cards(&[(2, Spades), (2, Diamonds)]), 2).unwrap();
        assert_eq!(info.chance(1, &trump_pair, false), 0.0);
    }

    #[test]
    fn response_catalogue_agrees_with_actual_small_endgame_hands() {
        use Suit::*;
        let cases = [
            vec![(16, Joker), (16, Joker)],
            vec![(15, Joker), (2, Hearts), (2, Hearts)],
            vec![
                (9, Spades),
                (9, Hearts),
                (13, Spades),
                (13, Hearts),
                (2, Hearts),
            ],
            vec![
                (3, Spades),
                (4, Spades),
                (5, Spades),
                (6, Spades),
                (2, Hearts),
            ],
            vec![
                (3, Spades),
                (3, Hearts),
                (4, Spades),
                (4, Hearts),
                (5, Spades),
                (2, Hearts),
            ],
            vec![
                (3, Spades),
                (3, Hearts),
                (3, Diamonds),
                (4, Hearts),
                (4, Spades),
                (2, Hearts),
            ],
        ];
        let probes = [
            vec![(14, Spades)],
            vec![(14, Spades), (14, Hearts)],
            vec![
                (12, Spades),
                (12, Hearts),
                (12, Diamonds),
                (8, Spades),
                (8, Diamonds),
            ],
            vec![
                (3, Spades),
                (4, Hearts),
                (5, Clubs),
                (6, Spades),
                (7, Diamonds),
            ],
            vec![
                (3, Spades),
                (4, Spades),
                (5, Spades),
                (6, Spades),
                (7, Spades),
            ],
        ];
        for case in cases {
            let mut hidden = cards(&case);
            hidden.sort_by_key(|c| (c.power(2), c.rank, c.uid));
            let own: Vec<_> = deck()
                .into_iter()
                .filter(|c| !hidden.contains(c))
                .take(2)
                .collect();
            let ctx = Context {
                seat: 0,
                counts: [2, hidden.len(), 0, 0],
                trick: None,
            };
            let remaining: Vec<_> = own.iter().chain(&hidden).copied().collect();
            let info = Knowledge::new(&own, 2, &ctx, &exhausted_except(&remaining));
            for probe in &probes {
                let current = classify(&cards(probe), 2).unwrap();
                let can = crate::bot::options(&hidden, 2)
                    .iter()
                    .any(|m| m.play.combo.beats(Some(&current)));
                assert_eq!(
                    info.chance(1, &current, false),
                    f64::from(can),
                    "{case:?} against {probe:?}"
                );
            }
        }
    }

    #[test]
    fn exhausted_endgame_predictions_match_exhaustive_classification_at_every_level() {
        use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
        use Suit::*;
        let probes = [
            vec![(14, Spades)],
            vec![(14, Spades), (14, Hearts)],
            vec![(8, Spades), (8, Hearts), (8, Diamonds)],
            vec![
                (9, Spades),
                (9, Hearts),
                (9, Diamonds),
                (8, Spades),
                (8, Diamonds),
            ],
            vec![
                (3, Spades),
                (4, Hearts),
                (5, Clubs),
                (6, Spades),
                (7, Diamonds),
            ],
            vec![
                (3, Spades),
                (3, Hearts),
                (4, Spades),
                (4, Hearts),
                (5, Spades),
                (5, Hearts),
            ],
            vec![
                (3, Spades),
                (3, Hearts),
                (3, Diamonds),
                (4, Spades),
                (4, Hearts),
                (4, Diamonds),
            ],
            vec![(3, Spades), (3, Hearts), (3, Diamonds), (3, Clubs)],
        ];
        let mut rng = StdRng::seed_from_u64(331700);
        for case in 0..130 {
            let level = case % 13 + 2;
            let low = if case % 2 == 0 { 2 } else { 9 };
            let mut pool: Vec<_> = deck()
                .into_iter()
                .filter(|c| (low..low + 6).contains(&c.rank) || c.is_joker())
                .filter(|c| !c.is_wild(level))
                .collect();
            pool.shuffle(&mut rng);
            let mut hidden = pool[..5].to_vec();
            hidden.extend(
                deck()
                    .into_iter()
                    .filter(|c| c.is_wild(level))
                    .take((case % 3) as usize),
            );
            hidden.sort_by_key(|c| (c.power(level), c.rank, c.uid));
            let own: Vec<_> = deck()
                .into_iter()
                .filter(|c| !hidden.contains(c))
                .take(2)
                .collect();
            let remaining: Vec<_> = own.iter().chain(&hidden).copied().collect();
            let ctx = Context {
                seat: 0,
                counts: [2, hidden.len(), 0, 0],
                trick: None,
            };
            let info = Knowledge::new(&own, level, &ctx, &exhausted_except(&remaining));
            let legal: Vec<_> = (1..1usize << hidden.len())
                .filter_map(|mask| {
                    let selected: Vec<_> = hidden
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask & (1 << i) != 0)
                        .map(|(_, c)| *c)
                        .collect();
                    classify(&selected, level)
                })
                .collect();
            for specs in &probes {
                let Some(current) = classify(&cards(specs), level) else {
                    continue;
                };
                let expected = legal.iter().any(|c| c.beats(Some(&current)));
                assert_eq!(
                    info.chance(1, &current, false),
                    f64::from(expected),
                    "level {level}, hidden {hidden:?}, current {current:?}"
                );
            }
        }
    }

    #[test]
    fn finite_deck_tail_respects_sampling_without_replacement() {
        assert!((tail(10, 2, 1, 1) - 0.2).abs() < 1e-12);
        assert!((tail(10, 2, 2, 1) - 17.0 / 45.0).abs() < 1e-12);
        assert_eq!(tail(4, 2, 4, 2), 1.0);
        assert_eq!(tail(4, 2, 1, 2), 0.0);
    }
}
