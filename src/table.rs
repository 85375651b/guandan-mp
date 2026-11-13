use crate::bot::{choose_move_with_history, Context};
use crate::cards::Card;
use crate::game::{Game, HandResult, Played};
use serde::{Deserialize, Serialize};

use std::time::{Duration, Instant};

pub const PROTOCOL_VERSION: u16 = 2;
pub type Peer = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Join { version: u16, name: String },
    Play { hand_id: u64, cards: Vec<u8> },
    Pass { hand_id: u64 },
    Return { hand_id: u64, card: u8 },
    Hint { hand_id: u64, request_id: u64 },
    NewGame,
    Ping,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::deck;
    fn join(t: &mut Table, peer: u64, now: Instant) {
        t.apply(
            peer,
            Command::Join {
                version: PROTOCOL_VERSION,
                name: format!("Player {peer}"),
            },
            now,
        )
        .unwrap();
    }
    fn tribute(t: &mut Table) {
        let d = deck();
        t.game.hands = [
            d.iter().copied().filter(|c| c.rank == 16).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ];
        for c in d.into_iter().filter(|c| c.rank != 16) {
            t.game
                .hands
                .iter_mut()
                .find(|h| h.len() < 27)
                .unwrap()
                .push(c);
        }
        t.game.begin_tribute(&[0, 2, 1, 3]);
    }
    #[test]
    fn unattended_table_never_plays() {
        let mut t = Table::default();
        let now = Instant::now();
        assert!(!t.tick(now + Duration::from_secs(500)));
        assert_eq!(t.game.hands.each_ref().map(Vec::len), [27; 4]);
        assert_eq!(t.phase(), Phase::Waiting);
        join(&mut t, 1, now);
        assert_eq!(t.game.hands.each_ref().map(Vec::len), [27; 4]);
    }
    #[test]
    fn human_leader_is_never_automated() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        t.game.turn = Some(0);
        t.schedule(now);
        assert!(!t.tick(now + Duration::from_secs(10)));
        assert_eq!(t.game.hands[0].len(), 27);
    }
    #[test]
    fn seat_takeover_cancels_pending_bot_move() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        t.game.turn = Some(1);
        t.schedule(now);
        join(&mut t, 2, now + Duration::from_millis(400));
        assert!(!t.tick(now + Duration::from_secs(10)));
        assert_eq!(t.game.hands[1].len(), 27);
    }
    #[test]
    fn joining_occupied_table_preserves_game() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        t.game.team_levels[0] = 7;
        let before = t.game.hands.clone();
        join(&mut t, 2, now);
        assert_eq!(t.game.hands, before);
        assert_eq!(t.game.team_levels[0], 7);
    }
    #[test]
    fn empty_table_stops_then_resets_on_next_join() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        t.game.team_levels[0] = 7;
        t.game.turn = Some(1);
        t.schedule(now);
        t.disconnect(1, now);
        let before = t.game.hands.clone();
        assert!(!t.tick(now + Duration::from_secs(30)));
        assert_eq!(t.game.hands, before);
        join(&mut t, 2, now);
        assert_eq!(t.game.team_levels, [2, 2]);
        assert_eq!(t.game.hands.each_ref().map(Vec::len), [27; 4]);
    }
    #[test]
    fn disconnection_hands_control_to_a_bot_if_human_remains() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        join(&mut t, 2, now);
        t.game.turn = Some(0);
        t.disconnect(1, now);
        assert!(t.tick(now + Duration::from_secs(1)));
        assert!(t.game.hands[0].len() < 27);
        assert_eq!(t.humans(), 1);
    }
    #[test]
    fn snapshots_do_not_contain_other_hands_or_unrevealed_returns() {
        let mut t = Table::default();
        let now = Instant::now();
        for p in 1..=4 {
            join(&mut t, p, now);
        }
        tribute(&mut t);
        let own = t.view(1);
        assert_eq!(own.your_hand.len(), 28);
        assert!(t.view(2).return_options.is_empty());
        assert!(t.view(999).your_hand.is_empty());
        let card = own.return_options[0];
        let id = t.game.hand_id;
        t.apply(1, Command::Return { hand_id: id, card }, now)
            .unwrap();
        let view = t.view(2);
        assert!(view.tribute.as_ref().unwrap().exchanges[0].ready);
        assert!(view
            .tribute
            .unwrap()
            .exchanges
            .iter()
            .all(|e| e.returned.is_none()));
        assert!(!view.log.iter().any(|s| s.contains(" returns ")));
        assert!(t
            .apply(2, Command::Return { hand_id: id, card }, now)
            .is_err());
        let card = t.view(3).return_options[0];
        t.apply(3, Command::Return { hand_id: id, card }, now)
            .unwrap();
        assert_eq!(t.game.hands.each_ref().map(Vec::len), [27; 4]);
        assert!(t
            .view(2)
            .tribute
            .unwrap()
            .exchanges
            .iter()
            .all(|e| e.returned.is_some()));
    }
    #[test]
    fn tribute_takeover_disconnect_and_empty_table_are_safe() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        join(&mut t, 2, now);
        tribute(&mut t);
        t.schedule(now);
        join(&mut t, 3, now + Duration::from_millis(400));
        assert!(!t.tick(now + Duration::from_secs(2)));
        assert!(!t.view(3).return_options.is_empty());
        t.disconnect(1, now);
        assert!(t.tick(now + Duration::from_secs(1)));
        assert!(t
            .game
            .tribute
            .as_ref()
            .unwrap()
            .exchanges
            .iter()
            .find(|e| e.to == 0)
            .unwrap()
            .returned
            .is_some());
        t.disconnect(2, now);
        t.disconnect(3, now);
        assert!(!t.tick(now + Duration::from_secs(30)));
        join(&mut t, 4, now);
        assert!(t.game.tribute.is_none());
        assert_eq!(t.game.hands.each_ref().map(Vec::len), [27; 4]);
    }
    #[test]
    fn stale_hand_actions_are_rejected() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        t.game.turn = Some(0);
        let before = t.game.hands.clone();
        assert!(t
            .apply(
                1,
                Command::Play {
                    hand_id: 0,
                    cards: vec![t.game.hands[0][0].uid]
                },
                now
            )
            .is_err());
        assert_eq!(t.game.hands, before);
    }
    #[test]
    fn protocol_mismatch_and_full_table_are_explicit_errors() {
        let mut t = Table::default();
        let now = Instant::now();
        assert!(t
            .apply(
                1,
                Command::Join {
                    version: 99,
                    name: "Wrong version".into()
                },
                now
            )
            .is_err());
        for p in 1..=4 {
            join(&mut t, p, now);
        }
        assert!(t
            .apply(
                5,
                Command::Join {
                    version: PROTOCOL_VERSION,
                    name: "Fifth".into()
                },
                now
            )
            .is_err());
        assert_eq!(t.humans(), 4);
    }
    #[test]
    fn level_seven_trumps_cannot_be_returned_by_humans_or_bots() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        join(&mut t, 2, now);
        t.game.team_levels[0] = 7;
        t.game.start_hand();
        tribute(&mut t);
        t.schedule(now);
        let view = t.view(1);
        for c in view.your_hand.iter().filter(|c| c.rank == 7) {
            assert!(!view.return_options.contains(&c.uid));
            assert!(t
                .apply(
                    1,
                    Command::Return {
                        hand_id: view.hand_id,
                        card: c.uid
                    },
                    now
                )
                .is_err());
        }
        assert!(t.tick(now + Duration::from_secs(1)));
        assert_ne!(
            t.game
                .tribute
                .as_ref()
                .unwrap()
                .exchanges
                .iter()
                .find(|e| e.to == 2)
                .unwrap()
                .returned
                .unwrap()
                .rank,
            7
        );
    }

    #[test]
    fn live_bot_uses_hand_planning_instead_of_the_old_largest_play_policy() {
        let mut t = Table::default();
        let now = Instant::now();
        join(&mut t, 1, now);
        let deck = deck();
        t.game.hands[1] = [(3, 3), (4, 3), (5, 2), (6, 2)]
            .into_iter()
            .flat_map(|(rank, count)| {
                deck.iter()
                    .filter(move |c| c.rank == rank)
                    .take(count)
                    .copied()
            })
            .collect();
        t.game.turn = Some(1);
        t.schedule(now);
        assert!(t.tick(now + Duration::from_secs(1)));
        assert_eq!(
            t.game.trick.as_ref().unwrap().combo.kind,
            crate::rules::Kind::FullHouse
        );
        assert_eq!(t.game.hands[1].len(), 5);
    }

    #[test]
    fn help_uses_level_three_without_changing_the_table_or_reading_hidden_cards() {
        let mut t = Table::default();
        join(&mut t, 1, Instant::now());
        let deck = deck();
        t.game.hands[0] = deck
            .iter()
            .copied()
            .filter(|c| {
                c.uid < 54
                    && (c.rank == 3
                        || matches!(
                            (c.rank, c.suit),
                            (4, crate::cards::Suit::Spades)
                                | (5, crate::cards::Suit::Diamonds)
                                | (7, crate::cards::Suit::Clubs)
                                | (9, crate::cards::Suit::Hearts)
                        ))
            })
            .collect();
        let played = Played {
            seat: 1,
            cards: deck
                .iter()
                .copied()
                .filter(|c| c.rank == 14)
                .take(2)
                .collect(),
            combo: crate::rules::classify(
                &deck
                    .iter()
                    .copied()
                    .filter(|c| c.rank == 14)
                    .take(2)
                    .collect::<Vec<_>>(),
                2,
            )
            .unwrap(),
        };
        t.game.hands[1].truncate(1);
        t.game.hands[2].truncate(20);
        t.game.hands[3].truncate(20);
        t.game.turn = Some(0);
        t.game.trick = Some(played.clone());
        t.game.history = vec![crate::game::PublicAction::Play(played)];
        // This position distinguishes the level 3 policy from level 2: stop
        // the one-card opponent from regaining the lead after their pair wins.
        assert!(crate::bot::choose_move(
            &t.game.hands[0],
            2,
            &Context {
                seat: 0,
                counts: [8, 1, 20, 20],
                trick: t.game.trick.as_ref().map(|p| (p.seat, &p.combo)),
            }
        )
        .is_none());
        let before = serde_json::to_value(t.view(1)).unwrap();
        let deadline = t.deadline;
        let hint = t.hint(1, t.game.hand_id, 42);
        assert_eq!(hint.request_id, 42);
        assert_eq!(hint.revision, t.revision);
        let uids = hint.cards.unwrap();
        assert_eq!(uids.len(), 4);
        assert!(uids
            .iter()
            .all(|uid| t.game.hands[0].iter().any(|c| c.uid == *uid && c.rank == 3)));
        assert_eq!(serde_json::to_value(t.view(1)).unwrap(), before);
        assert_eq!(t.deadline, deadline);
        assert_eq!(t.game.history.len(), 1);
        // Different hidden cards, identical public sizes and history.
        let other = t.game.hands[3][0];
        t.game.hands[3][0] = t.game.hands[1][0];
        t.game.hands[1][0] = other;
        assert_eq!(t.hint(1, t.game.hand_id, 43).cards.unwrap(), uids);
        t.game.play(0, &uids).unwrap();
    }

    #[test]
    fn help_can_recommend_a_pass_and_rejects_unavailable_turns() {
        let mut t = Table::default();
        join(&mut t, 1, Instant::now());
        assert!(t.hint(99, t.game.hand_id, 1).cards.is_err());
        assert!(t.hint(1, t.game.hand_id + 1, 1).cards.is_err());
        t.game.turn = Some(1);
        assert!(t.hint(1, t.game.hand_id, 1).cards.is_err());
        t.game.turn = Some(0);
        t.game.hands[0] = deck()
            .into_iter()
            .filter(|c| {
                (c.rank == 9 && c.suit == crate::cards::Suit::Spades && c.uid < 54)
                    || (c.rank == 14 && c.suit == crate::cards::Suit::Diamonds && c.uid < 54)
            })
            .collect();
        t.game.hands[2].clear();
        let card = deck().into_iter().find(|c| c.rank == 4).unwrap();
        t.game.trick = Some(Played {
            seat: 2,
            cards: vec![card],
            combo: crate::rules::classify(&[card], 2).unwrap(),
        });
        let before = serde_json::to_value(t.view(1)).unwrap();
        assert!(t.hint(1, t.game.hand_id, 1).cards.unwrap().is_empty());
        assert_eq!(serde_json::to_value(t.view(1)).unwrap(), before);
        t.game.hand_over = true;
        assert!(t.hint(1, t.game.hand_id, 1).cards.is_err());
        t.game.hand_over = false;
        tribute(&mut t);
        assert!(t.hint(1, t.game.hand_id, 1).cards.is_err());
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Message {
    State(Box<View>),
    Hint(Hint),
    Error(String),
    Pong,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hint {
    pub request_id: u64,
    pub hand_id: u64,
    pub revision: u64,
    /// An empty selection recommends passing. Errors are correlated to the request.
    pub cards: Result<Vec<u8>, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Waiting,
    Playing,
    Tribute,
    HandOver,
    GameOver,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeatView {
    pub name: String,
    pub bot: bool,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExchangeView {
    pub from: usize,
    pub to: usize,
    pub tribute: Card,
    pub ready: bool,
    pub returned: Option<Card>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TributeView {
    pub double: bool,
    pub complete: bool,
    pub rebellion: bool,
    pub leader: usize,
    pub exchanges: Vec<ExchangeView>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub revision: u64,
    pub hand_id: u64,
    pub phase: Phase,
    pub your_seat: Option<usize>,
    pub your_hand: Vec<Card>,
    pub seats: [SeatView; 4],
    pub level: u8,
    pub team_levels: [u8; 2],
    pub ace_streak: [u8; 2],
    pub turn: Option<usize>,
    pub trick: Option<Played>,
    pub plays: [Option<Played>; 4],
    pub passed: [bool; 4],
    pub tribute: Option<TributeView>,
    pub return_options: Vec<u8>,
    pub show_tribute: bool,
    pub result: Option<HandResult>,
    pub winner: Option<usize>,
    pub log: Vec<String>,
}

#[derive(Clone)]
struct Seat {
    peer: Option<Peer>,
    name: String,
}

pub struct Table {
    pub game: Game,
    seats: [Seat; 4],
    pub revision: u64,
    deadline: Option<Instant>,
    bot_delay: Duration,
    hand_delay: Duration,
}

impl Default for Table {
    fn default() -> Self {
        Self::new(Duration::from_millis(900), Duration::from_secs(4))
    }
}

impl Table {
    pub fn new(bot_delay: Duration, hand_delay: Duration) -> Self {
        Self {
            game: Game::new(),
            seats: std::array::from_fn(|s| Seat {
                peer: None,
                name: format!("Bot {}", s + 1),
            }),
            revision: 0,
            deadline: None,
            bot_delay,
            hand_delay,
        }
    }
    pub fn humans(&self) -> usize {
        self.seats.iter().filter(|s| s.peer.is_some()).count()
    }
    pub fn seat_of(&self, peer: Peer) -> Option<usize> {
        self.seats.iter().position(|s| s.peer == Some(peer))
    }
    pub fn phase(&self) -> Phase {
        if self.humans() == 0 {
            Phase::Waiting
        } else if self.game.winner.is_some() {
            Phase::GameOver
        } else if self.game.hand_over {
            Phase::HandOver
        } else if self.game.tribute_pending() {
            Phase::Tribute
        } else {
            Phase::Playing
        }
    }
    fn schedule(&mut self, now: Instant) {
        self.deadline = None;
        if self.humans() == 0 || self.game.winner.is_some() {
            return;
        }
        if self.game.hand_over {
            self.deadline = Some(now + self.hand_delay);
        } else if self.game.tribute_pending() {
            if self
                .game
                .tribute
                .as_ref()
                .unwrap()
                .exchanges
                .iter()
                .any(|e| e.returned.is_none() && self.seats[e.to].peer.is_none())
            {
                self.deadline = Some(now + self.bot_delay);
            }
        } else if self.game.turn.is_some_and(|s| self.seats[s].peer.is_none()) {
            self.deadline = Some(now + self.bot_delay);
        }
    }

    pub fn apply(&mut self, peer: Peer, command: Command, now: Instant) -> Result<(), String> {
        if matches!(command, Command::Ping) {
            return Ok(());
        }
        if let Command::Join { version, name } = command {
            if version != PROTOCOL_VERSION {
                return Err(
                    "Different game version. Host and guests must run the same release.".into(),
                );
            }
            if self.seat_of(peer).is_some() {
                return Ok(());
            }
            let seat = self
                .seats
                .iter()
                .position(|s| s.peer.is_none())
                .ok_or("All four seats are occupied.")?;
            if self.humans() == 0 {
                self.game = Game::new();
            }
            let name: String = name
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(20)
                .collect();
            self.seats[seat] = Seat {
                peer: Some(peer),
                name: if name.is_empty() {
                    "Guest".into()
                } else {
                    name
                },
            };
            self.game.note(format!(
                "{} sits in seat {}.",
                self.seats[seat].name,
                seat + 1
            ));
        } else {
            let seat = self.seat_of(peer).ok_or("Join a seat before playing.")?;
            match command {
                Command::Play { hand_id, cards } => {
                    self.check_hand(hand_id)?;
                    self.game.play(seat, &cards)?;
                }
                Command::Pass { hand_id } => {
                    self.check_hand(hand_id)?;
                    self.game.pass(seat)?;
                }
                Command::Return { hand_id, card } => {
                    self.check_hand(hand_id)?;
                    self.game.return_card(seat, card)?;
                }
                Command::NewGame => {
                    if self.game.winner.is_none() {
                        return Err("The current game has not finished.".into());
                    }
                    self.game = Game::new();
                }
                Command::Join { .. } => unreachable!(),
                Command::Ping => unreachable!(),
                Command::Hint { .. } => {
                    return Err("Help requests do not change the table.".into());
                }
            }
        }
        self.revision += 1;
        self.schedule(now);
        Ok(())
    }
    fn check_hand(&self, id: u64) -> Result<(), String> {
        if id == self.game.hand_id {
            Ok(())
        } else {
            Err("That action belongs to a previous hand. Select your cards again.".into())
        }
    }
    /// Run the same level 3 policy as a bot, using only this player's cards,
    /// public hand sizes, and the deal's public action history.
    pub fn hint(&self, peer: Peer, hand_id: u64, request_id: u64) -> Hint {
        let cards = (|| {
            self.check_hand(hand_id)?;
            let seat = self
                .seat_of(peer)
                .ok_or("Join a seat before asking for help.")?;
            if !self.game.legal_turn(seat) {
                return Err("Help is available during your turn after tribute.".into());
            }
            Ok(self
                .choice(seat)
                .map_or_else(Vec::new, |m| m.cards.iter().map(|c| c.uid).collect()))
        })();
        Hint {
            request_id,
            hand_id,
            revision: self.revision,
            cards,
        }
    }
    fn choice(&self, seat: usize) -> Option<crate::rules::Move> {
        choose_move_with_history(
            &self.game.hands[seat],
            self.game.level,
            &Context {
                seat,
                counts: self.game.hands.each_ref().map(Vec::len),
                trick: self.game.trick.as_ref().map(|p| (p.seat, &p.combo)),
            },
            &self.game.history,
        )
    }
    pub fn disconnect(&mut self, peer: Peer, now: Instant) -> bool {
        if let Some(seat) = self.seat_of(peer) {
            self.game.note(format!(
                "{} left. A bot takes seat {}.",
                self.seats[seat].name,
                seat + 1
            ));
            self.seats[seat] = Seat {
                peer: None,
                name: format!("Bot {}", seat + 1),
            };
            self.revision += 1;
            self.schedule(now);
            true
        } else {
            false
        }
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.humans() == 0 || self.deadline.is_none_or(|t| now < t) {
            return false;
        }
        if self.game.hand_over {
            self.game.start_hand();
        } else if self.game.tribute_pending() {
            let seat = self
                .game
                .tribute
                .as_ref()
                .unwrap()
                .exchanges
                .iter()
                .find(|e| e.returned.is_none() && self.seats[e.to].peer.is_none())
                .map(|e| e.to);
            if let Some(seat) = seat {
                if let Some(card) = self.game.return_options(seat).first() {
                    let _ = self.game.return_card(seat, card.uid);
                }
            }
        } else if let Some(seat) = self.game.turn {
            if self.seats[seat].peer.is_none() {
                let choice = self.choice(seat);
                if let Some(m) = choice {
                    let _ = self
                        .game
                        .play(seat, &m.cards.iter().map(|c| c.uid).collect::<Vec<_>>());
                } else {
                    let _ = self.game.pass(seat);
                }
            }
        }
        self.revision += 1;
        self.schedule(now);
        true
    }
    pub fn view(&self, peer: Peer) -> View {
        let seat = self.seat_of(peer);
        View {
            revision: self.revision,
            hand_id: self.game.hand_id,
            phase: self.phase(),
            your_seat: seat,
            your_hand: seat.map_or_else(Vec::new, |s| self.game.hands[s].clone()),
            seats: std::array::from_fn(|s| SeatView {
                name: self.seats[s].name.clone(),
                bot: self.seats[s].peer.is_none(),
                count: self.game.hands[s].len(),
            }),
            level: self.game.level,
            team_levels: self.game.team_levels,
            ace_streak: self.game.ace_streak,
            turn: self.game.turn,
            trick: self.game.trick.clone(),
            plays: self.game.plays.clone(),
            passed: self.game.passed,
            tribute: self.game.tribute.as_ref().map(|t| TributeView {
                double: t.double,
                complete: t.complete,
                rebellion: t.rebellion,
                leader: t.leader,
                exchanges: t
                    .exchanges
                    .iter()
                    .map(|e| ExchangeView {
                        from: e.from,
                        to: e.to,
                        tribute: e.tribute,
                        ready: e.returned.is_some(),
                        returned: if t.complete { e.returned } else { None },
                    })
                    .collect(),
            }),
            return_options: seat.map_or_else(Vec::new, |s| {
                self.game.return_options(s).iter().map(|c| c.uid).collect()
            }),
            show_tribute: self.game.tribute.is_some()
                && (self.game.tribute_pending() || self.game.play_count == 0),
            result: self.game.result.clone(),
            winner: self.game.winner,
            log: self.game.log.iter().rev().take(80).rev().cloned().collect(),
        }
    }
}
