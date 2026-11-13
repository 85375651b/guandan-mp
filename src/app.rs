use crate::{
    cards::{rank_label, Card},
    net::{Connection, Event, ServerHandle},
    rules::classify,
    table::{Command, Message, Phase, Table, View, PROTOCOL_VERSION},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

mod theme;
use theme::{CANVAS, CARD_TOP, ERROR, HAIRLINE, ICE, OBSIDIAN, PEWTER, SILVER};

#[derive(Clone, Copy)]
struct Layout {
    narrow: bool,
    margin: f32,
    card_height: f32,
    scale: f32,
}
impl Layout {
    fn for_size(size: Vec2) -> Self {
        Self {
            narrow: size.x < 760.0,
            margin: if size.x < 760.0 || size.y < 600.0 {
                8.0
            } else {
                14.0
            },
            card_height: (size.y * 0.105).clamp(48.0, 76.0),
            scale: (size.x / 1000.0).min(size.y / 700.0).clamp(0.86, 1.0),
        }
    }
    fn apply(self, ctx: &egui::Context) {
        ctx.style_mut(|s| {
            s.visuals = theme::visuals();
            s.spacing.item_spacing = Vec2::new(8.0 * self.scale, 6.0 * self.scale);
            s.spacing.button_padding = Vec2::new(10.0 * self.scale, 6.0 * self.scale);
            s.spacing.interact_size.y = 28.0 * self.scale;
            s.spacing.scroll = egui::style::ScrollStyle::solid();
            for (style, size) in [
                (egui::TextStyle::Body, 14.0),
                (egui::TextStyle::Button, 14.0),
                (egui::TextStyle::Heading, 20.0),
                (egui::TextStyle::Small, 11.0),
            ] {
                s.text_styles
                    .insert(style, theme::label_font(size * self.scale));
            }
        });
    }
}

fn control(ui: &mut egui::Ui, key: &'static str, label: &str, enabled: bool) -> egui::Response {
    let response = theme::ghost_button(ui, label, enabled);
    // Geometry assertions exercise the real widgets, including their clipping region.
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new(("control", key)),
            (response.rect, ui.clip_rect()),
        )
    });
    #[cfg(not(test))]
    let _ = key;
    response
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    name: String,
    address: String,
    tailnet_ip: String,
    port: u16,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "Player".into(),
            address: "".into(),
            tailnet_ip: std::env::var("TAILSCALE_IP").unwrap_or_default(),
            port: 3000,
        }
    }
}
fn settings_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .map(|p| p.join("guandan/settings.json"))
}
impl Settings {
    fn load() -> Self {
        settings_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn save(&self) {
        if let Some(p) = settings_path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(data) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(p, data);
            }
        }
    }
}

pub struct GuandanApp {
    settings: Settings,
    connection: Option<Connection>,
    host: Option<ServerHandle>,
    view: Option<View>,
    selected: BTreeSet<u8>,
    error: Option<String>,
    status: String,
    show_log: bool,
    show_rules: bool,
    endpoint: String,
    connected: bool,
    demo: bool,
    discovery: Option<mpsc::Receiver<Option<String>>>,
}

impl GuandanApp {
    pub fn new(cc: &eframe::CreationContext<'_>, demo: bool) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(theme::visuals());
        cc.egui_ctx.style_mut(|s| {
            s.spacing.item_spacing = Vec2::new(10.0, 9.0);
            s.spacing.button_padding = Vec2::new(15.0, 9.0);
        });
        let mut app = Self {
            settings: Settings::load(),
            connection: None,
            host: None,
            view: None,
            selected: BTreeSet::new(),
            error: None,
            status: String::new(),
            show_log: false,
            show_rules: false,
            endpoint: String::new(),
            connected: false,
            demo,
            discovery: None,
        };
        if demo {
            app.view = Some(demo_view());
            app.status = "Preview · native interface".into();
        } else if app.settings.tailnet_ip.is_empty() {
            app.detect_ip();
        }
        app
    }
    fn detect_ip(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.discovery = Some(rx);
        std::thread::spawn(move || {
            use std::process::{Command, Stdio};
            let found = (|| {
                let mut child = Command::new("tailscale")
                    .args(["ip", "-4"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok()?;
                let start = Instant::now();
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) if start.elapsed() < Duration::from_secs(2) => {
                            std::thread::sleep(Duration::from_millis(30))
                        }
                        _ => {
                            let _ = child.kill();
                            let _ = child.wait();
                            return None;
                        }
                    }
                }
                let output = child.wait_with_output().ok()?;
                let text = String::from_utf8(output.stdout).ok()?;
                text.lines()
                    .find_map(|l| l.trim().parse::<std::net::Ipv4Addr>().ok())
                    .map(|ip| ip.to_string())
            })();
            let _ = tx.send(found);
        });
    }
    fn poll(&mut self) {
        if let Some(rx) = &self.discovery {
            if let Ok(ip) = rx.try_recv() {
                if let Some(ip) = ip {
                    self.settings.tailnet_ip = ip;
                }
                self.discovery = None;
            }
        }
        let events: Vec<_> = self
            .connection
            .as_ref()
            .map(|c| c.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Connected => {
                    self.connected = true;
                    self.status = "Connected · joining the table…".into();
                }
                Event::Disconnected(error) => {
                    self.connected = false;
                    self.error = Some(error);
                    self.status = "Disconnected".into();
                }
                Event::Message(Message::Error(error)) => self.error = Some(error),
                Event::Message(Message::State(view)) => {
                    if self
                        .view
                        .as_ref()
                        .is_none_or(|old| old.hand_id != view.hand_id || old.phase != view.phase)
                    {
                        self.selected.clear();
                    }
                    self.selected.retain(|uid| {
                        view.your_hand.iter().any(|c| c.uid == *uid)
                            && (view.phase != Phase::Tribute || view.return_options.contains(uid))
                    });
                    if view.your_seat.is_some() {
                        self.status = "Connected".into();
                    }
                    self.view = Some(*view);
                }
                Event::Message(Message::Pong) => {}
            }
        }
    }
    fn connect(&mut self, address: String) {
        self.connection = Some(Connection::connect(
            address.clone(),
            self.settings.name.clone(),
        ));
        self.endpoint = address;
        self.connected = false;
        self.view = None;
        self.selected.clear();
        self.error = None;
        self.status = "Connecting…".into();
    }
    fn start_host(&mut self, solo: bool) {
        let bind = if solo {
            "127.0.0.1:0".to_string()
        } else {
            format!("0.0.0.0:{}", self.settings.port)
        };
        match ServerHandle::start(bind.parse().unwrap()) {
            Ok(host) => {
                let address = format!("127.0.0.1:{}", host.address.port());
                self.host = Some(host);
                self.settings.save();
                self.connect(address);
            }
            Err(error) => {
                self.error = Some(format!(
                    "Could not host: {error}. Try a different port or close the other host."
                ))
            }
        }
    }
    fn leave(&mut self) {
        self.connection = None;
        self.host = None;
        self.view = None;
        self.selected.clear();
        self.connected = false;
        self.error = None;
        self.status.clear();
        self.demo = false;
    }
    fn send(&mut self, command: Command) {
        if let Some(c) = &self.connection {
            match c.send(command) {
                Ok(()) => {
                    self.selected.clear();
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
    fn home(&mut self, ctx: &egui::Context) {
        let layout = Layout::for_size(ctx.screen_rect().size());
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(layout.margin))
            .show(ctx, |ui| {
                theme::slate(ui.painter(), ctx.screen_rect(), 0.0);
                egui::ScrollArea::vertical().id_salt("lobby-scroll").auto_shrink([false,false]).show(ui, |ui| {
                    ui.add_space(if layout.narrow { 8.0 } else { 20.0 });
                    ui.vertical_centered(|ui| {
                        theme::wordmark(ui, if layout.narrow {28.0} else {40.0});
                        ui.label(RichText::new("A table for four. Wherever your friends are.").color(PEWTER));
                    });
                    ui.add_space(12.0);
                    let width=ui.available_width().min(820.0);
                    ui.vertical_centered(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(width,0.0),egui::Layout::top_down(egui::Align::Min),|ui| {
                            ui.label("Your name");
                            theme::input(ui, egui::TextEdit::singleline(&mut self.settings.name).char_limit(20).desired_width(ui.available_width()));
                            ui.add_space(8.0);
                            theme::container().show(ui,|ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new("Play on this computer").heading().color(SILVER));
                                ui.label("A fresh game with three bots. Works entirely offline.");
                                if control(ui,"solo","Start solo game",true).clicked() {self.start_host(true);}
                            });
                            ui.add_space(8.0);
                            if layout.narrow {
                                self.host_form(ui);
                                ui.add_space(8.0);
                                self.join_form(ui);
                            } else {
                                ui.columns(2,|columns| {self.host_form(&mut columns[0]);self.join_form(&mut columns[1]);});
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Empty seats are bots. Friends can take an open seat at any time.").color(PEWTER));
                            if let Some(error)=&self.error {ui.colored_label(ERROR,error);}
                            if control(ui,"lobby-rules","Read the table rules",true).clicked() {self.show_rules=true;}
                        });
                    });
                });
            });
    }
    fn host_form(&mut self, ui: &mut egui::Ui) {
        theme::container().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Host a table").heading().color(SILVER));
            ui.label("Friends join you through Tailscale or your LAN.");
            ui.horizontal_wrapped(|ui| {
                ui.label("TCP port");
                ui.add(egui::DragValue::new(&mut self.settings.port).range(1024..=65535));
            });
            ui.label("Your Tailscale IP (for invitations)");
            theme::input(
                ui,
                egui::TextEdit::singleline(&mut self.settings.tailnet_ip)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("100.x.y.z")
                    .desired_width(ui.available_width()),
            );
            ui.horizontal_wrapped(|ui| {
                if control(
                    ui,
                    "detect-ip",
                    "Detect Tailscale IP",
                    self.discovery.is_none(),
                )
                .clicked()
                {
                    self.detect_ip();
                }
                if control(ui, "host", "Host game", true).clicked() {
                    self.start_host(false);
                }
            });
        });
    }
    fn join_form(&mut self, ui: &mut egui::Ui) {
        theme::container().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Join friends").heading().color(SILVER));
            ui.label("Enter the host's address or paste an invitation.");
            ui.label("Host address");
            theme::input(
                ui,
                egui::TextEdit::singleline(&mut self.settings.address)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("100.x.y.z:3000")
                    .desired_width(ui.available_width()),
            );
            ui.label(
                RichText::new("Everyone runs this desktop app and connects to Tailscale.")
                    .color(PEWTER)
                    .small(),
            );
            if control(ui, "join", "Join game", true).clicked() {
                self.settings.save();
                self.connect(self.settings.address.clone());
            }
        });
    }
    fn header(&mut self, ctx: &egui::Context, view: &View) {
        let layout = Layout::for_size(ctx.screen_rect().size());
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(OBSIDIAN)
                    .stroke(Stroke::new(1.0_f32, HAIRLINE))
                    .inner_margin(layout.margin),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    theme::wordmark(ui, if layout.narrow { 18.0 } else { 23.0 });
                    ui.label(
                        RichText::new(format!("Team A · {}", rank_label(view.team_levels[0])))
                            .color(ICE)
                            .font(FontId::proportional(14.0 * layout.scale)),
                    );
                    ui.label(
                        RichText::new(format!("Team B · {}", rank_label(view.team_levels[1])))
                            .color(ICE)
                            .font(FontId::proportional(14.0 * layout.scale)),
                    );
                    let mut wild = egui::text::LayoutJob::default();
                    let format = egui::TextFormat {
                        font_id: FontId::proportional(14.0 * layout.scale),
                        color: SILVER,
                        ..Default::default()
                    };
                    wild.append(
                        &format!("Wild · {} ", rank_label(view.level)),
                        0.0,
                        format.clone(),
                    );
                    wild.append(
                        "♥",
                        0.0,
                        egui::TextFormat {
                            color: ICE,
                            ..format
                        },
                    );
                    ui.label(wild);
                    if control(
                        ui,
                        "log-toggle",
                        if self.show_log {
                            "Hide log"
                        } else {
                            "Play log"
                        },
                        true,
                    )
                    .clicked()
                    {
                        self.show_log = !self.show_log;
                    }
                    if control(ui, "rules", "Rules", true).clicked() {
                        self.show_rules = !self.show_rules;
                    }
                    if control(
                        ui,
                        "leave",
                        if self.host.is_some() {
                            "Close table"
                        } else {
                            "Leave"
                        },
                        true,
                    )
                    .clicked()
                    {
                        self.leave();
                    }
                    ui.label(
                        RichText::new(if self.demo {
                            "Preview"
                        } else if self.connected {
                            "Connected"
                        } else {
                            "Offline"
                        })
                        .small()
                        .color(PEWTER),
                    );
                });
                if let Some(host) = &self.host {
                    if !host.address.ip().is_loopback() {
                        let ip = if self.settings.tailnet_ip.trim().is_empty() {
                            "XXX.XXX.XXX.XXX"
                        } else {
                            self.settings.tailnet_ip.trim()
                        };
                        let invitation = format!("guandan://{ip}:{}", host.address.port());
                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("On your tailnet: {invitation}"))
                                        .small()
                                        .color(PEWTER),
                                )
                                .wrap(),
                            );
                            if control(ui, "copy-invite", "Copy invitation", true).clicked() {
                                ctx.copy_text(invitation);
                            }
                        });
                    }
                }
            });
    }
    fn hand(&mut self, ctx: &egui::Context, view: &View) {
        let screen = ctx.screen_rect().size();
        let layout = Layout::for_size(screen);
        egui::TopBottomPanel::bottom("hand")
            .max_height(screen.y*0.68)
            .frame(egui::Frame::new().fill(OBSIDIAN).inner_margin(layout.margin))
            .show(ctx,|ui| {
                let mine=view.your_seat;
                let your_turn=self.connected && view.phase==Phase::Playing && view.turn==mine && mine.is_some();
                let returning=self.connected && view.phase==Phase::Tribute && !view.return_options.is_empty();
                // Bound explanatory text separately: it must never displace the buttons.
                egui::ScrollArea::vertical().id_salt("hand-info").max_height((screen.y*0.18).clamp(32.0,120.0)).min_scrolled_height(0.0).auto_shrink([false,true]).show(ui,|ui| {
                    if view.show_tribute {
                        if let Some(t)=&view.tribute {
                            let who=|s:usize| if Some(s)==mine {"you".to_string()} else {view.seats[s].name.clone()};
                            if t.rebellion {ui.colored_label(SILVER,format!("Tribute rebellion · no exchange · {} leads",who(t.leader)));}
                            else if returning {
                                let e=t.exchanges.iter().find(|e|Some(e.to)==mine).unwrap();
                                let low=view.your_hand.iter().any(|c|view.return_options.contains(&c.uid) && c.rank!=view.level && (2..=9).contains(&c.rank));
                                ui.colored_label(SILVER,format!("You received {} from {}. {}",e.tribute,who(e.from),if low {"Choose a highlighted non-trump 2–9 to return."} else {"No eligible 2–9: return your highlighted lowest non-wild card."}));
                            } else if !t.complete {ui.colored_label(SILVER,"Waiting for the other tribute recipients to choose their returns…");}
                            else {ui.colored_label(SILVER,format!("Tribute exchange complete · {} leads",who(t.leader)));}
                            ui.label(RichText::new(t.exchanges.iter().map(|e|format!("{} → {}: {}{}",who(e.from),who(e.to),e.tribute,e.returned.map_or(String::new(),|c|format!(" · returned {c}")))).collect::<Vec<_>>().join("     ")).small().color(PEWTER));
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("YOUR HAND · {} cards",view.your_hand.len())).small().color(PEWTER));
                        ui.label(RichText::new(status_text(view)).color(if your_turn {ICE} else {SILVER}).font(FontId::proportional(14.0*layout.scale)));
                    });
                    if let Some(error)=&self.error {ui.colored_label(ERROR,error);}
                    if !self.connected && !self.demo {ui.small("The host must remain running. Rejoining takes an open seat.");}
                });
                let card_size=Vec2::new(layout.card_height*51.0/76.0,layout.card_height);
                let lift=layout.card_height*0.16;
                egui::ScrollArea::horizontal().id_salt("hand-scroll").max_height(card_size.y+lift+14.0).show(ui,|ui| {
                    // Keep the entire rank/suit index exposed. Very narrow windows
                    // scroll the hand rather than cover the symbols on adjacent cards.
                    let min_step=26.0*card_scale(card_size);
                    let step=((ui.available_width()-card_size.x)/view.your_hand.len().saturating_sub(1).max(1) as f32).clamp(min_step,33.0);
                    let width=if view.your_hand.is_empty() {0.0} else {step*(view.your_hand.len()-1) as f32+card_size.x};
                    let (area,_)=ui.allocate_exact_size(Vec2::new(width.max(1.0),card_size.y+lift+6.0),Sense::hover());
                    for (i,card) in view.your_hand.iter().enumerate() {
                        let selected=self.selected.contains(&card.uid);
                        let eligible=!returning || view.return_options.contains(&card.uid);
                        let rect=Rect::from_min_size(area.min+Vec2::new(i as f32*step,if selected {1.0} else {lift}),card_size);
                        draw_card(ui.painter(),rect,*card,view.level,selected,returning && eligible,returning && !eligible);
                        let hit=Rect::from_min_size(rect.min,Vec2::new(if i+1==view.your_hand.len() {card_size.x} else {step},card_size.y));
                        let response=ui.interact(hit,ui.id().with(("card",card.uid)),Sense::click());
                        #[cfg(test)]
                        ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new(("hand-card",card.uid)),(response.rect,ui.clip_rect())));
                        if response.clicked() && (your_turn || returning) && eligible {
                            if returning {self.selected.clear();if !selected {self.selected.insert(card.uid);}}
                            else if selected {self.selected.remove(&card.uid);} else {self.selected.insert(card.uid);}
                        }
                        response.on_hover_text(format!("{card}{}",if card.is_wild(view.level) {" · wildcard"} else if card.rank==view.level {" · level trump"} else {""}));
                    }
                });
                let selected_cards:Vec<_>=view.your_hand.iter().copied().filter(|c|self.selected.contains(&c.uid)).collect();
                let selected_combo=classify(&selected_cards,view.level);
                ui.horizontal_wrapped(|ui| {
                    if view.phase==Phase::Tribute {
                        if control(ui,"return","Return selected card",returning && self.selected.len()==1).clicked() {self.send(Command::Return {hand_id:view.hand_id,card:*self.selected.first().unwrap()});}
                    } else {
                        let legal=your_turn && selected_combo.as_ref().is_some_and(|c|c.beats(view.trick.as_ref().map(|p|&p.combo)));
                        if control(ui,"play","Play selected",legal).clicked() {self.send(Command::Play {hand_id:view.hand_id,cards:self.selected.iter().copied().collect()});}
                        if control(ui,"pass","Pass",your_turn && view.trick.is_some()).clicked() {self.send(Command::Pass {hand_id:view.hand_id});}
                    }
                    if control(ui,"clear","Clear selection",!self.selected.is_empty()).clicked() {self.selected.clear();}
                    if view.phase==Phase::GameOver && control(ui,"new-game","New game",true).clicked() {self.send(Command::NewGame);}
                    if !self.connected && !self.demo && control(ui,"reconnect","Reconnect",true).clicked() {self.connect(self.endpoint.clone());}
                    if !selected_cards.is_empty() && view.phase!=Phase::Tribute {ui.label(selected_combo.as_ref().map_or("Not a legal combination",|c|c.label()));}
                });
            });
    }
    fn table(&mut self, ctx: &egui::Context, view: &View) {
        let layout = Layout::for_size(ctx.screen_rect().size());
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(layout.margin))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("table-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| paint_table(ui, view));
            });
        if matches!(view.phase, Phase::HandOver | Phase::GameOver) {
            if let Some(result) = &view.result {
                egui::Window::new(if view.phase == Phase::GameOver {
                    "Game complete"
                } else {
                    "Hand complete"
                })
                .id(egui::Id::new("result"))
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .max_size(ctx.screen_rect().size() - Vec2::new(32.0, 48.0))
                .min_size([180.0, 100.0])
                .collapsible(false)
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.heading(format!(
                        "Team {} {}",
                        if result.team == 0 { "A" } else { "B" },
                        if result.passed_ace {
                            "passes Ace!"
                        } else {
                            "wins the hand"
                        }
                    ));
                    for (i, s) in result.order.iter().enumerate() {
                        ui.label(format!(
                            "{}  {}",
                            if i < result.actual_finish_count {
                                format!("#{}", i + 1)
                            } else {
                                "Did not finish".into()
                            },
                            view.seats[*s].name
                        ));
                    }
                    if result.reset_to_two {
                        ui.colored_label(
                            SILVER,
                            "Three consecutive 1–4 Ace wins: back to level 2.",
                        );
                    } else if result.ace_failures > 0 {
                        ui.label(format!(
                            "Stays at Ace · {} / 3 consecutive 1–4 wins",
                            result.ace_failures
                        ));
                    } else {
                        ui.label(format!("Team level: {}", rank_label(result.level_after)));
                    }
                    if result.passed_ace {
                        if control(ui, "result-new-game", "Start a new game", true).clicked() {
                            self.send(Command::NewGame);
                        }
                    } else {
                        ui.small("The next deal and tribute exchange start automatically.");
                    }
                });
            }
        }
    }
    fn rules_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Table rules")
            .open(&mut self.show_rules)
            .default_size([620.0, 480.0])
            .max_size(ctx.screen_rect().size() - Vec2::new(32.0, 48.0))
            .min_size([180.0, 100.0])
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(include_str!("../RULES.txt"));
            });
    }
    fn log_contents(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("Play log");
            if control(ui, "log-close", "Close", true).clicked() {
                self.show_log = false;
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("log-entries")
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if let Some(view) = &self.view {
                    for line in &view.log {
                        ui.label(line);
                        ui.separator();
                    }
                }
            });
    }
    fn render(&mut self, ctx: &egui::Context) {
        let size = ctx.screen_rect().size();
        Layout::for_size(size).apply(ctx);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_log = false;
            self.show_rules = false;
        }
        if let Some(view) = self.view.clone() {
            self.header(ctx, &view);
            let floating_log = size.x < 960.0;
            if self.show_log && !floating_log {
                egui::SidePanel::right("log")
                    .frame(
                        egui::Frame::new()
                            .fill(OBSIDIAN)
                            .stroke(Stroke::new(1.0_f32, HAIRLINE))
                            .inner_margin(10.0),
                    )
                    .default_width(250.0)
                    .min_width(180.0)
                    .max_width((size.x * 0.28).min(320.0))
                    .show(ctx, |ui| self.log_contents(ui));
            }
            self.hand(ctx, &view);
            self.table(ctx, &view);
            if self.show_log && floating_log {
                let mut open = true;
                egui::Window::new("Play log")
                    .id(egui::Id::new("floating-log"))
                    .open(&mut open)
                    .default_size([300.0, size.y * 0.55])
                    .max_size(size - Vec2::new(24.0, 48.0))
                    .min_size([180.0, 100.0])
                    .show(ctx, |ui| self.log_contents(ui));
                self.show_log &= open;
            }
        } else if self.connection.is_some() {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space((size.y * 0.15).min(60.0));
                        ui.heading(&self.status);
                        if let Some(error) = &self.error {
                            ui.colored_label(ERROR, error);
                        }
                        if control(ui, "back", "Back", true).clicked() {
                            self.leave();
                        }
                    });
            });
        } else {
            self.home(ctx);
        }
        self.rules_window(ctx);
    }
}

impl eframe::App for GuandanApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        ctx.request_repaint_after(Duration::from_millis(80));
        self.render(ctx);
    }
}

fn paint_table(ui: &mut egui::Ui, view: &View) {
    // Shrink the painted table with the viewport. Very short windows can scroll
    // the table independently while the hand and action controls stay in place.
    let size = Vec2::new(ui.available_width(), ui.available_height().max(240.0));
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let center = r.center();
    let sx = r.width() / 900.0;
    let sy = r.height() / 440.0;
    let scale = sx.min(sy).min(1.0);
    let surface = r.shrink(2.0);
    let radius = 4.0 * scale;
    theme::slate(p, surface, radius);
    p.rect_stroke(
        surface,
        radius,
        Stroke::new(1.0_f32, HAIRLINE),
        egui::StrokeKind::Inside,
    );
    theme::engrave(p, surface, 24.0 * scale.max(0.65));
    // Recess the slate with dark inner edges instead of a colored felt rail.
    for inset in 1..=3 {
        p.rect_stroke(
            surface.shrink(inset as f32),
            (radius - inset as f32).max(0.0),
            Stroke::new(1.0_f32, Color32::from_black_alpha(65 / inset)),
            egui::StrokeKind::Inside,
        );
    }
    let radius = r.height().min(r.width()) * 0.25;
    p.add(egui::Shape::closed_line(
        vec![
            center - Vec2::new(0.0, radius),
            center + Vec2::new(radius, 0.0),
            center + Vec2::new(0.0, radius),
            center - Vec2::new(radius, 0.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgb(36, 55, 80)),
    ));
    let mine = view.your_seat.unwrap_or(0);
    for (offset, pos, role) in [
        (
            2,
            Pos2::new(center.x, r.top() + (33.0 * sy).max(26.0)),
            "PARTNER",
        ),
        (3, Pos2::new(r.left() + 66.0 * sx, center.y), "OPPONENT"),
        (1, Pos2::new(r.right() - 66.0 * sx, center.y), "OPPONENT"),
        (
            0,
            Pos2::new(center.x, r.bottom() - (31.0 * sy).max(26.0)),
            "YOU",
        ),
    ] {
        let seat = (mine + offset) % 4;
        let label = if r.width() < 650.0 {
            if offset == 2 {
                "Partner".into()
            } else if offset == 0 {
                "You".into()
            } else {
                format!("Seat {}", seat + 1)
            }
        } else {
            format!("{role} · {}", seat + 1)
        };
        draw_seat(
            p,
            pos,
            &view.seats[seat],
            &label,
            view.turn == Some(seat) && view.phase == Phase::Playing,
            scale,
            116.0 * sx,
        );
    }
    let side_width = r.width() * 0.29;
    let side_height = 90.0 * sy;
    for (offset, area) in [
        (
            2,
            Rect::from_center_size(
                Pos2::new(center.x, r.top() + (109.0 * sy).max(70.0)),
                Vec2::new(330.0 * sx, side_height),
            ),
        ),
        (
            3,
            Rect::from_min_size(
                Pos2::new(r.left() + 125.0 * sx, center.y - side_height / 2.0),
                Vec2::new(side_width, side_height),
            ),
        ),
        (
            1,
            Rect::from_min_size(
                Pos2::new(
                    r.right() - 125.0 * sx - side_width,
                    center.y - side_height / 2.0,
                ),
                Vec2::new(side_width, side_height),
            ),
        ),
        (
            0,
            Rect::from_center_size(
                Pos2::new(center.x, r.bottom() - (102.0 * sy).max(70.0)),
                Vec2::new(330.0 * sx, side_height),
            ),
        ),
    ] {
        let seat = (mine + offset) % 4;
        if let Some(played) = &view.plays[seat] {
            draw_play(
                p,
                area,
                &played.cards,
                view.level,
                view.trick.as_ref().is_some_and(|t| t.seat == seat),
                played.combo.label(),
                scale,
            );
        } else if view.passed[seat] {
            p.text(
                area.center(),
                Align2::CENTER_CENTER,
                "Pass",
                FontId::proportional((15.0 * scale).max(9.0)),
                PEWTER,
            );
        }
    }
    if view.plays.iter().all(Option::is_none)
        && matches!(view.phase, Phase::Playing | Phase::Tribute)
    {
        p.text(
            center,
            Align2::CENTER_CENTER,
            if view.phase == Phase::Tribute {
                "Tribute exchange"
            } else {
                "A fresh trick"
            },
            FontId::proportional((16.0 * scale).max(10.0)),
            PEWTER,
        );
    }
}

fn status_text(v: &View) -> String {
    match v.phase {
        Phase::Waiting => "Waiting for a player to sit down".into(),
        Phase::GameOver => "Game complete".into(),
        Phase::HandOver => "Next hand starts automatically".into(),
        Phase::Tribute => {
            if v.return_options.is_empty() {
                "Waiting for return cards".into()
            } else {
                "Return one highlighted card".into()
            }
        }
        Phase::Playing => {
            if v.your_seat.is_some() && v.turn == v.your_seat {
                v.trick.as_ref().map_or_else(
                    || "Your turn · lead any combination".into(),
                    |p| format!("Your turn · beat {} or pass", p.combo.label()),
                )
            } else {
                format!(
                    "Waiting for {}",
                    v.turn.map_or("the table", |s| v.seats[s].name.as_str())
                )
            }
        }
    }
}
fn draw_seat(
    p: &egui::Painter,
    pos: Pos2,
    seat: &crate::table::SeatView,
    role: &str,
    active: bool,
    scale: f32,
    max_name_width: f32,
) {
    let gap = (21.0 * scale).max(13.0);
    let name_font = theme::label_font((16.0 * scale).max(11.0));
    let color = if active { ICE } else { SILVER };
    let mut name = seat.name.clone();
    let mut count = name.chars().count();
    while count > 0
        && p.layout_no_wrap(name.clone(), name_font.clone(), color)
            .size()
            .x
            > max_name_width
    {
        count -= 1;
        name = format!("{}…", seat.name.chars().take(count).collect::<String>());
    }
    p.text(
        pos - Vec2::new(0.0, gap),
        Align2::CENTER_CENTER,
        role,
        theme::label_font((11.0 * scale).max(9.0)),
        PEWTER,
    );
    p.text(pos, Align2::CENTER_CENTER, name, name_font, color);
    p.text(
        pos + Vec2::new(0.0, gap),
        Align2::CENTER_CENTER,
        format!(
            "{} cards{}",
            seat.count,
            if seat.bot { " · bot" } else { "" }
        ),
        FontId::proportional((11.0 * scale).max(8.0)),
        PEWTER,
    );
    if active {
        let gem = pos - Vec2::new((max_name_width / 2.0).min(38.0) + 4.0, 0.0);
        p.add(egui::Shape::convex_polygon(
            vec![
                gem - Vec2::new(0.0, 4.0),
                gem + Vec2::new(3.0, 0.0),
                gem + Vec2::new(0.0, 4.0),
                gem - Vec2::new(3.0, 0.0),
            ],
            ICE,
            Stroke::new(0.6_f32, SILVER),
        ));
    }
}
fn draw_play(
    p: &egui::Painter,
    area: Rect,
    cards: &[Card],
    level: u8,
    winning: bool,
    label: &str,
    scale: f32,
) {
    let size = Vec2::new(41.0, 59.0) * scale;
    let step = ((area.width() - size.x) / cards.len().saturating_sub(1).max(1) as f32)
        .max(1.0)
        .min(24.0 * scale);
    let width = size.x + step * cards.len().saturating_sub(1) as f32;
    let card_top = area.center().y - size.y / 2.0 + 4.0 * scale;
    if winning {
        p.rect_stroke(
            Rect::from_min_max(
                Pos2::new(area.center().x - width / 2.0 - 4.0, card_top - 15.0),
                Pos2::new(area.center().x + width / 2.0 + 4.0, card_top + size.y + 4.0),
            ),
            6.0,
            Stroke::new(1.0_f32, PEWTER),
            egui::StrokeKind::Inside,
        );
    }
    p.text(
        Pos2::new(area.center().x, card_top - 8.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional((10.0 * scale).max(8.0)),
        if winning { SILVER } else { PEWTER },
    );
    for (i, c) in cards.iter().enumerate() {
        let rect = Rect::from_min_size(
            Pos2::new(area.center().x - width / 2.0 + i as f32 * step, card_top),
            size,
        );
        draw_card(p, rect, *c, level, false, false, false);
    }
}
fn card_scale(size: Vec2) -> f32 {
    (size.y / 76.0).min(size.x / 51.0).clamp(0.55, 1.0)
}

fn draw_card(
    p: &egui::Painter,
    r: Rect,
    c: Card,
    level: u8,
    selected: bool,
    eligible: bool,
    dim: bool,
) {
    let scale = card_scale(r.size());
    let top = if dim {
        Color32::from_rgb(20, 22, 25)
    } else if selected {
        Color32::from_rgb(26, 57, 98)
    } else {
        CARD_TOP
    };
    p.add(theme::surface(r, 5.0, top, OBSIDIAN));
    p.rect_stroke(
        r.shrink(1.0),
        4.0,
        Stroke::new(1.0_f32, Color32::from_black_alpha(90)),
        egui::StrokeKind::Inside,
    );
    p.rect_stroke(
        r,
        5.0,
        Stroke::new(
            if selected || eligible {
                2.0_f32
            } else {
                1.0_f32
            },
            if selected || eligible { ICE } else { HAIRLINE },
        ),
        egui::StrokeKind::Inside,
    );
    let ink = if dim {
        Color32::from_rgb(101, 108, 117)
    } else {
        SILVER
    };
    let suit_ink = if dim {
        ink
    } else if c.is_wild(level) {
        Color32::from_rgb(191, 219, 254)
    } else if c.is_red() {
        ICE
    } else {
        ink
    };
    // A standard corner index remains visible when cards overlap from the right.
    p.text(
        r.left_top() + Vec2::new(4.0, 4.0) * scale,
        Align2::LEFT_TOP,
        if c.is_joker() {
            if c.rank == 16 {
                "B"
            } else {
                "S"
            }
        } else {
            c.rank_label()
        },
        FontId::proportional(16.0 * scale),
        ink,
    );
    p.text(
        r.left_top() + Vec2::new(4.0, 23.0) * scale,
        Align2::LEFT_TOP,
        if c.is_joker() { "J" } else { c.suit_label() },
        FontId::proportional(19.0 * scale),
        suit_ink,
    );
    if c.is_joker() {
        p.text(
            Pos2::new(r.center().x, r.top() + 52.0 * scale),
            Align2::CENTER_CENTER,
            if c.rank == 16 { "BIG" } else { "SMALL" },
            FontId::proportional(10.0 * scale),
            ink,
        );
        p.text(
            Pos2::new(r.center().x, r.top() + 65.0 * scale),
            Align2::CENTER_CENTER,
            "JOKER",
            FontId::proportional(9.0 * scale),
            ink,
        );
    } else {
        p.text(
            r.right_bottom() - Vec2::new(4.0, 5.0) * scale,
            Align2::RIGHT_BOTTOM,
            c.suit_label(),
            FontId::proportional(19.0 * scale),
            suit_ink,
        );
        if c.rank == level && r.width() >= 29.0 {
            p.text(
                r.center() + Vec2::new(0.0, 5.0),
                Align2::CENTER_CENTER,
                if c.is_wild(level) { "WILD" } else { "LEVEL" },
                FontId::proportional((8.0 * scale).max(6.0)),
                if dim { ink } else { PEWTER },
            );
        }
    }
}

pub fn demo_view() -> View {
    let mut t = Table::default();
    let now = Instant::now();
    let _ = t.apply(
        1,
        Command::Join {
            version: PROTOCOL_VERSION,
            name: "Alex".into(),
        },
        now,
    );
    let deck = crate::cards::deck();
    for seat in 0..4 {
        let cards: Vec<_> = (0..5)
            .map(|i| {
                deck.iter()
                    .find(|c| {
                        c.rank == 6 + i
                            && c.suit
                                == [
                                    crate::cards::Suit::Clubs,
                                    crate::cards::Suit::Hearts,
                                    crate::cards::Suit::Spades,
                                    crate::cards::Suit::Diamonds,
                                ][seat]
                    })
                    .copied()
                    .unwrap()
            })
            .collect();
        t.game.plays[seat] = Some(crate::game::Played {
            seat,
            combo: classify(&cards, 2).unwrap(),
            cards,
        });
    }
    t.game.trick = t.game.plays[3].clone();
    t.game.turn = Some(0);
    let mut v = t.view(1);
    v.seats[1].name = "Blair".into();
    v.seats[2].name = "Casey".into();
    v.seats[3].name = "Drew".into();
    v
}

#[cfg(test)]
mod responsive_tests {
    use super::*;

    fn app(view: Option<View>) -> GuandanApp {
        GuandanApp {
            settings: Settings::default(),
            connection: None,
            host: None,
            view,
            selected: BTreeSet::new(),
            error: None,
            status: String::new(),
            show_log: false,
            show_rules: false,
            endpoint: String::new(),
            connected: true,
            demo: false,
            discovery: None,
        }
    }
    fn frame(
        app: &mut GuandanApp,
        ctx: &egui::Context,
        size: Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        theme::install_fonts(ctx);
        let time = ctx.input(|i| i.time) + 1.0 / 60.0;
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                time: Some(time),
                events,
                focused: true,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    }
    fn settle(app: &mut GuandanApp, ctx: &egui::Context, size: Vec2) {
        for _ in 0..4 {
            frame(app, ctx, size, Vec::new());
        }
    }
    fn geometry(ctx: &egui::Context, key: &'static str) -> (Rect, Rect) {
        ctx.data(|d| d.get_temp(egui::Id::new(("control", key))))
            .unwrap_or_else(|| panic!("Missing control: {key}"))
    }
    fn visible(ctx: &egui::Context, key: &'static str, size: Vec2) -> bool {
        let (rect, clip) = geometry(ctx, key);
        Rect::from_min_size(Pos2::ZERO, size)
            .expand(0.5)
            .contains_rect(rect)
            && clip.expand(0.5).contains_rect(rect)
    }
    fn assert_controls(ctx: &egui::Context, keys: &[&'static str], size: Vec2) {
        for key in keys {
            assert!(
                visible(ctx, key, size),
                "{key} clipped at {size:?}: {:?}",
                geometry(ctx, key)
            );
        }
    }
    fn click(app: &mut GuandanApp, ctx: &egui::Context, size: Vec2, pos: Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                size,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }
    fn tribute_view() -> View {
        let mut table = Table::default();
        table
            .apply(
                1,
                Command::Join {
                    version: PROTOCOL_VERSION,
                    name: "Alex".into(),
                },
                Instant::now(),
            )
            .unwrap();
        table.game.team_levels[0] = 7;
        table.game.start_hand();
        let deck = crate::cards::deck();
        table.game.hands[0] = deck
            .iter()
            .copied()
            .filter(|c| c.uid < 54 && (c.rank == 7 || c.rank == 2 || c.rank == 9))
            .collect();
        table.game.hands[3] = vec![*deck.iter().find(|c| c.rank == 14).unwrap()];
        table.game.begin_tribute(&[0, 1, 2, 3]);
        table.view(1)
    }
    #[test]
    fn controls_reflow_after_live_window_resizes() {
        let ctx = egui::Context::default();
        let mut app = app(Some(demo_view()));
        app.host = Some(ServerHandle::start("0.0.0.0:0".parse().unwrap()).unwrap());
        app.settings.tailnet_ip = "100.100.100.100".into();
        app.error=Some("A deliberately long connection or invalid-play explanation that should wrap and scroll instead of moving the action controls off the bottom of the window.".repeat(3));
        for size in [
            [1200.0, 820.0],
            [800.0, 480.0],
            [420.0, 340.0],
            [640.0, 360.0],
            [1024.0, 600.0],
        ] {
            let size = Vec2::from(size);
            settle(&mut app, &ctx, size);
            assert_controls(
                &ctx,
                &[
                    "log-toggle",
                    "rules",
                    "leave",
                    "copy-invite",
                    "play",
                    "pass",
                    "clear",
                ],
                size,
            );
            let view = app.view.as_ref().unwrap();
            for card in &view.your_hand {
                let (rect, clip): (Rect, Rect) = ctx
                    .data(|d| d.get_temp(egui::Id::new(("hand-card", card.uid))))
                    .unwrap();
                assert!(
                    clip.top() <= rect.top() && clip.bottom() >= rect.bottom(),
                    "card {} vertically clipped at {size:?}",
                    card.uid
                );
            }
        }
    }

    #[test]
    fn rank_and_suit_indices_stay_exposed_and_every_card_is_reachable() {
        let mut view = demo_view();
        view.your_hand = crate::cards::deck()
            .into_iter()
            .filter(|c| (9..=14).contains(&c.rank) || c.is_wild(2) || c.rank == 16)
            .take(27)
            .collect();
        view.your_hand.sort_by_key(|c| (c.power(2), c.rank, c.uid));
        view.level = 2;
        let cards = view.your_hand.clone();
        for size in [
            [1200.0, 820.0],
            [1000.0, 700.0],
            [800.0, 480.0],
            [420.0, 340.0],
        ] {
            for selected in [false, true] {
                let size = Vec2::from(size);
                let ctx = egui::Context::default();
                let mut app = app(Some(view.clone()));
                if selected {
                    app.selected.extend(cards.iter().step_by(2).map(|c| c.uid));
                }
                settle(&mut app, &ctx, size);
                let mut reached = BTreeSet::new();
                for _ in 0..20 {
                    let output = frame(&mut app, &ctx, size, Vec::new());
                    for card in &cards {
                        let (exposed, clip): (Rect, Rect) = ctx
                            .data(|d| d.get_temp(egui::Id::new(("hand-card", card.uid))))
                            .unwrap();
                        if !clip.expand(0.5).contains_rect(exposed) {
                            continue;
                        }
                        reached.insert(card.uid);
                        let labels = if card.is_joker() {
                            [if card.rank == 16 { "B" } else { "S" }, "J"]
                        } else {
                            [card.rank_label(), card.suit_label()]
                        };
                        for label in labels {
                            assert!(
                                output.shapes.iter().any(|shape| {
                                    let egui::epaint::Shape::Text(text) = &shape.shape else {
                                        return false;
                                    };
                                    text.galley.job.text == label
                                        && text.pos.y < exposed.top() + exposed.height() * 0.45
                                        && exposed
                                            .expand(0.5)
                                            .contains_rect(text.visual_bounding_rect())
                                        && shape
                                            .clip_rect
                                            .expand(0.5)
                                            .contains_rect(text.visual_bounding_rect())
                                }),
                                "{card}: index {label} covered at {size:?}, selected={selected}"
                            );
                        }
                    }
                    let (_, clip): (Rect, Rect) = ctx
                        .data(|d| d.get_temp(egui::Id::new(("hand-card", cards[0].uid))))
                        .unwrap();
                    frame(
                        &mut app,
                        &ctx,
                        size,
                        vec![
                            egui::Event::PointerMoved(clip.center()),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: Vec2::new(-40.0, 0.0),
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
                assert_eq!(reached.len(), cards.len(), "unreachable cards at {size:?}");
                assert_controls(&ctx, &["play", "pass", "clear"], size);
            }
        }
    }
    #[test]
    fn lobby_controls_are_reachable_by_scrolling_in_small_windows() {
        for size in [
            [420.0, 340.0],
            [640.0, 360.0],
            [800.0, 480.0],
            [1200.0, 820.0],
        ] {
            let size = Vec2::from(size);
            let ctx = egui::Context::default();
            let mut app = app(None);
            settle(&mut app, &ctx, size);
            let mut reached = std::collections::BTreeSet::new();
            for _ in 0..35 {
                for key in ["solo", "detect-ip", "host", "join", "lobby-rules"] {
                    if visible(&ctx, key, size) {
                        reached.insert(key);
                    }
                }
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![
                        egui::Event::PointerMoved(Pos2::new(size.x - 25.0, size.y / 2.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: Vec2::new(0.0, -45.0),
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            assert_eq!(
                reached.len(),
                5,
                "unreachable lobby controls at {size:?}: {reached:?}"
            );
        }
    }
    #[test]
    fn tribute_controls_and_card_selection_work_in_small_windows() {
        let size = Vec2::new(420.0, 340.0);
        let ctx = egui::Context::default();
        let view = tribute_view();
        let eligible = view.return_options[0];
        let trump = view.your_hand.iter().find(|c| c.rank == 7).unwrap().uid;
        let mut app = app(Some(view));
        settle(&mut app, &ctx, size);
        assert_controls(
            &ctx,
            &["return", "clear", "leave", "log-toggle", "rules"],
            size,
        );
        let (rect, _): (Rect, Rect) = ctx
            .data(|d| d.get_temp(egui::Id::new(("hand-card", eligible))))
            .unwrap();
        click(&mut app, &ctx, size, rect.left_top() + Vec2::new(3.0, 5.0));
        assert_eq!(app.selected, BTreeSet::from([eligible]));
        let (rect, _): (Rect, Rect) = ctx
            .data(|d| d.get_temp(egui::Id::new(("hand-card", trump))))
            .unwrap();
        click(&mut app, &ctx, size, rect.left_top() + Vec2::new(3.0, 5.0));
        assert_eq!(app.selected, BTreeSet::from([eligible]));
        settle(&mut app, &ctx, Vec2::new(640.0, 360.0));
        assert_eq!(app.selected, BTreeSet::from([eligible]));
        assert_controls(&ctx, &["return", "clear"], Vec2::new(640.0, 360.0));
    }
    #[test]
    fn log_can_be_opened_and_closed_without_fullscreen() {
        let size = Vec2::new(420.0, 340.0);
        let ctx = egui::Context::default();
        let mut app = app(Some(demo_view()));
        settle(&mut app, &ctx, size);
        click(
            &mut app,
            &ctx,
            size,
            geometry(&ctx, "log-toggle").0.center(),
        );
        assert!(app.show_log);
        settle(&mut app, &ctx, size);
        assert_controls(&ctx, &["log-close"], size);
        click(&mut app, &ctx, size, geometry(&ctx, "log-close").0.center());
        assert!(!app.show_log);
        settle(&mut app, &ctx, size);
        assert_controls(&ctx, &["play", "pass", "clear"], size);
    }
}
