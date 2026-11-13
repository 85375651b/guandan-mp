//! Small cached procedural textures, including a real blurred version for glass.
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

const VERTICES: [Vec2; 9] = [
    Vec2::new(-0.5, -0.18),
    Vec2::new(-0.28, -0.46),
    Vec2::new(0.28, -0.46),
    Vec2::new(0.5, -0.18),
    Vec2::new(0.0, 0.62),
    Vec2::new(-0.18, -0.18),
    Vec2::new(0.18, -0.18),
    Vec2::new(0.0, -0.46),
    Vec2::new(0.0, 0.12),
];
const FACES: [([usize; 3], [f32; 3]); 10] = [
    ([0, 1, 5], [19.0, 40.0, 71.0]),
    ([1, 7, 5], [28.0, 56.0, 92.0]),
    ([7, 6, 5], [25.0, 53.0, 93.0]),
    ([7, 2, 6], [20.0, 44.0, 76.0]),
    ([2, 3, 6], [31.0, 61.0, 103.0]),
    ([0, 5, 4], [12.0, 27.0, 49.0]),
    ([5, 8, 4], [20.0, 43.0, 78.0]),
    ([5, 6, 8], [30.0, 62.0, 106.0]),
    ([6, 3, 4], [14.0, 32.0, 59.0]),
    ([8, 6, 4], [18.0, 39.0, 70.0]),
];
const GEMS: [(Vec2, f32); 2] = [(Vec2::new(0.91, 0.20), 0.43), (Vec2::new(0.08, 0.78), 0.40)];

#[derive(Clone)]
struct Backdrop {
    size: [usize; 2],
    blurred: egui::TextureHandle,
}

fn texture(ctx: &egui::Context) -> egui::TextureHandle {
    let screen = ctx.screen_rect();
    let size = [
        (screen.width() / 5.0).round().clamp(160.0, 320.0) as usize,
        (screen.height() / 5.0).round().clamp(100.0, 240.0) as usize,
    ];
    let key = egui::Id::new("diamond-vault-backdrop");
    let cached = ctx.data(|d| d.get_temp::<Backdrop>(key));
    let backdrop = if let Some(cached) = cached.filter(|c| c.size == size) {
        cached
    } else {
        let image = image(size, screen.size());
        let blurred = blur_image(&image);
        let backdrop = Backdrop {
            size,
            blurred: ctx.load_texture(
                "vault-facets-blurred",
                blurred,
                egui::TextureOptions::LINEAR,
            ),
        };
        ctx.data_mut(|d| d.insert_temp(key, backdrop.clone()));
        backdrop
    };
    backdrop.blurred
}

pub(super) fn shape(p: &egui::Painter, r: Rect, radius: f32) -> egui::Shape {
    let screen = p.ctx().screen_rect();
    let texture = texture(p.ctx());
    let mut mesh = egui::Mesh::with_texture(texture.id());
    let mut points = vec![r.center()];
    points.extend(super::boundary(r, radius));
    for pos in points {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: Pos2::new(
                (pos.x - screen.left()) / screen.width(),
                (pos.y - screen.top()) / screen.height(),
            ),
            color: Color32::WHITE,
        });
    }
    let count = mesh.vertices.len() as u32 - 1;
    for i in 1..=count {
        mesh.add_triangle(0, i, i % count + 1);
    }
    egui::Shape::mesh(mesh)
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Visible facets stay vector sharp at any DPI. Only the glass backdrop uses a
/// small raster texture; the two paths share the same crystal geometry/colors.
pub(super) fn sharp(p: &egui::Painter, r: Rect, radius: f32) -> egui::Shape {
    let screen = p.ctx().screen_rect();
    let bounds = super::boundary(r, radius);
    let mut shapes = vec![super::surface(
        r,
        radius,
        Color32::from_rgb(16, 22, 32),
        super::CANVAS,
    )];
    for (center, scale) in GEMS {
        let center = screen.min + center * screen.size();
        for (indices, color) in FACES {
            let polygon: Vec<_> = indices
                .iter()
                .map(|&i| center + VERTICES[i] * screen.width() * scale)
                .collect();
            let polygon = clip(polygon, &bounds);
            if polygon.len() < 3 {
                continue;
            }
            let color = Color32::from_rgb(
                (color[0] * 0.77 + 15.0 * 0.23) as u8,
                (color[1] * 0.77 + 20.0 * 0.23) as u8,
                (color[2] * 0.77 + 28.0 * 0.23) as u8,
            );
            shapes.push(egui::Shape::convex_polygon(
                polygon,
                color,
                egui::Stroke::NONE,
            ));
        }
    }
    egui::Shape::Vec(shapes)
}

fn clip(mut polygon: Vec<Pos2>, boundary: &[Pos2]) -> Vec<Pos2> {
    for (&a, &b) in boundary.iter().zip(boundary.iter().cycle().skip(1)) {
        if polygon.is_empty() {
            break;
        }
        let edge = b - a;
        if edge.length_sq() < 0.0001 {
            continue;
        }
        let input = std::mem::take(&mut polygon);
        let mut previous = *input.last().unwrap();
        let mut d_previous = cross(edge, previous - a);
        for current in input {
            let d_current = cross(edge, current - a);
            if (d_current >= 0.0) != (d_previous >= 0.0) {
                polygon.push(
                    previous + (current - previous) * (d_previous / (d_previous - d_current)),
                );
            }
            if d_current >= 0.0 {
                polygon.push(current);
            }
            previous = current;
            d_previous = d_current;
        }
    }
    polygon.dedup_by(|a, b| a.distance_sq(*b) < 0.001);
    if polygon.len() > 2 && polygon[0].distance_sq(*polygon.last().unwrap()) < 0.001 {
        polygon.pop();
    }
    polygon
}

fn image(size: [usize; 2], screen: Vec2) -> egui::ColorImage {
    let mut image = egui::ColorImage::new(size, super::CANVAS);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let uv = Vec2::new(x as f32 / size[0] as f32, y as f32 / size[1] as f32);
            let vignette = ((uv - Vec2::splat(0.5)).length_sq() * 0.55).min(0.35);
            let ambient =
                (1.0 - ((uv - Vec2::new(0.85, 0.15)) * Vec2::new(1.0, 0.8)).length()).max(0.0);
            let mut color = [
                15.0 - vignette * 9.0 + ambient * 2.0,
                17.0 - vignette * 9.0 + ambient * 5.0,
                21.0 - vignette * 9.0 + ambient * 12.0,
            ];
            for (center, scale) in GEMS {
                let point = (uv - center) * screen / (screen.x * scale);
                for (indices, facet) in FACES {
                    let a = VERTICES[indices[0]];
                    let b = VERTICES[indices[1]];
                    let c = VERTICES[indices[2]];
                    let total = cross(b - a, c - a);
                    let u = cross(b - point, c - point) / total;
                    let v = cross(c - point, a - point) / total;
                    let w = 1.0 - u - v;
                    if u >= 0.0 && v >= 0.0 && w >= 0.0 {
                        for i in 0..3 {
                            color[i] = color[i] * 0.23 + facet[i] * 0.77;
                        }
                        if u.min(v).min(w) < 0.012 {
                            color[0] += 3.0;
                            color[1] += 5.0;
                            color[2] += 7.0;
                        }
                        break;
                    }
                }
            }
            image.pixels[y * size[0] + x] =
                Color32::from_rgb(color[0] as u8, color[1] as u8, color[2] as u8);
        }
    }
    image
}

/// A separable Gaussian at reduced resolution gives a soft 20–30 point blur.
/// Generated only on viewport texture-size changes, with no render-thread readback.
fn blur_image(source: &egui::ColorImage) -> egui::ColorImage {
    let weights = [1.0, 4.0, 7.0, 10.0, 13.0, 10.0, 7.0, 4.0, 1.0];
    let mut image = source.clone();
    for horizontal in [true, false] {
        let input = image.clone();
        for y in 0..source.size[1] {
            for x in 0..source.size[0] {
                let mut sum = [0.0; 3];
                for (i, &weight) in weights.iter().enumerate() {
                    let delta = i as isize - 4;
                    let sx = if horizontal {
                        (x as isize + delta).clamp(0, source.size[0] as isize - 1) as usize
                    } else {
                        x
                    };
                    let sy = if horizontal {
                        y
                    } else {
                        (y as isize + delta).clamp(0, source.size[1] as isize - 1) as usize
                    };
                    let c = input.pixels[sy * source.size[0] + sx];
                    for (sum, value) in sum.iter_mut().zip([c.r(), c.g(), c.b()]) {
                        *sum += value as f32 * weight;
                    }
                }
                image.pixels[y * source.size[0] + x] = Color32::from_rgb(
                    (sum[0] / 57.0) as u8,
                    (sum[1] / 57.0) as u8,
                    (sum[2] / 57.0) as u8,
                );
            }
        }
    }
    image
}
