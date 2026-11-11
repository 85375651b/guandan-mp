use crate::cards::{deck, rank_label, Card, BIG_JOKER};
use crate::rules::{classify, Combo};
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Played {
    pub seat: usize,
    pub cards: Vec<Card>,
    pub combo: Combo,
}

/// Public facts from this deal. Kept independently of the shortened display log
/// and cleared on a new deal. No hands or unrevealed return choices belong here.
#[derive(Clone, Debug)]
pub enum PublicAction {
    Play(Played),
    Pass {
        seat: usize,
        winner: usize,
        combo: Combo,
        remaining: usize,
    },
    Exchange {
        from: usize,
        to: usize,
        given: Card,
        returned: Card,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandResult {
    pub order: Vec<usize>,
    pub actual_finish_count: usize,
    pub team: usize,
    pub upgrade: u8,
    pub level_after: u8,
    pub ace_failures: u8,
    pub reset_to_two: bool,
    pub passed_ace: bool,
}

#[derive(Clone, Debug)]
pub struct Exchange {
    pub from: usize,
    pub to: usize,
    pub tribute: Card,
    pub returned: Option<Card>,
}

#[derive(Clone, Debug)]
pub struct Tribute {
    pub double: bool,
    pub complete: bool,
    pub rebellion: bool,
    pub leader: usize,
    pub exchanges: Vec<Exchange>,
}

#[derive(Clone)]
pub struct Game {
    pub hands: [Vec<Card>; 4],
    pub team_levels: [u8; 2],
    pub ace_streak: [u8; 2],
    pub banker: usize,
    pub hand_banker: usize,
    pub level: u8,
    pub hand_id: u64,
    pub turn: Option<usize>,
    pub active: [bool; 4],
    pub finished: Vec<usize>,
    pub trick: Option<Played>,
    pub plays: [Option<Played>; 4],
    pub passed: [bool; 4],
    pub passes: usize,
    pub hand_over: bool,
    pub winner: Option<usize>,
    pub result: Option<HandResult>,
    pub tribute: Option<Tribute>,
    pub log: Vec<String>,
    pub play_count: usize,
    pub history: Vec<PublicAction>,
    rng: StdRng,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        Self::with_seed(rand::thread_rng().gen())
    }
    pub fn with_seed(seed: u64) -> Self {
        let mut g = Self {
            hands: std::array::from_fn(|_| Vec::new()),
            team_levels: [2, 2],
            ace_streak: [0, 0],
            banker: 0,
            hand_banker: 0,
            level: 2,
            hand_id: 0,
            turn: None,
            active: [true; 4],
            finished: Vec::new(),
            trick: None,
            plays: std::array::from_fn(|_| None),
            passed: [false; 4],
            passes: 0,
            hand_over: false,
            winner: None,
            result: None,
            tribute: None,
            log: Vec::new(),
            play_count: 0,
            history: Vec::new(),
            rng: StdRng::seed_from_u64(seed),
        };
        g.start_hand();
        g
    }
    pub fn note(&mut self, text: String) {
        self.log.push(text);
        if self.log.len() > 256 {
            self.log.drain(..self.log.len() - 256);
        }
    }
    pub fn start_hand(&mut self) {
        self.level = self.team_levels[self.banker];
        self.hand_banker = self.banker;
        self.hand_id += 1;
        let mut cards = deck();
        cards.shuffle(&mut self.rng);
        self.hands = std::array::from_fn(|s| cards.iter().copied().skip(s).step_by(4).collect());
        self.active = [true; 4];
        self.finished.clear();
        self.trick = None;
        self.plays = std::array::from_fn(|_| None);
        self.passed = [false; 4];
        self.passes = 0;
        self.hand_over = false;
        self.play_count = 0;
        self.history.clear();
        self.tribute = None;
        self.turn = Some(
            self.result
                .as_ref()
                .map_or_else(|| self.rng.gen_range(0..4), |r| r.order[0]),
        );
        self.sort_hands();
        self.note(format!(
            "Hand {} · playing {}",
            self.hand_id,
            rank_label(self.level)
        ));
        if let Some(result) = self.result.clone() {
            self.begin_tribute(&result.order);
        }
    }
    fn sort_hands(&mut self) {
        for h in &mut self.hands {
            h.sort_by_key(|c| (c.power(self.level), c.rank, c.uid));
        }
    }
    pub fn tribute_pending(&self) -> bool {
        self.tribute.as_ref().is_some_and(|t| !t.complete)
    }
    pub fn legal_turn(&self, seat: usize) -> bool {
        seat < 4
            && self.winner.is_none()
            && !self.hand_over
            && !self.tribute_pending()
            && self.turn == Some(seat)
            && self.active[seat]
    }
    pub fn begin_tribute(&mut self, order: &[usize]) {
        let double = order[0] % 2 == order[1] % 2;
        let donors: Vec<_> = if double {
            order[2..4].to_vec()
        } else {
            vec![order[3]]
        };
        let rebellion = donors
            .iter()
            .flat_map(|s| &self.hands[*s])
            .filter(|c| c.rank == BIG_JOKER)
            .count()
            == 2;
        let mut tribute = Tribute {
            double,
            complete: rebellion,
            rebellion,
            leader: order[0],
            exchanges: Vec::new(),
        };
        if rebellion {
            self.turn = Some(order[0]);
            self.note(format!(
                "Tribute rebellion: both big jokers are on the tribute side. Seat {} leads.",
                order[0] + 1
            ));
        } else {
            let mut offers: Vec<_> = donors
                .iter()
                .map(|s| {
                    (
                        *s,
                        *self.hands[*s]
                            .iter()
                            .filter(|c| !c.is_wild(self.level))
                            .max_by_key(|c| c.power(self.level))
                            .expect("27-card hand has a non-wild card"),
                    )
                })
                .collect();
            offers.sort_by_key(|(_, c)| std::cmp::Reverse(c.power(self.level)));
            tribute.leader = offers[0].0;
            self.turn = None;
            for (i, (from, card)) in offers.into_iter().enumerate() {
                let to = order[i];
                self.hands[from].retain(|c| c.uid != card.uid);
                self.hands[to].push(card);
                tribute.exchanges.push(Exchange {
                    from,
                    to,
                    tribute: card,
                    returned: None,
                });
                self.note(format!(
                    "Seat {} gives {} to seat {} as tribute.",
                    from + 1,
                    card,
                    to + 1
                ));
            }
            self.sort_hands();
        }
        self.tribute = Some(tribute);
    }
    pub fn return_options(&self, seat: usize) -> Vec<Card> {
        if seat >= 4
            || !self.tribute_pending()
            || !self
                .tribute
                .as_ref()
                .unwrap()
                .exchanges
                .iter()
                .any(|e| e.to == seat && e.returned.is_none())
        {
            return Vec::new();
        }
        let non_wild: Vec<_> = self.hands[seat]
            .iter()
            .copied()
            .filter(|c| !c.is_wild(self.level))
            .collect();
        // Effective strength excludes ALL level suits, even when printed 2–9.
        let low: Vec<_> = non_wild
            .iter()
            .copied()
            .filter(|c| !c.is_joker() && c.power(self.level) <= 7)
            .collect();
        if !low.is_empty() {
            return low;
        }
        let min = non_wild.iter().map(|c| c.power(self.level)).min();
        non_wild
            .into_iter()
            .filter(|c| Some(c.power(self.level)) == min)
            .collect()
    }
    pub fn return_card(&mut self, seat: usize, uid: u8) -> Result<(), String> {
        let card = self.return_options(seat).into_iter().find(|c| c.uid == uid).ok_or("Choose an eligible return card: a non-trump 2–9, or the lowest non-wild card if none is available.")?;
        let t = self.tribute.as_mut().unwrap();
        t.exchanges
            .iter_mut()
            .find(|e| e.to == seat && e.returned.is_none())
            .unwrap()
            .returned = Some(card);
        if t.exchanges.iter().all(|e| e.returned.is_some()) {
            let exchanges = t.exchanges.clone();
            let leader = t.leader;
            for e in exchanges {
                let card = e.returned.unwrap();
                self.hands[e.to].retain(|c| c.uid != card.uid);
                self.hands[e.from].push(card);
                self.history.push(PublicAction::Exchange {
                    from: e.from,
                    to: e.to,
                    given: e.tribute,
                    returned: card,
                });
                self.note(format!(
                    "Seat {} returns {} to seat {}.",
                    e.to + 1,
                    card,
                    e.from + 1
                ));
            }
            self.tribute.as_mut().unwrap().complete = true;
            self.turn = Some(leader);
            self.sort_hands();
            self.note(format!("Exchange complete. Seat {} leads.", leader + 1));
        }
        Ok(())
    }
    pub fn play(&mut self, seat: usize, uids: &[u8]) -> Result<(), String> {
        if !self.legal_turn(seat) {
            return Err("It is not your turn, or the tribute exchange is still pending.".into());
        }
        let unique: HashSet<_> = uids.iter().copied().collect();
        if unique.len() != uids.len() {
            return Err("A card cannot be selected twice.".into());
        }
        let cards: Vec<_> = self.hands[seat]
            .iter()
            .copied()
            .filter(|c| unique.contains(&c.uid))
            .collect();
        if cards.len() != uids.len() {
            return Err("A selected card is not in your hand.".into());
        }
        let combo =
            classify(&cards, self.level).ok_or("Those cards do not form a legal combination.")?;
        if !combo.beats(self.trick.as_ref().map(|p| &p.combo)) {
            return Err("That combination does not beat the current play.".into());
        }
        self.hands[seat].retain(|c| !unique.contains(&c.uid));
        self.note(format!(
            "Seat {} plays {} ({})",
            seat + 1,
            cards
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            combo.label()
        ));
        let played = Played { seat, cards, combo };
        self.history.push(PublicAction::Play(played.clone()));
        self.trick = Some(played.clone());
        self.plays[seat] = Some(played);
        self.passes = 0;
        self.passed = [false; 4];
        self.play_count += 1;
        if self.hands[seat].is_empty() {
            self.active[seat] = false;
            self.finished.push(seat);
            self.note(format!(
                "Seat {} finishes #{}.",
                seat + 1,
                self.finished.len()
            ));
            if self.finished.len() >= 3
                || (self.finished.len() == 2 && self.finished[0] % 2 == self.finished[1] % 2)
            {
                self.end_hand();
                return Ok(());
            }
        }
        self.turn = self.next_active(seat);
        Ok(())
    }
    pub fn pass(&mut self, seat: usize) -> Result<(), String> {
        if !self.legal_turn(seat) {
            return Err("It is not your turn.".into());
        }
        let winner = self
            .trick
            .as_ref()
            .ok_or("The leader must play a combination.")?
            .seat;
        self.history.push(PublicAction::Pass {
            seat,
            winner,
            combo: self.trick.as_ref().unwrap().combo.clone(),
            remaining: self.hands[seat].len(),
        });
        self.passes += 1;
        self.passed[seat] = true;
        self.note(format!("Seat {} passes.", seat + 1));
        let needed = self.active.iter().filter(|a| **a).count() - usize::from(self.active[winner]);
        if self.passes >= needed {
            self.note(format!("Seat {} wins the trick.", winner + 1));
            self.trick = None;
            self.passes = 0;
            self.passed = [false; 4];
            self.plays = std::array::from_fn(|_| None);
            self.turn = if self.active[winner] {
                Some(winner)
            } else if self.active[(winner + 2) % 4] {
                Some((winner + 2) % 4)
            } else {
                self.next_active(winner)
            };
        } else {
            self.turn = self.next_active(seat);
        }
        Ok(())
    }
    fn next_active(&self, from: usize) -> Option<usize> {
        (1..=4).map(|n| (from + n) % 4).find(|s| self.active[*s])
    }
    pub fn end_hand(&mut self) {
        if self.hand_over {
            return;
        }
        self.hand_over = true;
        let actual_finish_count = self.finished.len();
        for s in 0..4 {
            if !self.finished.contains(&s) {
                self.finished.push(s);
            }
        }
        let team = self.finished[0] % 2;
        let upgrade = if self.finished[1] % 2 == team {
            3
        } else if self.finished[2] % 2 == team {
            2
        } else {
            1
        };
        let own_ace = self.hand_banker == team && self.level == 14;
        let ace_failures = if own_ace && upgrade == 1 {
            self.ace_streak[team] + 1
        } else {
            0
        };
        self.ace_streak = [0, 0];
        let reset_to_two = ace_failures == 3;
        let passed_ace = own_ace && upgrade >= 2;
        if ace_failures > 0 {
            self.team_levels[team] = if reset_to_two { 2 } else { 14 };
            self.ace_streak[team] = if reset_to_two { 0 } else { ace_failures };
        } else {
            self.team_levels[team] = (self.team_levels[team] + upgrade).min(14);
        }
        self.banker = team;
        self.result = Some(HandResult {
            order: self.finished.clone(),
            actual_finish_count,
            team,
            upgrade,
            level_after: self.team_levels[team],
            ace_failures,
            reset_to_two,
            passed_ace,
        });
        self.note(format!(
            "Team {} wins the hand. {}",
            if team == 0 { "A" } else { "B" },
            if reset_to_two {
                "Three consecutive 1–4 Ace wins: back to 2.".into()
            } else if ace_failures > 0 {
                format!("Stays at Ace ({ace_failures}/3 consecutive 1–4 wins).")
            } else {
                format!("Level {}.", rank_label(self.team_levels[team]))
            }
        ));
        if passed_ace {
            self.winner = Some(team);
            self.note(format!(
                "Team {} passes Ace and wins the game!",
                if team == 0 { "A" } else { "B" }
            ));
        }
    }
}
