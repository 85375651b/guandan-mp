use crate::i18n::text as tr;
use crate::{
    cards::{rank_label, Card, Suit},
    net::{Connection, Event, ServerHandle},
    rules::classify,
    table::{Command, Hint, Message, Phase, Table, View, PROTOCOL_VERSION},
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
    let response = theme::ghost_button(ui, tr(label), enabled);
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
            name: "Player 玩家".into(),
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum HelpStatus {
    Pending,
    Cards,
    Pass,
}

#[derive(Clone, Copy)]
struct PlayHelp {
    request_id: u64,
    hand_id: u64,
    revision: u64,
    seat: usize,
    status: HelpStatus,
}
impl PlayHelp {
    fn matches(&self, view: &View) -> bool {
        view.hand_id == self.hand_id
            && view.revision == self.revision
            && view.your_seat == Some(self.seat)
            && view.turn == Some(self.seat)
            && view.phase == Phase::Playing
    }
}

pub struct GuandanApp {
    settings: Settings,
    connection: Option<Connection>,
    host: Option<ServerHandle>,
    view: Option<View>,
    selected: BTreeSet<u8>,
    help: Option<PlayHelp>,
    next_help_request: u64,
    straight_flush_view: bool,
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
            help: None,
            next_help_request: 0,
            straight_flush_view: false,
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
            app.status = tr("Preview · native interface").into();
            #[cfg(feature = "screenshot")]
            {
                app.straight_flush_view =
                    std::env::var_os("GUANDAN_SCREENSHOT_FLUSH_VIEW").is_some();
            }
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
                    self.status = tr("Connected · joining the table…").into();
                }
                Event::Disconnected(error) => {
                    if self.help.is_some() {
                        self.clear_selection();
                    }
                    self.connected = false;
                    self.error = Some(error);
                    self.status = tr("Disconnected").into();
                }
                Event::Message(Message::Error(error)) => self.error = Some(error),
                Event::Message(Message::State(view)) => self.set_view(*view),
                Event::Message(Message::Hint(hint)) => self.receive_hint(hint),
                Event::Message(Message::Pong) => {}
            }
        }
    }
    fn clear_selection(&mut self) {
        self.selected.clear();
        self.help = None;
    }
    fn set_view(&mut self, view: View) {
        if self
            .view
            .as_ref()
            .is_none_or(|old| old.hand_id != view.hand_id || old.phase != view.phase)
            || self.help.is_some_and(|help| !help.matches(&view))
        {
            self.clear_selection();
        }
        self.selected.retain(|uid| {
            view.your_hand.iter().any(|c| c.uid == *uid)
                && (view.phase != Phase::Tribute || view.return_options.contains(uid))
        });
        if view.your_seat.is_some() {
            self.status = tr("Connected").into();
        }
        self.view = Some(view);
    }
    fn help_label(&self) -> &'static str {
        if self.help.is_some() || !self.selected.is_empty() {
            "cancel"
        } else {
            "help"
        }
    }
    fn toggle_help(&mut self, view: &View) {
        if self.help_label() == "cancel" {
            self.clear_selection();
            return;
        }
        let Some(seat) = view.your_seat else {
            return;
        };
        if !self.connected || view.phase != Phase::Playing || view.turn != Some(seat) {
            return;
        }
        self.next_help_request = self.next_help_request.wrapping_add(1);
        let command = Command::Hint {
            hand_id: view.hand_id,
            request_id: self.next_help_request,
        };
        // Socket I/O and the host's heuristic run away from the UI thread.
        let result = self
            .connection
            .as_ref()
            .ok_or_else(|| tr("Connect to a table before asking for help.").to_string())
            .and_then(|c| c.send(command));
        match result {
            Ok(()) => {
                self.help = Some(PlayHelp {
                    request_id: self.next_help_request,
                    hand_id: view.hand_id,
                    revision: view.revision,
                    seat,
                    status: HelpStatus::Pending,
                });
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn receive_hint(&mut self, hint: Hint) {
        let Some(help) = self.help else {
            return;
        };
        if help.status != HelpStatus::Pending || hint.request_id != help.request_id {
            return;
        }
        let Some(view) = &self.view else {
            return;
        };
        if !self.connected
            || !help.matches(view)
            || hint.hand_id != help.hand_id
            || hint.revision != help.revision
        {
            self.clear_selection();
            return;
        }
        match hint.cards {
            Ok(uids) => {
                let cards: Vec<_> = view
                    .your_hand
                    .iter()
                    .copied()
                    .filter(|c| uids.contains(&c.uid))
                    .collect();
                let valid = cards.len() == uids.len()
                    && if cards.is_empty() {
                        view.trick.is_some()
                    } else {
                        classify(&cards, view.level)
                            .is_some_and(|c| c.beats(view.trick.as_ref().map(|p| &p.combo)))
                    };
                if !valid {
                    self.clear_selection();
                    self.error =
                        Some(tr("The suggestion is no longer legal. Ask for help again.").into());
                    return;
                }
                self.selected = uids.into_iter().collect();
                self.help = Some(PlayHelp {
                    status: if self.selected.is_empty() {
                        HelpStatus::Pass
                    } else {
                        HelpStatus::Cards
                    },
                    ..help
                });
            }
            Err(error) => {
                self.clear_selection();
                self.error = Some(error);
            }
        }
    }
    fn connect(&mut self, address: String) {
        self.straight_flush_view = false;
        self.connection = Some(Connection::connect(
            address.clone(),
            self.settings.name.clone(),
        ));
        self.endpoint = address;
        self.connected = false;
        self.view = None;
        self.clear_selection();
        self.error = None;
        self.status = tr("Connecting…").into();
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
                    "Could not host: {error}. Try a different port or close the other host. / 无法创建房间：{error}。请更换端口或关闭其他房主程序。"
                ))
            }
        }
    }
    fn leave(&mut self) {
        self.straight_flush_view = false;
        self.connection = None;
        self.host = None;
        self.view = None;
        self.clear_selection();
        self.connected = false;
        self.error = None;
        self.status.clear();
        self.demo = false;
    }
    fn send(&mut self, command: Command) {
        if let Some(c) = &self.connection {
            match c.send(command) {
                Ok(()) => {
                    self.clear_selection();
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
                        ui.label(RichText::new(tr("A table for four. Wherever your friends are.")).color(PEWTER));
                    });
                    ui.add_space(12.0);
                    let width=ui.available_width().min(820.0);
                    ui.vertical_centered(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(width,0.0),egui::Layout::top_down(egui::Align::Min),|ui| {
                            ui.label(tr("Your name"));
                            theme::input(ui, egui::TextEdit::singleline(&mut self.settings.name).char_limit(20).desired_width(ui.available_width()));
                            ui.add_space(8.0);
                            theme::container().show(ui,|ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(tr("Play on this computer")).heading().color(SILVER));
                                ui.label(tr("A fresh game with three bots. Works entirely offline."));
                                if control(ui,"solo",tr("Start solo game"),true).clicked() {self.start_host(true);}
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
                            ui.label(RichText::new(tr("Empty seats are bots. Friends can take an open seat at any time.")).color(PEWTER));
                            if let Some(error)=&self.error {ui.colored_label(ERROR,error);}
                            if control(ui,"lobby-rules",tr("Read the table rules"),true).clicked() {self.show_rules=true;}
                        });
                    });
                });
            });
    }
    fn host_form(&mut self, ui: &mut egui::Ui) {
        theme::container().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr("Host a table")).heading().color(SILVER));
            ui.label(tr("Friends join you through Tailscale or your LAN."));
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("TCP port"));
                ui.add(egui::DragValue::new(&mut self.settings.port).range(1024..=65535));
            });
            ui.label(tr("Your Tailscale IP (for invitations)"));
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
                    tr("Detect Tailscale IP"),
                    self.discovery.is_none(),
                )
                .clicked()
                {
                    self.detect_ip();
                }
                if control(ui, "host", tr("Host game"), true).clicked() {
                    self.start_host(false);
                }
            });
        });
    }
    fn join_form(&mut self, ui: &mut egui::Ui) {
        theme::container().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr("Join friends")).heading().color(SILVER));
            ui.label(tr("Enter the host's address or paste an invitation."));
            ui.label(tr("Host address"));
            theme::input(
                ui,
                egui::TextEdit::singleline(&mut self.settings.address)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("100.x.y.z:3000")
                    .desired_width(ui.available_width()),
            );
            ui.label(
                RichText::new(tr(
                    "Everyone runs this desktop app and connects to Tailscale.",
                ))
                .color(PEWTER)
                .small(),
            );
            if control(ui, "join", tr("Join game"), true).clicked() {
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
                        RichText::new(format!("Team A A队 · {}", rank_label(view.team_levels[0])))
                            .color(ICE)
                            .font(FontId::proportional(14.0 * layout.scale)),
                    );
                    ui.label(
                        RichText::new(format!("Team B B队 · {}", rank_label(view.team_levels[1])))
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
                        &format!("Wild 逢人配 · {} ", rank_label(view.level)),
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
                            tr("Hide log")
                        } else {
                            tr("Play log")
                        },
                        true,
                    )
                    .clicked()
                    {
                        self.show_log = !self.show_log;
                    }
                    if control(ui, "rules", tr("Rules"), true).clicked() {
                        self.show_rules = !self.show_rules;
                    }
                    if control(
                        ui,
                        "leave",
                        if self.host.is_some() {
                            tr("Close table")
                        } else {
                            tr("Leave")
                        },
                        true,
                    )
                    .clicked()
                    {
                        self.leave();
                    }
                    ui.label(
                        RichText::new(if self.demo {
                            tr("Preview")
                        } else if self.connected {
                            tr("Connected")
                        } else {
                            tr("Offline")
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
                                    RichText::new(format!(
                                        "On your tailnet 组网邀请: {invitation}"
                                    ))
                                    .small()
                                    .color(PEWTER),
                                )
                                .wrap(),
                            );
                            if control(ui, "copy-invite", tr("Copy invitation"), true).clicked() {
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
        let your_turn = self.connected
            && view.phase == Phase::Playing
            && view.your_seat.is_some()
            && view.turn == view.your_seat;
        let returning =
            self.connected && view.phase == Phase::Tribute && !view.return_options.is_empty();
        // Allocate actions first at the bottom. A tall suit grid can only scroll
        // its own viewport, never push the toggle or play controls off screen.
        let actions = egui::TopBottomPanel::bottom("hand-actions")
            .frame(
                egui::Frame::new()
                    .fill(OBSIDIAN)
                    .inner_margin(egui::Margin::symmetric(layout.margin as i8, 4)),
            )
            .show(ctx, |ui| {
                let selected_cards: Vec<_> = view
                    .your_hand
                    .iter()
                    .copied()
                    .filter(|c| self.selected.contains(&c.uid))
                    .collect();
                let selected_combo = classify(&selected_cards, view.level);
                ui.horizontal_wrapped(|ui| {
                    if view.phase == Phase::Tribute {
                        if control(
                            ui,
                            "return",
                            tr("Return selected card"),
                            returning && self.selected.len() == 1,
                        )
                        .clicked()
                        {
                            self.send(Command::Return {
                                hand_id: view.hand_id,
                                card: *self.selected.first().unwrap(),
                            });
                        }
                    } else {
                        let legal = your_turn
                            && selected_combo
                                .as_ref()
                                .is_some_and(|c| c.beats(view.trick.as_ref().map(|p| &p.combo)));
                        if control(ui, "play", tr("Play selected"), legal).clicked() {
                            self.send(Command::Play {
                                hand_id: view.hand_id,
                                cards: self.selected.iter().copied().collect(),
                            });
                        }
                        if control(ui, "pass", tr("Pass"), your_turn && view.trick.is_some())
                            .clicked()
                        {
                            self.send(Command::Pass {
                                hand_id: view.hand_id,
                            });
                        }
                    }
                    if control(
                        ui,
                        "clear",
                        tr("Clear selection"),
                        !self.selected.is_empty(),
                    )
                    .clicked()
                    {
                        self.clear_selection();
                    }
                    if control(
                        ui,
                        "help",
                        self.help_label(),
                        your_turn || self.help.is_some() || !self.selected.is_empty(),
                    )
                    .clicked()
                    {
                        self.toggle_help(view);
                    }
                    if control(ui, "flush-view", self.hand_view_label(), true).clicked() {
                        self.straight_flush_view = !self.straight_flush_view;
                    }
                    if view.phase == Phase::GameOver
                        && control(ui, "new-game", tr("New game"), true).clicked()
                    {
                        self.send(Command::NewGame);
                    }
                    if !self.connected
                        && !self.demo
                        && control(ui, "reconnect", tr("Reconnect"), true).clicked()
                    {
                        self.connect(self.endpoint.clone());
                    }
                    if !selected_cards.is_empty() && view.phase != Phase::Tribute {
                        ui.label(
                            selected_combo
                                .as_ref()
                                .map_or(tr("Not a legal combination"), |c| tr(c.label())),
                        );
                    }
                    if let Some(help) = self.help {
                        match help.status {
                            HelpStatus::Pending => {
                                ui.small(tr("Finding a level 3 suggestion…"));
                            }
                            HelpStatus::Pass => {
                                ui.colored_label(ICE, tr("Level 3: pass recommended."));
                            }
                            HelpStatus::Cards => {}
                        }
                    }
                });
            });
        let hand_fraction = if self.straight_flush_view { 0.54 } else { 0.68 };
        let limit = (screen.y * hand_fraction - actions.response.rect.height())
            .min(ctx.available_rect().height())
            .max(70.0);
        let height = if self.straight_flush_view {
            (screen.y * 0.066).clamp(40.0, 50.0)
        } else {
            layout.card_height
        };
        let card_size = Vec2::new(height * 51.0 / 76.0, height);
        let lift = height * if self.straight_flush_view { 0.08 } else { 0.16 };
        let min_cards = height + lift + 14.0 + if self.straight_flush_view { 20.0 } else { 0.0 };
        let wanted = if self.straight_flush_view {
            20.0 + 4.0 * (height + lift + 4.0) + 14.0
        } else {
            min_cards
        };
        let spacing = ctx.style().spacing.item_spacing.y;
        let info_limit = (screen.y * 0.18)
            .clamp(
                32.0,
                if self.straight_flush_view {
                    52.0
                } else {
                    120.0
                },
            )
            .min((limit - 2.0 * layout.margin - 2.0 * spacing - min_cards).max(16.0));
        // Fit the suit rows instead of filling the whole height allowance.
        // The table receives the reclaimed space above this bottom panel.
        let panel_height = (info_limit + wanted + 2.0 * layout.margin + 2.0 * spacing).min(limit);
        egui::TopBottomPanel::bottom("hand")
            .exact_height(panel_height)
            .frame(
                egui::Frame::new()
                    .fill(OBSIDIAN)
                    .inner_margin(layout.margin),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("hand-info")
                    .max_height(info_limit)
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        self.hand_information(ui, view, your_turn, returning, layout)
                    });
                let available = ui.available_height().max(24.0);
                egui::ScrollArea::both()
                    .drag_to_scroll(false)
                    .id_salt(if self.straight_flush_view {
                        "suit-hand-scroll"
                    } else {
                        "hand-scroll"
                    })
                    .max_height(available.min(wanted))
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if self.straight_flush_view {
                            self.suit_hand(ui, view, card_size, lift);
                        } else {
                            self.regular_hand(ui, view, card_size, lift);
                        }
                    });
            });
    }

    fn hand_view_label(&self) -> &'static str {
        if self.straight_flush_view {
            "restore regular view"
        } else {
            "see straight flushes"
        }
    }

    fn hand_information(
        &self,
        ui: &mut egui::Ui,
        view: &View,
        your_turn: bool,
        returning: bool,
        layout: Layout,
    ) {
        if view.show_tribute {
            if let Some(t) = &view.tribute {
                let who = |s: usize| {
                    if Some(s) == view.your_seat {
                        tr("you").to_string()
                    } else {
                        view.seats[s].name.clone()
                    }
                };
                if t.rebellion {
                    ui.colored_label(
                        SILVER,
                        format!("Tribute rebellion · no exchange · {} leads / 抗贡，无需交换，由 {} 首出", who(t.leader), who(t.leader)),
                    );
                } else if returning {
                    let e = t
                        .exchanges
                        .iter()
                        .find(|e| Some(e.to) == view.your_seat)
                        .unwrap();
                    let low = view.your_hand.iter().any(|c| {
                        view.return_options.contains(&c.uid)
                            && c.rank != view.level
                            && (2..=9).contains(&c.rank)
                    });
                    ui.colored_label(
                        SILVER,
                        format!(
                            "You received {} from {}. / 你收到 {} 的贡牌 {}。{}",
                            e.tribute,
                            who(e.from),
                            who(e.from),
                            e.tribute,
                            if low {
                                tr("Choose a highlighted non-trump 2–9 to return.")
                            } else {
                                tr("No eligible 2–9: return your highlighted lowest non-wild card.")
                            }
                        ),
                    );
                } else if !t.complete {
                    ui.colored_label(
                        SILVER,
                        tr("Waiting for the other tribute recipients to choose their returns…"),
                    );
                } else {
                    ui.colored_label(
                        SILVER,
                        format!(
                            "Tribute exchange complete · {} leads / 进贡还牌完成，由 {} 首出",
                            who(t.leader),
                            who(t.leader)
                        ),
                    );
                }
                ui.label(
                    RichText::new(
                        t.exchanges
                            .iter()
                            .map(|e| {
                                format!(
                                    "{} → {}: {}{}",
                                    who(e.from),
                                    who(e.to),
                                    e.tribute,
                                    e.returned
                                        .map_or(String::new(), |c| format!(" · returned 还牌 {c}"))
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("     "),
                    )
                    .small()
                    .color(PEWTER),
                );
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "YOUR HAND 手牌 · {} cards 张",
                    view.your_hand.len()
                ))
                .small()
                .color(PEWTER),
            );
            ui.label(
                RichText::new(status_text(view))
                    .color(if your_turn { ICE } else { SILVER })
                    .font(FontId::proportional(14.0 * layout.scale)),
            );
        });
        if let Some(error) = &self.error {
            ui.colored_label(ERROR, error);
        }
        if !self.connected && !self.demo {
            ui.small(tr(
                "The host must remain running. Rejoining takes an open seat.",
            ));
        }
        if self.straight_flush_view {
            ui.label(
                RichText::new(tr(
                    "Printed ranks 2–A · Wild hearts can complete any suit · A can also be low",
                ))
                .small()
                .color(PEWTER),
            );
        }
    }

    fn regular_hand(&mut self, ui: &mut egui::Ui, view: &View, card_size: Vec2, lift: f32) {
        let min_step = 26.0 * card_scale(card_size);
        let step = ((ui.available_width() - card_size.x)
            / view.your_hand.len().saturating_sub(1).max(1) as f32)
            .clamp(min_step, 33.0);
        let width = if view.your_hand.is_empty() {
            0.0
        } else {
            step * (view.your_hand.len() - 1) as f32 + card_size.x
        };
        let (area, _) = ui.allocate_exact_size(
            Vec2::new(width.max(1.0), card_size.y + lift + 6.0),
            Sense::hover(),
        );
        for (i, card) in view.your_hand.iter().enumerate() {
            let rect = Rect::from_min_size(
                area.min
                    + Vec2::new(
                        i as f32 * step,
                        if self.selected.contains(&card.uid) {
                            1.0
                        } else {
                            lift
                        },
                    ),
                card_size,
            );
            self.hand_card(
                ui,
                view,
                *card,
                rect,
                if i + 1 == view.your_hand.len() {
                    card_size.x
                } else {
                    step
                },
            );
        }
    }

    fn suit_hand(&mut self, ui: &mut egui::Ui, view: &View, card_size: Vec2, lift: f32) {
        let label_width = 30.0;
        let duplicate_step = 26.0 * card_scale(card_size);
        let column = card_size.x + duplicate_step + 4.0;
        let row_pitch = card_size.y + lift + 4.0;
        let mut jokers: Vec<_> = view
            .your_hand
            .iter()
            .copied()
            .filter(|c| c.is_joker())
            .collect();
        jokers.sort_by_key(|c| (c.rank, c.uid));
        let suit_width = label_width + column * 13.0;
        let joker_width = if jokers.is_empty() {
            0.0
        } else {
            14.0 + card_size.x.max(38.0)
        };
        let (area, _) = ui.allocate_exact_size(
            Vec2::new(suit_width + joker_width, 20.0 + 4.0 * row_pitch),
            Sense::hover(),
        );
        for rank in 2..=14 {
            ui.painter().text(
                area.min
                    + Vec2::new(
                        label_width
                            + (rank - 2) as f32 * column
                            + (card_size.x + duplicate_step) * 0.5,
                        8.0,
                    ),
                Align2::CENTER_CENTER,
                rank_label(rank),
                FontId::proportional(10.0),
                PEWTER,
            );
        }
        for (row, suit) in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs]
            .into_iter()
            .enumerate()
        {
            let y = area.top() + 20.0 + row as f32 * row_pitch;
            let label = Card {
                uid: 0,
                rank: 3,
                suit,
            };
            ui.painter().text(
                Pos2::new(area.left() + 11.0, y + lift + card_size.y * 0.5),
                Align2::CENTER_CENTER,
                label.suit_label(),
                FontId::proportional(17.0),
                if label.is_red() { ICE } else { SILVER },
            );
            for rank in 2..=14 {
                let x = area.left() + label_width + (rank - 2) as f32 * column;
                let mut cards: Vec<_> = view
                    .your_hand
                    .iter()
                    .copied()
                    .filter(|c| c.suit == suit && c.rank == rank)
                    .collect();
                cards.sort_by_key(|c| c.uid);
                if cards.is_empty() {
                    ui.painter().rect_stroke(
                        Rect::from_min_size(Pos2::new(x, y + lift), card_size),
                        3.0,
                        Stroke::new(1.0_f32, Color32::from_rgb(27, 40, 58)),
                        egui::StrokeKind::Inside,
                    );
                }
                for (i, card) in cards.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        Pos2::new(
                            x + i as f32 * duplicate_step,
                            y + if self.selected.contains(&card.uid) {
                                1.0
                            } else {
                                lift
                            },
                        ),
                        card_size,
                    );
                    self.hand_card(
                        ui,
                        view,
                        *card,
                        rect,
                        if i + 1 == cards.len() {
                            card_size.x
                        } else {
                            duplicate_step
                        },
                    );
                }
            }
        }
        if !jokers.is_empty() {
            let x = area.left() + suit_width + 14.0;
            ui.painter().line_segment(
                [
                    Pos2::new(x - 7.0, area.top()),
                    Pos2::new(x - 7.0, area.bottom()),
                ],
                Stroke::new(1.0_f32, HAIRLINE),
            );
            ui.painter().text(
                Pos2::new(x + card_size.x * 0.5, area.top() + 8.0),
                Align2::CENTER_CENTER,
                tr("Jokers"),
                FontId::proportional(10.0),
                PEWTER,
            );
            for (i, card) in jokers.iter().enumerate() {
                let rect = Rect::from_min_size(
                    Pos2::new(
                        x,
                        area.top()
                            + 20.0
                            + i as f32 * row_pitch
                            + if self.selected.contains(&card.uid) {
                                1.0
                            } else {
                                lift
                            },
                    ),
                    card_size,
                );
                self.hand_card(ui, view, *card, rect, card_size.x);
            }
        }
    }

    fn hand_card(&mut self, ui: &mut egui::Ui, view: &View, card: Card, rect: Rect, exposed: f32) {
        let returning =
            self.connected && view.phase == Phase::Tribute && !view.return_options.is_empty();
        let your_turn = self.connected
            && view.phase == Phase::Playing
            && view.your_seat.is_some()
            && view.turn == view.your_seat;
        let selected = self.selected.contains(&card.uid);
        let eligible = !returning || view.return_options.contains(&card.uid);
        draw_card(
            ui.painter(),
            rect,
            card,
            view.level,
            selected,
            returning && eligible,
            returning && !eligible,
        );
        let hit = Rect::from_min_size(rect.min, Vec2::new(exposed, rect.height()));
        let response = ui.interact(hit, ui.id().with(("card", card.uid)), Sense::click());
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new(("hand-card", card.uid)),
                (response.rect, ui.clip_rect()),
            )
        });
        if response.clicked() && (your_turn || returning) && eligible {
            self.help = None;
            if returning {
                self.selected.clear();
                if !selected {
                    self.selected.insert(card.uid);
                }
            } else if selected {
                self.selected.remove(&card.uid);
            } else {
                self.selected.insert(card.uid);
            }
        }
        response.on_hover_text(format!(
            "{card}{}",
            if card.is_wild(view.level) {
                tr(" · wildcard · can fill a missing rank in any suit")
            } else if card.rank == view.level {
                tr(" · level trump")
            } else {
                ""
            }
        ));
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
                    tr("Game complete")
                } else {
                    tr("Hand complete")
                })
                .id(egui::Id::new("result"))
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .max_size(ctx.screen_rect().size() - Vec2::new(32.0, 48.0))
                .min_size([180.0, 100.0])
                .collapsible(false)
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.heading(format!(
                        "Team 队伍 {} {}",
                        if result.team == 0 { "A" } else { "B" },
                        if result.passed_ace {
                            tr("passes Ace!")
                        } else {
                            tr("wins the hand")
                        }
                    ));
                    for (i, s) in result.order.iter().enumerate() {
                        ui.label(format!(
                            "{}  {}",
                            if i < result.actual_finish_count {
                                format!("#{}", i + 1)
                            } else {
                                tr("Did not finish").into()
                            },
                            view.seats[*s].name
                        ));
                    }
                    if result.reset_to_two {
                        ui.colored_label(
                            SILVER,
                            tr("Three consecutive 1–4 Ace wins: back to level 2."),
                        );
                    } else if result.ace_failures > 0 {
                        ui.label(format!(
                            "Stays at Ace 保持打 A · {} / 3 consecutive 1–4 wins 连续一四名获胜",
                            result.ace_failures
                        ));
                    } else {
                        ui.label(format!(
                            "Team level 队伍级数: {}",
                            rank_label(result.level_after)
                        ));
                    }
                    if result.passed_ace {
                        if control(ui, "result-new-game", tr("Start a new game"), true).clicked() {
                            self.send(Command::NewGame);
                        }
                    } else {
                        ui.small(tr(
                            "The next deal and tribute exchange start automatically.",
                        ));
                    }
                });
            }
        }
    }
    fn rules_window(&mut self, ctx: &egui::Context) {
        egui::Window::new(tr("Table rules"))
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
            ui.heading(tr("Play log"));
            if control(ui, "log-close", tr("Close"), true).clicked() {
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
                egui::Window::new(tr("Play log"))
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
                        if control(ui, "back", tr("Back"), true).clicked() {
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
            tr("PARTNER"),
        ),
        (3, Pos2::new(r.left() + 66.0 * sx, center.y), tr("OPPONENT")),
        (
            1,
            Pos2::new(r.right() - 66.0 * sx, center.y),
            tr("OPPONENT"),
        ),
        (
            0,
            Pos2::new(center.x, r.bottom() - (31.0 * sy).max(26.0)),
            tr("YOU"),
        ),
    ] {
        let seat = (mine + offset) % 4;
        let label = if r.width() < 650.0 {
            if offset == 2 {
                tr("Partner").into()
            } else if offset == 0 {
                tr("You").into()
            } else {
                format!("Seat 座位 {}", seat + 1)
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
                tr("Pass"),
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
                tr("Tribute exchange")
            } else {
                tr("A fresh trick")
            },
            FontId::proportional((16.0 * scale).max(10.0)),
            PEWTER,
        );
    }
}

fn status_text(v: &View) -> String {
    match v.phase {
        Phase::Waiting => tr("Waiting for a player to sit down").into(),
        Phase::GameOver => tr("Game complete").into(),
        Phase::HandOver => tr("Next hand starts automatically").into(),
        Phase::Tribute => {
            if v.return_options.is_empty() {
                tr("Waiting for return cards").into()
            } else {
                tr("Return one highlighted card").into()
            }
        }
        Phase::Playing => {
            if v.your_seat.is_some() && v.turn == v.your_seat {
                v.trick.as_ref().map_or_else(
                    || tr("Your turn · lead any combination").into(),
                    |p| {
                        format!(
                            "Your turn 轮到你 · beat 压过 {} or pass 或过牌",
                            tr(p.combo.label())
                        )
                    },
                )
            } else {
                format!(
                    "Waiting for 等待 {}",
                    v.turn
                        .map_or("the table 牌桌", |s| v.seats[s].name.as_str())
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
            "{} cards 张{}",
            seat.count,
            if seat.bot { tr(" · bot") } else { "" }
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
        .min(size.x + 4.0 * scale);
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
        tr(label),
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
            if c.rank == 16 { tr("BIG") } else { tr("SMALL") },
            FontId::proportional(10.0 * scale),
            ink,
        );
        p.text(
            Pos2::new(r.center().x, r.top() + 65.0 * scale),
            Align2::CENTER_CENTER,
            tr("JOKER"),
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
                if c.is_wild(level) {
                    tr("WILD")
                } else {
                    tr("LEVEL")
                },
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

    #[test]
    fn bundled_fonts_render_chinese_in_labels_names_and_input_fields() {
        let ctx = egui::Context::default();
        let mut app = app(Some(demo_view()));
        app.view.as_mut().unwrap().seats[0].name = "小明".into();
        let output = frame(&mut app, &ctx, Vec2::new(1000.0, 700.0), Vec::new());
        let chinese: String = concat!(
            include_str!("i18n.rs"),
            include_str!("game.rs"),
            include_str!("table.rs"),
            include_str!("../RULES.txt")
        )
        .chars()
        .filter(|c| ('\u{3400}'..='\u{9fff}').contains(c))
        .collect();
        for font in [
            theme::label_font(14.0),
            FontId::proportional(14.0),
            FontId::monospace(14.0),
        ] {
            assert!(
                ctx.fonts(|f| f.has_glyphs(&font, &chinese)),
                "missing Chinese glyphs in {font:?}"
            );
        }
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(t) if t.galley.job.text == "GUANDAN 掼蛋")));
    }

    fn app(view: Option<View>) -> GuandanApp {
        GuandanApp {
            settings: Settings::default(),
            connection: None,
            host: None,
            view,
            selected: BTreeSet::new(),
            help: None,
            next_help_request: 0,
            straight_flush_view: false,
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
                    "help",
                    "flush-view",
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
    fn suit_view() -> View {
        let mut view = demo_view();
        view.your_hand = crate::cards::deck()
            .into_iter()
            .filter(|c| {
                (c.uid < 54 && (3..=7).contains(&c.rank))
                    || c.is_wild(2)
                    || (c.uid >= 54 && c.rank == 5 && c.suit == Suit::Spades)
                    || c.is_joker()
            })
            .collect();
        view.your_hand.sort_by_key(|c| (c.power(2), c.rank, c.uid));
        view.level = 2;
        view.your_seat = Some(0);
        view.turn = Some(0);
        view.phase = Phase::Playing;
        view.trick = None;
        view
    }
    fn card_geometry(ctx: &egui::Context, uid: u8) -> (Rect, Rect) {
        ctx.data(|d| d.get_temp(egui::Id::new(("hand-card", uid))))
            .unwrap()
    }
    fn toggle_hand(app: &mut GuandanApp, ctx: &egui::Context, size: Vec2) {
        click(app, ctx, size, geometry(ctx, "flush-view").0.center());
        settle(app, ctx, size);
    }
    #[test]
    fn suit_toggle_preserves_selection_and_groups_by_printed_rank() {
        let size = Vec2::new(1400.0, 900.0);
        let ctx = egui::Context::default();
        let view = suit_view();
        let cards = view.your_hand.clone();
        assert_eq!(cards.len(), 27);
        let mut app = app(Some(view));
        settle(&mut app, &ctx, size);
        assert_eq!(app.hand_view_label(), "see straight flushes");
        let first = cards[0].uid;
        let original = card_geometry(&ctx, first).0;
        click(&mut app, &ctx, size, original.left_top() + Vec2::splat(4.0));
        assert_eq!(app.selected, BTreeSet::from([first]));
        toggle_hand(&mut app, &ctx, size);
        assert!(app.straight_flush_view);
        assert_eq!(app.hand_view_label(), "restore regular view");
        assert_eq!(app.selected, BTreeSet::from([first]));
        // Clear the raised card before comparing the row baselines.
        click(&mut app, &ctx, size, geometry(&ctx, "clear").0.center());
        settle(&mut app, &ctx, size);
        let mut previous_row = None;
        for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
            let mut row: Vec<_> = cards.iter().filter(|c| c.suit == suit).collect();
            row.sort_by_key(|c| (c.rank, c.uid));
            let y = card_geometry(&ctx, row[0].uid).0.top();
            if let Some(previous) = previous_row {
                assert!(y > previous);
            }
            previous_row = Some(y);
            for card in &row {
                let (rect, clip) = card_geometry(&ctx, card.uid);
                assert!(clip.contains_rect(rect), "{card} clipped");
                assert!((rect.top() - y).abs() < 0.5);
            }
            for pair in row.windows(2) {
                assert!(
                    card_geometry(&ctx, pair[0].uid).0.left()
                        < card_geometry(&ctx, pair[1].uid).0.left()
                );
            }
        }
        // Jokers remain selectable in their own column, after all suit cards.
        let rightmost_suit = cards
            .iter()
            .filter(|c| !c.is_joker())
            .map(|c| card_geometry(&ctx, c.uid).0.right())
            .fold(0.0, f32::max);
        for card in cards.iter().filter(|c| c.is_joker()) {
            let (rect, clip) = card_geometry(&ctx, card.uid);
            assert!(clip.contains_rect(rect));
            assert!(rect.left() > rightmost_suit);
            click(&mut app, &ctx, size, rect.center());
            assert!(app.selected.contains(&card.uid));
        }
        // The two deck copies can be chosen independently, including wilds.
        for card in cards.iter().filter(|c| c.is_wild(2)) {
            let rect = card_geometry(&ctx, card.uid).0;
            click(&mut app, &ctx, size, rect.left_top() + Vec2::splat(4.0));
            assert!(app.selected.contains(&card.uid));
        }
        let selection = app.selected.clone();
        toggle_hand(&mut app, &ctx, size);
        assert!(!app.straight_flush_view);
        assert_eq!(app.hand_view_label(), "see straight flushes");
        assert_eq!(app.selected, selection);
        assert_eq!(app.view.as_ref().unwrap().your_hand, cards);
        assert_eq!(card_geometry(&ctx, first).0.left(), original.left());
    }
    #[test]
    fn suit_rows_scroll_without_hiding_actions_and_keep_tribute_restrictions() {
        let size = Vec2::new(420.0, 340.0);
        let ctx = egui::Context::default();
        let view = suit_view();
        let cards = view.your_hand.clone();
        let mut app = app(Some(view));
        settle(&mut app, &ctx, size);
        toggle_hand(&mut app, &ctx, size);
        let mut reached = BTreeSet::new();
        for row in 0..16 {
            for _ in 0..24 {
                for card in &cards {
                    let (rect, clip) = card_geometry(&ctx, card.uid);
                    if clip.expand(0.5).contains_rect(rect) {
                        reached.insert(card.uid);
                    }
                }
                assert_controls(&ctx, &["play", "pass", "clear", "flush-view"], size);
                let clip = card_geometry(&ctx, cards[0].uid).1;
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![
                        egui::Event::PointerMoved(clip.center()),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: Vec2::new(if row % 2 == 0 { -50.0 } else { 50.0 }, 0.0),
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            let clip = card_geometry(&ctx, cards[0].uid).1;
            frame(
                &mut app,
                &ctx,
                size,
                vec![
                    egui::Event::PointerMoved(clip.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, -20.0),
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(
            reached.len(),
            cards.len(),
            "unreachable suit cards: {reached:?}"
        );

        let ctx = egui::Context::default();
        let view = tribute_view();
        let eligible = view
            .your_hand
            .iter()
            .find(|c| c.suit == Suit::Spades && view.return_options.contains(&c.uid))
            .unwrap()
            .uid;
        let trump = view
            .your_hand
            .iter()
            .find(|c| c.suit == Suit::Spades && c.rank == 7)
            .unwrap()
            .uid;
        let mut app = self::app(Some(view));
        settle(&mut app, &ctx, size);
        toggle_hand(&mut app, &ctx, size);
        assert_controls(&ctx, &["return", "clear", "flush-view"], size);
        for uid in [eligible, trump] {
            let (rect, clip) = card_geometry(&ctx, uid);
            assert!(
                clip.contains_rect(rect),
                "card {uid}: {rect:?} outside {clip:?}"
            );
            click(&mut app, &ctx, size, rect.left_top() + Vec2::splat(4.0));
            assert_eq!(app.selected, BTreeSet::from([eligible]));
        }
        toggle_hand(&mut app, &ctx, size);
        assert_eq!(app.selected, BTreeSet::from([eligible]));
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

    fn pending_hint(app: &mut GuandanApp, request_id: u64, cards: Vec<u8>) -> Hint {
        let view = app.view.as_ref().unwrap();
        app.help = Some(PlayHelp {
            request_id,
            hand_id: view.hand_id,
            revision: view.revision,
            seat: view.your_seat.unwrap(),
            status: HelpStatus::Pending,
        });
        Hint {
            request_id,
            hand_id: view.hand_id,
            revision: view.revision,
            cards: Ok(cards),
        }
    }

    #[test]
    fn help_ignores_cancelled_replaced_and_outdated_replies() {
        let ctx = egui::Context::default();
        let size = Vec2::new(420.0, 340.0);
        let mut app = app(Some(suit_view()));
        let uid = app.view.as_ref().unwrap().your_hand[0].uid;
        let hint = pending_hint(&mut app, 1, vec![uid]);
        settle(&mut app, &ctx, size);
        assert_eq!(app.help_label(), "cancel");
        assert_controls(&ctx, &["play", "pass", "clear", "help", "flush-view"], size);
        click(&mut app, &ctx, size, geometry(&ctx, "help").0.center());
        app.receive_hint(hint);
        assert!(app.selected.is_empty());
        assert!(app.help.is_none());
        let invalid = pending_hint(&mut app, 6, vec![255]);
        app.receive_hint(invalid);
        assert!(app.selected.is_empty());
        assert!(app.help.is_none());
        assert!(app.error.is_some());
        let mut rejected = pending_hint(&mut app, 7, vec![uid]);
        rejected.cards = Err("Help is available during your turn after tribute.".into());
        app.receive_hint(rejected);
        assert!(app.selected.is_empty());
        assert!(app.help.is_none());
        assert_eq!(app.help_label(), "help");

        let old = pending_hint(&mut app, 2, vec![uid]);
        let hint = pending_hint(&mut app, 3, vec![uid]);
        app.receive_hint(old);
        assert_eq!(app.help.unwrap().request_id, 3);
        settle(&mut app, &ctx, size);
        // Manually choosing a card also abandons the pending suggestion.
        let rect = card_geometry(&ctx, uid).0;
        click(&mut app, &ctx, size, rect.left_top() + Vec2::splat(4.0));
        assert_eq!(app.selected, BTreeSet::from([uid]));
        assert!(app.help.is_none());
        app.receive_hint(hint);
        assert_eq!(app.selected, BTreeSet::from([uid]));
        click(&mut app, &ctx, size, geometry(&ctx, "help").0.center());
        assert!(app.selected.is_empty());

        for change_hand in [false, true] {
            let hint = pending_hint(&mut app, 4, vec![uid]);
            app.receive_hint(hint.clone());
            assert_eq!(app.selected, BTreeSet::from([uid]));
            let mut updated = app.view.clone().unwrap();
            if change_hand {
                updated.hand_id += 1;
            } else {
                updated.revision += 1;
                updated.turn = Some(1);
            }
            app.set_view(updated);
            app.receive_hint(hint);
            assert!(app.selected.is_empty());
            assert!(app.help.is_none());
            app.set_view(suit_view());
        }
        let mut hint = pending_hint(&mut app, 5, vec![uid]);
        hint.revision += 1;
        app.receive_hint(hint);
        assert!(app.selected.is_empty());
        assert!(app.help.is_none());
    }

    #[test]
    fn help_pass_advice_is_visible_and_can_be_cancelled_in_a_small_window() {
        for suit_rows in [false, true] {
            let ctx = egui::Context::default();
            let size = Vec2::new(420.0, 340.0);
            let mut app = app(Some(demo_view()));
            app.straight_flush_view = suit_rows;
            let hint = pending_hint(&mut app, 1, Vec::new());
            app.receive_hint(hint);
            assert!(app.selected.is_empty());
            assert!(app.help.is_some_and(|h| h.status == HelpStatus::Pass));
            settle(&mut app, &ctx, size);
            let output = frame(&mut app, &ctx, size, Vec::new());
            assert!(output.shapes.iter().any(|shape| {
                let egui::epaint::Shape::Text(text) = &shape.shape else {
                    return false;
                };
                text.galley.job.text == tr("Level 3: pass recommended.")
                    && shape
                        .clip_rect
                        .expand(0.5)
                        .contains_rect(text.visual_bounding_rect())
            }));
            assert_controls(&ctx, &["play", "pass", "clear", "help", "flush-view"], size);
            click(&mut app, &ctx, size, geometry(&ctx, "help").0.center());
            assert_eq!(app.help_label(), "help");
            assert!(app.help.is_none());
        }
    }

    #[test]
    fn help_button_selects_a_play_from_a_native_host_in_both_views() {
        use std::thread;
        let server = ServerHandle::start("127.0.0.1:0".parse().unwrap()).unwrap();
        // Occupy every seat so the position stays fixed while requesting advice.
        let mut clients: Vec<_> = (0..4)
            .map(|i| Connection::connect(server.address.to_string(), format!("Player {i}")))
            .collect();
        let mut views: Vec<Option<View>> = vec![None; 4];
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            for (client, view) in clients.iter().zip(&mut views) {
                for event in client.events.try_iter() {
                    match event {
                        Event::Message(Message::State(state)) => *view = Some(*state),
                        Event::Disconnected(error) | Event::Message(Message::Error(error)) => {
                            panic!("{error}")
                        }
                        _ => {}
                    }
                }
            }
            if views
                .iter()
                .all(|v| v.as_ref().is_some_and(|v| v.seats.iter().all(|s| !s.bot)))
            {
                break;
            }
            assert!(Instant::now() < deadline, "joining host timed out");
            thread::sleep(Duration::from_millis(5));
        }
        let index = views
            .iter()
            .position(|v| {
                let v = v.as_ref().unwrap();
                v.your_seat == v.turn
            })
            .unwrap();
        let view = views[index].take().unwrap();
        let before = serde_json::to_value(&view).unwrap();
        let mut app = app(Some(view));
        app.connection = Some(clients.swap_remove(index));
        let ctx = egui::Context::default();
        for (suit_rows, size) in [
            (false, Vec2::new(420.0, 340.0)),
            (true, Vec2::new(1000.0, 700.0)),
        ] {
            app.straight_flush_view = suit_rows;
            settle(&mut app, &ctx, size);
            assert_eq!(app.help_label(), "help");
            assert_controls(&ctx, &["help", "play", "pass", "clear", "flush-view"], size);
            click(&mut app, &ctx, size, geometry(&ctx, "help").0.center());
            assert!(app.help.is_some_and(|h| h.status == HelpStatus::Pending));
            let deadline = Instant::now() + Duration::from_secs(12);
            while app.help.is_some_and(|h| h.status == HelpStatus::Pending) {
                app.poll();
                assert!(app.error.is_none(), "{:?}", app.error);
                assert!(Instant::now() < deadline, "help request timed out");
                thread::sleep(Duration::from_millis(5));
            }
            assert!(app.help.is_some());
            assert_eq!(app.help_label(), "cancel");
            let view = app.view.as_ref().unwrap();
            let cards: Vec<_> = view
                .your_hand
                .iter()
                .copied()
                .filter(|c| app.selected.contains(&c.uid))
                .collect();
            if cards.is_empty() {
                assert!(view.trick.is_some());
                assert!(app.help.is_some_and(|h| h.status == HelpStatus::Pass));
            } else {
                assert!(classify(&cards, view.level)
                    .unwrap()
                    .beats(view.trick.as_ref().map(|p| &p.combo)));
                assert_eq!(cards.len(), app.selected.len());
            }
            assert_eq!(serde_json::to_value(view).unwrap(), before);
            settle(&mut app, &ctx, size);
            assert_controls(&ctx, &["help", "play", "pass", "clear", "flush-view"], size);
            click(&mut app, &ctx, size, geometry(&ctx, "help").0.center());
            assert!(app.selected.is_empty());
            assert!(app.help.is_none());
        }
    }
}
