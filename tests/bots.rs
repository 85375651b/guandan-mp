use guandan_desktop::{
    bot::{choose_move, choose_move_with_history, Context},
    cards::{deck, Card, Suit},
    game::Game,
    rules::{choose_move as baseline, classify, Bomb, Kind},
};

fn cards(specs: &[&str]) -> Vec<Card> {
    let mut available = deck();
    specs
        .iter()
        .map(|s| {
            let i = available
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
            available.remove(i)
        })
        .collect()
}

#[test]
fn plans_two_full_houses_instead_of_a_plane_and_two_pairs() {
    let hand = cards(&["3S", "3H", "3D", "4S", "4H", "4D", "5S", "5D", "6S", "6D"]);
    assert_eq!(
        baseline(&hand, 2, None, false).unwrap().combo.kind,
        Kind::Plane
    );
    let m = choose_move(
        &hand,
        2,
        &Context {
            seat: 0,
            counts: [10, 20, 20, 20],
            trick: None,
        },
    )
    .unwrap();
    assert_eq!(m.combo.kind, Kind::FullHouse);
    let rest: Vec<_> = hand.into_iter().filter(|c| !m.cards.contains(c)).collect();
    assert_eq!(classify(&rest, 2).unwrap().kind, Kind::FullHouse);
}

#[test]
fn keeps_a_bomb_intact_while_shedding_a_straight() {
    let hand = cards(&["3S", "3H", "3D", "3C", "4S", "5D", "6C", "7H", "8S", "big"]);
    let m = choose_move(
        &hand,
        2,
        &Context {
            seat: 0,
            counts: [10, 20, 20, 20],
            trick: None,
        },
    )
    .unwrap();
    assert_eq!(m.combo.kind, Kind::Straight);
    assert!(m.cards.iter().all(|c| c.rank != 3));
}

#[test]
fn saves_bomb_unless_it_stops_a_finish_or_leaves_one_play() {
    let hand = cards(&["3S", "3H", "3D", "3C", "4S", "5D", "7C", "9H"]);
    let high = classify(&cards(&["AS"]), 2).unwrap();
    let mut ctx = Context {
        seat: 0,
        counts: [8, 20, 20, 20],
        trick: Some((1, &high)),
    };
    assert!(choose_move(&hand, 2, &ctx).is_none());
    ctx.counts[1] = 1;
    assert_eq!(choose_move(&hand, 2, &ctx).unwrap().combo.bomb, Bomb::Rank);
    let short = cards(&["3S", "3H", "3D", "3C", "4S", "4D"]);
    ctx.counts = [6, 20, 20, 20];
    assert_eq!(choose_move(&short, 2, &ctx).unwrap().combo.bomb, Bomb::Rank);
}

#[test]
fn lets_partner_win_but_takes_a_chance_to_go_out() {
    let current = classify(&cards(&["6S", "6H"]), 2).unwrap();
    let ctx = Context {
        seat: 0,
        counts: [3, 20, 1, 20],
        trick: Some((2, &current)),
    };
    assert!(choose_move(&cards(&["7S", "7H", "9D"]), 2, &ctx).is_none());
    let m = choose_move(&cards(&["7S", "7H"]), 2, &ctx).unwrap();
    assert_eq!(m.cards.len(), 2);
}

#[test]
fn checks_both_opponents_when_deciding_to_stop_a_finish() {
    let hand = cards(&["3S", "3H", "3D", "3C", "4S", "5D", "7C", "9H"]);
    let current = classify(&cards(&["AS", "AD"]), 2).unwrap();
    let ctx = Context {
        seat: 0,
        counts: [8, 1, 20, 2],
        trick: Some((1, &current)),
    };
    assert_eq!(choose_move(&hand, 2, &ctx).unwrap().combo.bomb, Bomb::Rank);
}

#[test]
fn prevents_an_opponent_with_one_card_from_regaining_the_lead_on_a_pair() {
    let hand = cards(&["3S", "3H", "3D", "3C", "4S", "5D", "7C", "9H"]);
    let current = classify(&cards(&["AS", "AD"]), 2).unwrap();
    let ctx = Context {
        seat: 0,
        counts: [8, 1, 20, 20],
        trick: Some((1, &current)),
    };
    assert!(choose_move(&hand, 2, &ctx).is_none());
    assert_eq!(
        choose_move_with_history(&hand, 2, &ctx, &[])
            .unwrap()
            .combo
            .bomb,
        Bomb::Rank
    );
}

#[test]
fn feeds_a_partner_with_two_cards_and_leads_low() {
    let hand = cards(&["3S", "3H", "4S", "4D", "5S", "5D", "6S", "6D"]);
    let ctx = Context {
        seat: 0,
        counts: [8, 20, 2, 20],
        trick: None,
    };
    let m = choose_move(&hand, 2, &ctx).unwrap();
    assert_eq!(m.combo.kind, Kind::Pair);
    assert_eq!(m.combo.rank, 3);
}

#[test]
fn wins_high_to_take_the_lead_for_a_nearly_finished_partner() {
    let hand = cards(&["3S", "9D", "AC"]);
    let current = classify(&cards(&["4H"]), 2).unwrap();
    let ctx = Context {
        seat: 0,
        counts: [3, 20, 1, 20],
        trick: Some((1, &current)),
    };
    assert_eq!(choose_move(&hand, 2, &ctx).unwrap().combo.rank, 14);
}

#[test]
fn does_not_feed_a_single_to_an_opponent_with_one_card() {
    let hand = cards(&["3S", "4S", "4D"]);
    let ctx = Context {
        seat: 0,
        counts: [3, 1, 20, 20],
        trick: None,
    };
    assert_eq!(choose_move(&hand, 2, &ctx).unwrap().combo.kind, Kind::Pair);
}

#[test]
fn finished_partners_relay_is_not_overcalled() {
    let hand = cards(&["9S", "AD"]);
    let current = classify(&cards(&["4H"]), 2).unwrap();
    let ctx = Context {
        seat: 0,
        counts: [2, 1, 0, 20],
        trick: Some((2, &current)),
    };
    assert!(choose_move(&hand, 2, &ctx).is_none());
}

#[test]
fn wildcard_and_joker_combinations_still_obey_the_rules() {
    for specs in [
        vec!["small", "small"],
        vec!["big", "big"],
        vec!["3S", "3H", "2H"],
    ] {
        let hand = cards(&specs);
        let ctx = Context {
            seat: 0,
            counts: [hand.len(), 20, 20, 20],
            trick: None,
        };
        let m = choose_move(&hand, 2, &ctx).unwrap();
        assert_eq!(m.cards.len(), hand.len());
        assert_eq!(classify(&m.cards, 2), Some(m.combo));
    }
    let hand = cards(&["small", "2H"]);
    let current = classify(&cards(&["AS", "AD"]), 2).unwrap();
    let ctx = Context {
        seat: 0,
        counts: [2, 20, 20, 20],
        trick: Some((1, &current)),
    };
    assert!(choose_move(&hand, 2, &ctx).is_none());
}

#[test]
fn two_fully_smart_games_complete_through_scoring_and_tribute() {
    for seed in [500, 501] {
        let mut g = Game::with_seed(seed);
        let mut hands = 0;
        while g.winner.is_none() {
            hands += 1;
            assert!(hands < 200);
            while g.tribute_pending() {
                let seat = g
                    .tribute
                    .as_ref()
                    .unwrap()
                    .exchanges
                    .iter()
                    .find(|e| e.returned.is_none())
                    .unwrap()
                    .to;
                let card = g.return_options(seat)[0];
                g.return_card(seat, card.uid).unwrap();
            }
            assert_eq!(g.hands.each_ref().map(Vec::len), [27; 4]);
            let mut turns = 0;
            while !g.hand_over {
                let seat = g.turn.unwrap();
                let m = choose_move_with_history(
                    &g.hands[seat],
                    g.level,
                    &Context {
                        seat,
                        counts: g.hands.each_ref().map(Vec::len),
                        trick: g.trick.as_ref().map(|p| (p.seat, &p.combo)),
                    },
                    &g.history,
                );
                if let Some(m) = m {
                    g.play(seat, &m.cards.iter().map(|c| c.uid).collect::<Vec<_>>())
                        .unwrap();
                } else {
                    g.pass(seat).unwrap();
                }
                turns += 1;
                assert!(turns < 2000);
            }
            if g.winner.is_none() {
                g.start_hand();
            }
        }
    }
}

fn play_hand(seed: u64, smart_team: usize, tracking: bool) -> (bool, usize, std::time::Duration) {
    let mut g = Game::with_seed(seed);
    g.team_levels = [(seed % 13 + 2) as u8; 2];
    g.start_hand();
    let mut turns = 0;
    let mut slowest = std::time::Duration::ZERO;
    let mut played = std::collections::HashSet::new();
    while !g.hand_over {
        let seat = g.turn.unwrap();
        let current = g.trick.as_ref();
        let m = if seat % 2 == smart_team {
            let start = std::time::Instant::now();
            let context = Context {
                seat,
                counts: g.hands.each_ref().map(Vec::len),
                trick: current.map(|p| (p.seat, &p.combo)),
            };
            let m = if tracking {
                choose_move_with_history(&g.hands[seat], g.level, &context, &g.history)
            } else {
                choose_move(&g.hands[seat], g.level, &context)
            };
            slowest = slowest.max(start.elapsed());
            m
        } else if tracking {
            choose_move(
                &g.hands[seat],
                g.level,
                &Context {
                    seat,
                    counts: g.hands.each_ref().map(Vec::len),
                    trick: current.map(|p| (p.seat, &p.combo)),
                },
            )
        } else {
            baseline(
                &g.hands[seat],
                g.level,
                current.map(|p| &p.combo),
                current.is_some_and(|p| p.seat % 2 == seat % 2),
            )
        };
        if let Some(m) = m {
            assert_eq!(classify(&m.cards, g.level).as_ref(), Some(&m.combo));
            for c in &m.cards {
                assert!(played.insert(c.uid));
            }
            g.play(seat, &m.cards.iter().map(|c| c.uid).collect::<Vec<_>>())
                .unwrap();
        } else {
            g.pass(seat).unwrap();
        }
        let remaining: std::collections::HashSet<_> =
            g.hands.iter().flatten().map(|c| c.uid).collect();
        assert_eq!(remaining.len(), 108 - played.len());
        assert!(remaining.is_disjoint(&played));
        turns += 1;
        assert!(turns < 2000, "seed {seed} stalled");
    }
    (g.result.unwrap().team == smart_team, turns, slowest)
}

#[test]
fn seeded_hands_finish_with_legal_moves_and_no_duplicated_cards() {
    for seed in 0..4 {
        for team in 0..2 {
            play_hand(seed, team, false);
            play_hand(seed, team, true);
        }
    }
}

/// A reproducible comparison, kept out of the quick regression suite. Each seed
/// is played twice, swapping policies between teams, with identical deals.
/// Run in release mode; wall time from debug builds is not representative.
#[test]
#[ignore = "benchmark: cargo test --release --no-default-features --test bots compare_against_baseline -- --ignored --nocapture"]
fn compare_against_baseline() {
    let mut wins = 0;
    let mut turns = 0;
    let mut slowest = std::time::Duration::ZERO;
    let start = std::time::Instant::now();
    for seed in 0..100 {
        for team in 0..2 {
            let (won, n, elapsed) = play_hand(seed, team, false);
            wins += usize::from(won);
            turns += n;
            slowest = slowest.max(elapsed);
        }
    }
    eprintln!(
        "Level 2 won {wins}/200 hands; {turns} turns in {:?}; slowest decision {slowest:?}",
        start.elapsed()
    );
}

#[test]
#[ignore = "benchmark: cargo test --release --no-default-features --test bots compare_tracking_against_level_two -- --ignored --nocapture"]
fn compare_tracking_against_level_two() {
    let mut wins = 0;
    let mut turns = 0;
    let mut slowest = std::time::Duration::ZERO;
    let start = std::time::Instant::now();
    let first: u64 = std::env::var("GUANDAN_BENCHMARK_START")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let deals: u64 = std::env::var("GUANDAN_BENCHMARK_DEALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    for seed in first..first + deals {
        for team in 0..2 {
            let (won, n, elapsed) = play_hand(seed, team, true);
            wins += usize::from(won);
            turns += n;
            slowest = slowest.max(elapsed);
        }
    }
    eprintln!("Level 3 won {wins}/{} hands against level 2 (seeds {first}..{}); {turns} turns in {:?}; slowest decision {slowest:?}", deals * 2, first + deals, start.elapsed());
}
