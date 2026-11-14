use serde::{Deserialize, Serialize};
use std::fmt;

pub const SMALL_JOKER: u8 = 15;
pub const BIG_JOKER: u8 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
    Joker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Card {
    pub uid: u8,
    pub rank: u8,
    pub suit: Suit,
}

impl Card {
    pub fn is_joker(self) -> bool {
        self.rank >= SMALL_JOKER
    }
    pub fn is_wild(self, level: u8) -> bool {
        self.suit == Suit::Hearts && self.rank == level
    }
    pub fn power(self, level: u8) -> i32 {
        match self.rank {
            BIG_JOKER => 15,
            SMALL_JOKER => 14,
            r if r == level => 13,
            r => i32::from(r) - 2,
        }
    }
    pub fn rank_label(self) -> &'static str {
        rank_label(self.rank)
    }
    pub fn suit_label(self) -> &'static str {
        match self.suit {
            Suit::Spades => "♠",
            Suit::Hearts => "♥",
            Suit::Diamonds => "♦",
            Suit::Clubs => "♣",
            Suit::Joker => "",
        }
    }
    pub fn is_red(self) -> bool {
        matches!(self.suit, Suit::Hearts | Suit::Diamonds) || self.rank == BIG_JOKER
    }
}

pub fn rank_label(rank: u8) -> &'static str {
    match rank {
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        10 => "10",
        11 => "J",
        12 => "Q",
        13 => "K",
        14 => "A",
        15 => "Small joker",
        16 => "Big joker",
        _ => "?",
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_joker() {
            write!(
                f,
                "{} {}",
                self.rank_label(),
                if self.rank == BIG_JOKER {
                    "大王"
                } else {
                    "小王"
                }
            )
        } else {
            write!(f, "{}{}", self.rank_label(), self.suit_label())
        }
    }
}

pub fn deck() -> Vec<Card> {
    let mut cards = Vec::with_capacity(108);
    for _ in 0..2 {
        for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
            for rank in 2..=14 {
                cards.push(Card {
                    uid: cards.len() as u8,
                    rank,
                    suit,
                });
            }
        }
        for rank in [SMALL_JOKER, BIG_JOKER] {
            cards.push(Card {
                uid: cards.len() as u8,
                rank,
                suit: Suit::Joker,
            });
        }
    }
    cards
}
