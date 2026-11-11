use guandan_desktop::{
    cards::deck,
    game::{Game, PublicAction},
};

#[test]
fn accepted_actions_survive_trick_cleanup_and_rejected_actions_add_nothing() {
    let mut g = Game::with_seed(41);
    let seat = g.turn.unwrap();
    assert!(g.pass(seat).is_err());
    assert!(g.history.is_empty());
    let card = g.hands[seat][0];
    g.play(seat, &[card.uid]).unwrap();
    let winning = g.trick.clone().unwrap();
    assert!(matches!(&g.history[0], PublicAction::Play(p) if p.seat == seat && p.cards == [card]));
    assert!(g.play(seat, &[card.uid]).is_err());
    assert_eq!(g.history.len(), 1);
    while g.trick.is_some() {
        let passer = g.turn.unwrap();
        let count = g.hands[passer].len();
        g.pass(passer).unwrap();
        assert!(matches!(g.history.last().unwrap(), PublicAction::Pass {
            seat: s, winner, combo, remaining,
        } if *s == passer && *winner == seat && *combo == winning.combo && *remaining == count));
    }
    assert_eq!(g.history.len(), 4);
    assert!(g.plays.iter().all(Option::is_none));
    for _ in 0..300 {
        g.note("Display-log entry".into());
    }
    assert_eq!(g.log.len(), 256);
    assert_eq!(g.history.len(), 4);
    g.start_hand();
    assert!(g.history.is_empty());
    assert_eq!(g.hands.each_ref().map(Vec::len), [27; 4]);
}

#[test]
fn tribute_return_choices_enter_public_history_only_when_revealed_together() {
    let mut g = Game::with_seed(21);
    // Both big jokers in the winning team, so the double tribute proceeds.
    let deck = deck();
    g.hands = [
        deck.iter().copied().filter(|c| c.rank == 16).collect(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    for card in deck.into_iter().filter(|c| c.rank != 16) {
        g.hands
            .iter_mut()
            .find(|h| h.len() < 27)
            .unwrap()
            .push(card);
    }
    g.begin_tribute(&[0, 2, 1, 3]);
    let exchanges = g.tribute.as_ref().unwrap().exchanges.clone();
    let first = g.return_options(exchanges[0].to)[0];
    g.return_card(exchanges[0].to, first.uid).unwrap();
    assert!(g.history.is_empty());
    let second = g.return_options(exchanges[1].to)[0];
    g.return_card(exchanges[1].to, second.uid).unwrap();
    assert_eq!(g.history.len(), 2);
    for (action, exchange) in g.history.iter().zip(&g.tribute.as_ref().unwrap().exchanges) {
        assert!(
            matches!(action, PublicAction::Exchange { from, to, given, returned }
            if *from == exchange.from && *to == exchange.to
                && *given == exchange.tribute && Some(*returned) == exchange.returned)
        );
    }
}
