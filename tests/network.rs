use guandan_desktop::{
    net::{normalize_address, Connection, Event, ServerHandle},
    rules::choose_move,
    table::{Command, Message, Phase, View},
};
use std::{
    io::Write,
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

fn pump(clients: &[Connection], views: &mut [Option<View>]) -> Vec<String> {
    let mut errors = Vec::new();
    for (i, c) in clients.iter().enumerate() {
        for event in c.events.try_iter() {
            match event {
                Event::Message(Message::State(v)) => views[i] = Some(*v),
                Event::Message(Message::Error(e)) | Event::Disconnected(e) => errors.push(e),
                _ => {}
            }
        }
    }
    errors
}
fn until(mut check: impl FnMut() -> bool) {
    let start = Instant::now();
    while !check() {
        assert!(
            start.elapsed() < Duration::from_secs(12),
            "network wait timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn invitation_addresses_support_tailnet_dns_ipv4_and_ipv6() {
    assert_eq!(
        normalize_address("100.90.80.70").unwrap(),
        "100.90.80.70:3000"
    );
    assert_eq!(
        normalize_address("guandan://host.tailnet.ts.net:4555/").unwrap(),
        "host.tailnet.ts.net:4555"
    );
    assert_eq!(normalize_address("::1").unwrap(), "[::1]:3000");
    assert!(normalize_address("http://100.1.2.3:3000").is_err());
    assert!(normalize_address("").is_err());
}
#[test]
fn four_native_clients_finish_a_hand_exchange_tribute_and_detect_host_shutdown() {
    let server = ServerHandle::start("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = server.address.to_string();
    let mut clients = Vec::new();
    let mut views: Vec<Option<View>> = Vec::new();
    for i in 0..4 {
        clients.push(Connection::connect(
            address.clone(),
            format!("Native player {}", i + 1),
        ));
        views.push(None);
        until(|| {
            assert!(pump(&clients, &mut views).is_empty());
            views[i].as_ref().is_some_and(|v| v.your_seat == Some(i))
        });
    }
    until(|| {
        assert!(pump(&clients, &mut views).is_empty());
        views
            .iter()
            .all(|v| v.as_ref().is_some_and(|v| v.seats.iter().all(|s| !s.bot)))
    });
    let first_hand = views[0].as_ref().unwrap().hand_id;
    let mut actions = 0;
    loop {
        assert!(pump(&clients, &mut views).is_empty());
        let latest = views
            .iter()
            .filter_map(|v| v.as_ref().map(|v| v.revision))
            .max()
            .unwrap();
        if views
            .iter()
            .any(|v| v.as_ref().is_none_or(|v| v.revision != latest))
        {
            thread::sleep(Duration::from_millis(3));
            continue;
        }
        let state = views[0].as_ref().unwrap();
        if state.phase == Phase::HandOver {
            break;
        }
        assert_eq!(state.hand_id, first_hand);
        let seat = state.turn.unwrap();
        let own = views[seat].as_ref().unwrap();
        let partner = own.trick.as_ref().is_some_and(|p| p.seat % 2 == seat % 2);
        let command = if let Some(m) = choose_move(
            &own.your_hand,
            own.level,
            own.trick.as_ref().map(|p| &p.combo),
            partner,
        ) {
            Command::Play {
                hand_id: own.hand_id,
                cards: m.cards.iter().map(|c| c.uid).collect(),
            }
        } else {
            Command::Pass {
                hand_id: own.hand_id,
            }
        };
        clients[seat].send(command).unwrap();
        actions += 1;
        assert!(actions < 2000);
        until(|| {
            assert!(pump(&clients, &mut views).is_empty());
            views[seat].as_ref().unwrap().revision > latest
        });
    }
    until(|| {
        assert!(pump(&clients, &mut views).is_empty());
        views
            .iter()
            .all(|v| v.as_ref().is_some_and(|v| v.hand_id > first_hand))
    });
    for i in 0..4 {
        if let Some(card) = views[i].as_ref().unwrap().return_options.first().copied() {
            clients[i]
                .send(Command::Return {
                    hand_id: views[i].as_ref().unwrap().hand_id,
                    card,
                })
                .unwrap();
        }
    }
    until(|| {
        assert!(pump(&clients, &mut views).is_empty());
        views
            .iter()
            .all(|v| v.as_ref().is_some_and(|v| v.phase == Phase::Playing))
    });
    let all: Vec<_> = views
        .iter()
        .flat_map(|v| v.as_ref().unwrap().your_hand.iter().map(|c| c.uid))
        .collect();
    assert_eq!(all.len(), 108);
    assert_eq!(
        all.iter().collect::<std::collections::HashSet<_>>().len(),
        108
    );
    for view in views.iter().flatten() {
        assert_eq!(view.your_hand.len(), 27);
        assert!(view.seats.iter().all(|s| s.count == 27));
    }
    drop(server);
    until(|| !pump(&clients, &mut views).is_empty());
}
#[test]
fn malformed_and_oversized_connections_do_not_kill_the_host() {
    let host = ServerHandle::start("127.0.0.1:0".parse().unwrap()).unwrap();
    for bytes in [b"not json\n".to_vec(), vec![b'x'; 70000]] {
        let mut socket = TcpStream::connect(host.address).unwrap();
        let _ = socket.write_all(&bytes);
    }
    let clients = vec![Connection::connect(
        host.address.to_string(),
        "Healthy client".into(),
    )];
    let mut views = vec![None];
    until(|| {
        assert!(pump(&clients, &mut views).is_empty());
        views[0].as_ref().is_some_and(|v| v.your_seat == Some(0))
    });
    assert_eq!(views[0].as_ref().unwrap().your_hand.len(), 27);
}
