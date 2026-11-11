use guandan_desktop::{
    cards::{deck, Card, Suit},
    game::Game,
    rules::{choose_move, classify, Bomb, Combo, Kind},
};
use serde::Deserialize;
use std::collections::HashSet;

fn cards(specs: &[&str]) -> Vec<Card> {
    let mut deck = deck();
    specs
        .iter()
        .map(|s| {
            let i = deck
                .iter()
                .position(|c| match *s {
                    "small" => c.rank == 15,
                    "big" => c.rank == 16,
                    _ => {
                        let suit = match s.as_bytes()[s.len() - 1] {
                            b'S' => Suit::Spades,
                            b'H' => Suit::Hearts,
                            b'D' => Suit::Diamonds,
                            _ => Suit::Clubs,
                        };
                        c.suit == suit && c.rank_label() == &s[..s.len() - 1]
                    }
                })
                .unwrap();
            deck.remove(i)
        })
        .collect()
}
fn at(level: u8, team: usize) -> Game {
    let mut g = Game::with_seed(42);
    g.team_levels[team] = level;
    g.banker = team;
    g.start_hand();
    g
}
fn finish(g: &mut Game, order: &[usize]) {
    g.finished = order.to_vec();
    g.end_hand();
}
fn resolve(g: &mut Game) {
    while g.tribute_pending() {
        let s = g
            .tribute
            .as_ref()
            .unwrap()
            .exchanges
            .iter()
            .find(|e| e.returned.is_none())
            .unwrap()
            .to;
        let card = g.return_options(s)[0];
        g.return_card(s, card.uid).unwrap();
    }
}

#[test]
fn legacy_combination_parity() {
    #[derive(Deserialize)]
    struct Case {
        cards: Vec<Card>,
        level: u8,
        expected: Option<Combo>,
    }
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/combos.json")).unwrap();
    assert!(cases.len() >= 60);
    for (i, c) in cases.into_iter().enumerate() {
        assert_eq!(
            classify(&c.cards, c.level),
            c.expected,
            "fixture {i}: {:?}",
            c.cards
        );
    }
}
#[test]
fn deck_is_two_complete_decks() {
    let d = deck();
    assert_eq!(d.len(), 108);
    assert_eq!(d.iter().map(|c| c.uid).collect::<HashSet<_>>().len(), 108);
    assert_eq!(d.iter().filter(|c| c.rank == 16).count(), 2);
}
#[test]
fn jokers_are_never_wildcard_targets() {
    for level in 2..=14 {
        let mut wild = cards(&["2H"])[0];
        wild.rank = level;
        for j in [15, 16] {
            let joker = deck().into_iter().find(|c| c.rank == j).unwrap();
            assert!(classify(&[joker, wild], level).is_none());
        }
        let mut fake = cards(&["small", "small", "big"]);
        fake.push(wild);
        assert!(classify(&fake, level).is_none());
        assert_eq!(
            classify(&cards(&["small", "small"]), level).unwrap().kind,
            Kind::Pair
        );
        assert_eq!(
            classify(&cards(&["big", "big"]), level).unwrap().kind,
            Kind::Pair
        );
        assert_eq!(
            classify(&cards(&["small", "small", "big", "big"]), level)
                .unwrap()
                .bomb,
            Bomb::Jokers
        );
    }
}
#[test]
fn wildcard_sequences_and_ai_straight_flushes() {
    let hand = cards(&["8S", "9S", "10S", "5H", "5H"]);
    assert_eq!(classify(&hand, 5).unwrap().bomb, Bomb::StraightFlush);
    assert!(guandan_desktop::rules::candidates(&hand, 5)
        .iter()
        .any(|m| m.combo.bomb == Bomb::StraightFlush));
    assert_eq!(
        classify(&cards(&["AS", "2D", "3C", "4S", "5D"]), 7)
            .unwrap()
            .kind,
        Kind::Straight
    );
    assert!(classify(&cards(&["QS", "KD", "AC", "2S", "3D"]), 7).is_none());
}
#[test]
fn invalid_or_foreign_card_submissions_are_atomic() {
    let mut g = at(5, 0);
    g.turn = Some(0);
    let before = g.hands.clone();
    let uid = g.hands[0][0].uid;
    assert!(g.play(1, &[uid]).is_err());
    assert_eq!(g.hands, before);
    assert!(g.play(0, &[uid, uid]).is_err());
    assert_eq!(g.hands, before);
    let foreign = g.hands[1][0].uid;
    assert!(g.play(0, &[uid, foreign]).is_err());
    assert_eq!(g.hands, before);
    assert!(g.play(0, &[]).is_err());
    assert_eq!(g.hands, before);
}
#[test]
fn double_down_ends_immediately() {
    let mut g = at(2, 0);
    finish(&mut g, &[0, 2]);
    let r = g.result.unwrap();
    assert_eq!(r.actual_finish_count, 2);
    assert_eq!(r.upgrade, 3);
    assert_eq!(r.level_after, 5);
}
#[test]
fn finished_winner_relays_to_partner_after_every_active_player_passes() {
    let mut g = at(2, 0);
    g.hands[3] = cards(&["KS"]);
    g.hands[0] = cards(&["3S", "4S"]);
    g.hands[1] = cards(&["5S", "6S"]);
    g.hands[2] = cards(&["7S", "8S"]);
    g.turn = Some(3);
    g.play(3, &[g.hands[3][0].uid]).unwrap();
    g.pass(0).unwrap();
    g.pass(1).unwrap();
    assert!(g.trick.is_some());
    assert_eq!(g.turn, Some(2));
    g.pass(2).unwrap();
    assert!(g.trick.is_none());
    assert_eq!(g.turn, Some(1));
    assert!(g.plays.iter().all(Option::is_none));
}
#[test]
fn reaching_ace_does_not_win_until_ace_is_played() {
    let mut g = at(13, 0);
    finish(&mut g, &[0, 2]);
    assert_eq!(g.team_levels[0], 14);
    assert!(g.winner.is_none());
    assert_eq!(g.level, 13);
    g.start_hand();
    assert_eq!(g.level, 14);
}
#[test]
fn passing_own_ace_needs_first_and_second_or_third() {
    for team in [0, 1] {
        for order in [vec![team, team + 2], vec![team, 1 - team, team + 2]] {
            let mut g = at(14, team);
            finish(&mut g, &order);
            assert_eq!(g.winner, Some(team));
        }
    }
}
#[test]
fn three_consecutive_ace_fourth_place_wins_reset_only_the_winning_team() {
    for team in [0, 1] {
        let mut g = at(14, team);
        let other = 1 - team;
        for n in 1..=3 {
            finish(&mut g, &[team, other, other + 2]);
            assert!(g.winner.is_none());
            assert_eq!(g.result.as_ref().unwrap().ace_failures, n);
            assert_eq!(g.team_levels[team], if n == 3 { 2 } else { 14 });
            assert_eq!(g.team_levels[other], 2);
            if n < 3 {
                g.start_hand();
                resolve(&mut g);
            }
        }
        assert!(g.result.as_ref().unwrap().reset_to_two);
        assert_eq!(g.ace_streak, [0, 0]);
        g.start_hand();
        assert_eq!(g.level, 2);
    }
}
#[test]
fn intervening_loss_breaks_ace_streak() {
    let mut g = at(14, 0);
    finish(&mut g, &[0, 1, 3]);
    assert_eq!(g.ace_streak[0], 1);
    g.start_hand();
    resolve(&mut g);
    finish(&mut g, &[1, 0, 3]);
    assert_eq!(g.ace_streak[0], 0);
    assert_eq!(g.team_levels[0], 14);
}
#[test]
fn winning_on_other_teams_lower_level_does_not_pass_ace() {
    let mut g = at(5, 1);
    g.team_levels[0] = 14;
    finish(&mut g, &[0, 2]);
    assert!(g.winner.is_none());
    assert!(!g.result.unwrap().passed_ace);
}
#[test]
fn single_tribute_exempts_wilds_and_preserves_reciprocal_return() {
    let mut g = at(5, 0);
    g.hands[3] = cards(&["5H", "AS", "KD"]);
    g.hands[0] = cards(&["2S", "9D", "10C"]);
    g.begin_tribute(&[0, 1, 2, 3]);
    let e = &g.tribute.as_ref().unwrap().exchanges[0];
    assert_eq!((e.from, e.to, e.tribute.rank), (3, 0, 14));
    assert_eq!(g.turn, None);
    assert!(!g.legal_turn(0));
    assert!(g.play(0, &[g.hands[0][0].uid]).is_err());
    assert!(g.pass(0).is_err());
    assert_eq!(
        g.return_options(0)
            .iter()
            .map(|c| c.rank)
            .collect::<Vec<_>>(),
        vec![2, 9]
    );
    let card = g.return_options(0)[1];
    g.return_card(0, card.uid).unwrap();
    assert!(g.hands[3].contains(&card));
    assert!(g.hands[3].iter().any(|c| c.is_wild(5)));
    assert_eq!(g.turn, Some(3));
}
#[test]
fn first_fourth_victory_tributes_from_the_winners_partner() {
    let mut g = at(5, 0);
    g.hands[2] = cards(&["small", "AS", "5H"]);
    g.hands[0] = cards(&["3S", "9D"]);
    g.begin_tribute(&[0, 1, 3, 2]);
    let e = &g.tribute.as_ref().unwrap().exchanges[0];
    assert_eq!((e.from, e.to, e.tribute.rank), (2, 0, 15));
    resolve(&mut g);
    assert_eq!(g.turn, Some(2));
}
#[test]
fn double_tribute_sorts_offers_and_holds_returns_until_both_are_ready() {
    let mut g = at(5, 0);
    g.hands[1] = cards(&["KS", "9S"]);
    g.hands[3] = cards(&["AS", "8S"]);
    g.hands[0] = cards(&["2S", "3S"]);
    g.hands[2] = cards(&["4S", "6S"]);
    g.begin_tribute(&[0, 2, 1, 3]);
    let e = &g.tribute.as_ref().unwrap().exchanges;
    assert_eq!((e[0].from, e[0].to, e[0].tribute.rank), (3, 0, 14));
    assert_eq!((e[1].from, e[1].to, e[1].tribute.rank), (1, 2, 13));
    let a = g.return_options(0)[0];
    let b = g.return_options(2)[0];
    let before = g.hands.clone();
    g.return_card(0, a.uid).unwrap();
    assert_eq!(g.hands, before);
    assert!(g.tribute_pending());
    assert!(g.return_card(0, a.uid).is_err());
    g.return_card(2, b.uid).unwrap();
    assert!(!g.tribute_pending());
    assert!(g.hands[3].contains(&a));
    assert!(g.hands[1].contains(&b));
    assert_eq!(g.turn, Some(3));
}
#[test]
fn equal_tribute_cards_keep_unique_pairings() {
    let mut g = at(5, 0);
    g.hands[1] = cards(&["AS"]);
    g.hands[3] = cards(&["AD"]);
    g.hands[0] = cards(&["2S"]);
    g.hands[2] = cards(&["3S"]);
    g.begin_tribute(&[0, 2, 1, 3]);
    let giver = g.tribute.as_ref().unwrap().exchanges[0].from;
    resolve(&mut g);
    assert_eq!(g.turn, Some(giver));
}
#[test]
fn largest_tribute_uses_new_level_power_not_printed_rank() {
    let mut g = at(8, 0);
    g.hands[3] = cards(&["8H", "AS", "8C"]);
    g.hands[0] = cards(&["2S"]);
    g.begin_tribute(&[0, 1, 2, 3]);
    let c = g.tribute.as_ref().unwrap().exchanges[0].tribute;
    assert_eq!((c.rank, c.suit), (8, Suit::Clubs));
}
#[test]
fn all_level_suits_are_excluded_from_low_returns() {
    for level in 2..=9 {
        let mut g = at(level, 0);
        g.hands[0] = deck()
            .into_iter()
            .filter(|c| {
                c.uid < 54
                    && (c.rank == level
                        || c.rank == 10
                        || (c.rank == 2 && c.suit == Suit::Clubs)
                        || (c.rank == 9 && c.suit == Suit::Diamonds))
            })
            .collect();
        g.hands[3] = cards(&["AS"]);
        g.begin_tribute(&[0, 1, 2, 3]);
        assert!(!g.return_options(0).is_empty());
        assert!(g
            .return_options(0)
            .iter()
            .all(|c| c.rank != level && (2..=9).contains(&c.rank)));
        for c in g.hands[0].clone().into_iter().filter(|c| c.rank == level) {
            assert!(g.return_card(0, c.uid).is_err());
        }
    }
}
#[test]
fn fallback_returns_lowest_nonwild_not_a_printed_low_trump() {
    let mut g = at(7, 0);
    g.hands[0] = cards(&["7H", "7S", "7D", "7C", "10S", "JD"]);
    g.hands[3] = cards(&["AS"]);
    g.begin_tribute(&[0, 1, 2, 3]);
    assert_eq!(
        g.return_options(0)
            .iter()
            .map(|c| c.rank)
            .collect::<Vec<_>>(),
        vec![10]
    );
    resolve(&mut g);
    assert!(g.hands[3].iter().any(|c| c.rank == 10));
    assert_eq!(g.hands[0].iter().filter(|c| c.rank == 7).count(), 4);
}
#[test]
fn double_rebellion_allows_split_or_combined_big_jokers() {
    for split in [false, true] {
        let mut g = at(5, 0);
        let jokers = cards(&["big", "big"]);
        g.hands[1] = if split {
            vec![jokers[0]]
        } else {
            jokers.clone()
        };
        g.hands[3] = if split {
            vec![jokers[1]]
        } else {
            cards(&["4S"])
        };
        let before = g.hands.clone();
        g.begin_tribute(&[0, 2, 1, 3]);
        assert!(g.tribute.as_ref().unwrap().rebellion);
        assert_eq!(g.hands, before);
        assert_eq!(g.turn, Some(0));
    }
}
#[test]
fn single_rebellion_needs_both_big_jokers_in_last_place() {
    let mut g = at(5, 0);
    g.hands[3] = cards(&["big", "big"]);
    g.begin_tribute(&[0, 1, 2, 3]);
    assert!(g.tribute.as_ref().unwrap().rebellion);
    assert_eq!(g.turn, Some(0));
    let j = cards(&["big", "big"]);
    g.hands[1] = vec![j[0]];
    g.hands[3] = vec![j[1]];
    g.hands[0] = cards(&["2S"]);
    g.begin_tribute(&[0, 1, 2, 3]);
    assert!(!g.tribute.as_ref().unwrap().rebellion);
}
#[test]
fn candidate_classification_matches_authoritative_hand_order() {
    let hand = cards(&["QS", "QD", "JS", "JD", "JH"]);
    for m in guandan_desktop::rules::candidates(&hand, 11) {
        let ids: HashSet<_> = m.cards.iter().map(|c| c.uid).collect();
        let canonical: Vec<_> = hand
            .iter()
            .copied()
            .filter(|c| ids.contains(&c.uid))
            .collect();
        assert_eq!(classify(&canonical, 11).unwrap(), m.combo);
    }
    let current = classify(&cards(&["KS", "KD", "KC", "9S", "9D"]), 11).unwrap();
    assert!(choose_move(&hand, 11, Some(&current), false).is_none());
}
#[test]
fn thirty_full_games_complete_with_unique_cards_and_balanced_exchanges() {
    for seed in 0..30 {
        let mut g = Game::with_seed(seed);
        let mut hands = 0;
        let mut turns = 0;
        while g.winner.is_none() {
            hands += 1;
            assert!(hands < 200, "seed {seed} stuck");
            resolve(&mut g);
            assert_eq!(g.hands.each_ref().map(Vec::len), [27; 4]);
            assert_eq!(
                g.hands
                    .iter()
                    .flatten()
                    .map(|c| c.uid)
                    .collect::<HashSet<_>>()
                    .len(),
                108
            );
            while !g.hand_over {
                turns += 1;
                assert!(turns < 50000, "seed {seed} stuck");
                let s = g.turn.unwrap();
                let partner = g.trick.as_ref().is_some_and(|p| p.seat % 2 == s % 2);
                if let Some(m) = choose_move(
                    &g.hands[s],
                    g.level,
                    g.trick.as_ref().map(|p| &p.combo),
                    partner,
                ) {
                    g.play(s, &m.cards.iter().map(|c| c.uid).collect::<Vec<_>>())
                        .unwrap();
                } else {
                    g.pass(s).unwrap();
                }
            }
            assert!(g.team_levels.iter().all(|l| (2..=14).contains(l)));
            if g.winner.is_none() {
                g.start_hand();
            }
        }
    }
}
