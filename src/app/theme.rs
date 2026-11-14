//! Diamond Vault: embedded editorial type, sapphire glass, and platinum details.
mod backdrop;

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Vec2};

pub(super) const CANVAS: Color32 = Color32::from_rgb(15, 17, 21);
pub(super) const SLATE: Color32 = Color32::from_rgb(15, 23, 42);
pub(super) const OBSIDIAN: Color32 = Color32::from_rgb(10, 15, 24);
pub(super) const SILVER: Color32 = Color32::from_rgb(248, 250, 252);
pub(super) const PEWTER: Color32 = Color32::from_rgb(148, 163, 184);
pub(super) const ICE: Color32 = Color32::from_rgb(96, 165, 250);
pub(super) const HAIRLINE: Color32 = Color32::from_rgba_premultiplied(57, 58, 60, 64);
pub(super) const ERROR: Color32 = ICE;
pub(super) const CARD_TOP: Color32 = Color32::from_rgb(23, 36, 56);

pub(super) fn install_fonts(ctx: &egui::Context) {
    let key = egui::Id::new("diamond-vault-cinzel-fonts");
    if ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false) {
        return;
    }
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Cinzel".into(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Cinzel-Variable.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "Droid Sans Fallback".into(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/DroidSansFallbackFull.ttf"
        ))
        .into(),
    );
    for family in fonts.families.values_mut() {
        family.push("Droid Sans Fallback".into());
    }
    let mut family = vec!["Cinzel".into()];
    family.extend(fonts.families[&egui::FontFamily::Proportional].clone());
    fonts
        .families
        .insert(egui::FontFamily::Name("Diamond Serif".into()), family);
    ctx.set_fonts(fonts);
    ctx.data_mut(|d| d.insert_temp(key, true));
}

pub(super) fn label_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("Diamond Serif".into()))
}

pub(super) fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(PEWTER);
    v.panel_fill = CANVAS;
    v.window_fill = SLATE;
    v.extreme_bg_color = Color32::from_rgb(8, 15, 28);
    v.faint_bg_color = SLATE;
    v.code_bg_color = SLATE;
    v.hyperlink_color = ICE;
    v.warn_fg_color = ICE;
    v.error_fg_color = ERROR;
    v.window_stroke = Stroke::new(1.0_f32, HAIRLINE);
    v.window_corner_radius = 4.into();
    v.menu_corner_radius = 4.into();
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(150),
    };
    v.popup_shadow = v.window_shadow;
    v.selection.bg_fill = Color32::from_rgb(30, 58, 95);
    v.selection.stroke = Stroke::new(1.0_f32, ICE);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = 4.into();
        w.bg_fill = SLATE;
        w.weak_bg_fill = SLATE;
        w.bg_stroke = Stroke::new(1.0_f32, HAIRLINE);
        w.fg_stroke = Stroke::new(1.0_f32, SILVER);
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_fill = OBSIDIAN;
    v.widgets.noninteractive.weak_bg_fill = OBSIDIAN;
    v.widgets.inactive.bg_fill = Color32::from_rgb(12, 22, 39);
    v.widgets.inactive.weak_bg_fill = v.widgets.inactive.bg_fill;
    v.widgets.hovered.bg_fill = Color32::from_rgb(30, 58, 95);
    v.widgets.hovered.weak_bg_fill = v.widgets.hovered.bg_fill;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(145, 177, 218));
    v.widgets.active.bg_fill = Color32::from_rgb(38, 76, 130);
    v.widgets.active.weak_bg_fill = v.widgets.active.bg_fill;
    v.widgets.active.bg_stroke = Stroke::new(1.5_f32, ICE);
    v.widgets.open.bg_fill = Color32::from_rgb(23, 42, 70);
    v.widgets.open.weak_bg_fill = v.widgets.open.bg_fill;
    v
}

pub(super) fn wordmark(ui: &mut egui::Ui, size: f32) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        "GUANDAN",
        0.0,
        egui::TextFormat {
            font_id: label_font(size),
            color: SILVER,
            extra_letter_spacing: size * 0.12,
            ..Default::default()
        },
    );
    job.append(
        " 掼蛋",
        0.0,
        egui::TextFormat {
            font_id: label_font(size),
            color: SILVER,
            ..Default::default()
        },
    );
    ui.label(job);
}

pub(super) struct GlassPanel;
pub(super) fn container() -> GlassPanel {
    GlassPanel
}

impl GlassPanel {
    pub(super) fn show<R>(
        self,
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let background = ui.painter().add(egui::Shape::Noop);
        let result = egui::Frame::new()
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0_f32, HAIRLINE))
            .corner_radius(4)
            .inner_margin(16.0)
            .show(ui, content);
        let r = result.response.rect;
        ui.painter().set(
            background,
            egui::Shape::Vec(vec![
                backdrop::shape(ui.painter(), r, 4.0),
                surface(
                    r,
                    4.0,
                    Color32::from_rgba_unmultiplied(15, 23, 42, 191),
                    Color32::from_rgba_unmultiplied(30, 41, 59, 166),
                ),
            ]),
        );
        engrave(ui.painter(), r, 18.0);
        result
    }
}

fn blend(top: Color32, bottom: Color32, t: f32) -> Color32 {
    let a = top.to_array();
    let b = bottom.to_array();
    let c: [u8; 4] = std::array::from_fn(|i| egui::lerp(a[i] as f32..=b[i] as f32, t) as u8);
    Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3])
}

fn boundary(r: Rect, radius: f32) -> Vec<Pos2> {
    let radius = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
    let mut points = Vec::with_capacity(36);
    for (center, angle) in [
        (
            Pos2::new(r.right() - radius, r.top() + radius),
            -std::f32::consts::FRAC_PI_2,
        ),
        (Pos2::new(r.right() - radius, r.bottom() - radius), 0.0),
        (
            Pos2::new(r.left() + radius, r.bottom() - radius),
            std::f32::consts::FRAC_PI_2,
        ),
        (
            Pos2::new(r.left() + radius, r.top() + radius),
            std::f32::consts::PI,
        ),
    ] {
        for i in 0..=8 {
            points.push(
                center
                    + Vec2::angled(angle + i as f32 / 8.0 * std::f32::consts::FRAC_PI_2) * radius,
            );
        }
    }
    points
}

pub(super) fn surface(r: Rect, radius: f32, top: Color32, bottom: Color32) -> egui::Shape {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.center(), blend(top, bottom, 0.5));
    for pos in boundary(r, radius) {
        mesh.colored_vertex(
            pos,
            blend(
                top,
                bottom,
                ((pos.y - r.top()) / r.height().max(1.0)).clamp(0.0, 1.0),
            ),
        );
    }
    let count = mesh.vertices.len() as u32 - 1;
    for i in 1..=count {
        mesh.add_triangle(0, i, i % count + 1);
    }
    egui::Shape::mesh(mesh)
}

pub(super) fn ghost_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let background = ui.painter().add(egui::Shape::Noop);
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).color(SILVER)).fill(Color32::TRANSPARENT),
    );
    let lit = enabled && (response.hovered() || response.has_focus());
    let (top, bottom) = if !enabled {
        (Color32::from_rgb(23, 32, 47), Color32::from_rgb(12, 20, 33))
    } else if response.is_pointer_button_down_on() {
        (
            Color32::from_rgb(34, 70, 122),
            Color32::from_rgb(15, 35, 67),
        )
    } else if lit {
        (
            Color32::from_rgb(48, 101, 174),
            Color32::from_rgb(23, 52, 94),
        )
    } else {
        (Color32::from_rgb(30, 58, 95), Color32::from_rgb(15, 32, 57))
    };
    let mut shapes = vec![surface(response.rect, 4.0, top, bottom)];
    if lit {
        shapes.insert(
            0,
            egui::epaint::Shadow {
                offset: [0, 0],
                blur: 12,
                spread: 1,
                color: Color32::from_rgba_unmultiplied(59, 130, 246, 40),
            }
            .as_shape(response.rect, 4)
            .into(),
        );
    }
    ui.painter().set(background, egui::Shape::Vec(shapes));
    let r = response.rect.shrink(2.0);
    ui.painter().line_segment(
        [
            Pos2::new(r.left() + 4.0, r.top()),
            Pos2::new(r.right() - 4.0, r.top()),
        ],
        Stroke::new(
            1.0_f32,
            if enabled {
                Color32::from_rgb(101, 135, 178)
            } else {
                Color32::from_rgb(37, 49, 66)
            },
        ),
    );
    response
}

pub(super) fn input(ui: &mut egui::Ui, edit: egui::TextEdit<'_>) -> egui::Response {
    let background = ui.painter().add(egui::Shape::Noop);
    let response = ui.add(
        edit.frame(false)
            .margin(egui::Margin::symmetric(8, 6))
            .text_color(SILVER),
    );
    let r = response.rect.expand2(Vec2::new(8.0, 6.0));
    ui.painter().set(
        background,
        surface(
            r,
            3.0,
            Color32::from_rgb(7, 13, 24),
            Color32::from_rgb(13, 24, 42),
        ),
    );
    for inset in 1..=3 {
        ui.painter().line_segment(
            [
                r.left_top() + Vec2::new(3.0, inset as f32),
                r.right_top() + Vec2::new(-3.0, inset as f32),
            ],
            Stroke::new(1.0_f32, Color32::from_black_alpha(100 / inset)),
        );
    }
    ui.painter().rect_stroke(
        r,
        3.0,
        Stroke::new(
            if response.has_focus() {
                1.5_f32
            } else {
                1.0_f32
            },
            if response.has_focus() { ICE } else { HAIRLINE },
        ),
        egui::StrokeKind::Inside,
    );
    response
}

pub(super) fn engrave(p: &egui::Painter, r: Rect, length: f32) {
    let r = r.shrink(5.0);
    let length = length.min(r.width() * 0.14).min(r.height() * 0.3);
    for (corner, x, y) in [
        (r.left_top(), 1.0, 1.0),
        (r.right_top(), -1.0, 1.0),
        (r.right_bottom(), -1.0, -1.0),
        (r.left_bottom(), 1.0, -1.0),
    ] {
        p.add(egui::Shape::line(
            vec![
                corner + Vec2::new(0.0, length * y),
                corner,
                corner + Vec2::new(length * x, 0.0),
            ],
            Stroke::new(1.0_f32, Color32::from_rgb(113, 135, 164)),
        ));
        let inner = corner + Vec2::new(3.0 * x, 3.0 * y);
        p.add(egui::Shape::line(
            vec![
                inner + Vec2::new(0.0, length * 0.45 * y),
                inner,
                inner + Vec2::new(length * 0.45 * x, 0.0),
            ],
            Stroke::new(0.6_f32, Color32::from_rgb(62, 80, 104)),
        ));
    }
}

pub(super) fn slate(p: &egui::Painter, r: Rect, radius: f32) {
    p.add(backdrop::sharp(p, r, radius));
    if radius > 0.0 {
        p.add(surface(
            r,
            radius,
            Color32::from_black_alpha(45),
            Color32::from_black_alpha(80),
        ));
    }
}
