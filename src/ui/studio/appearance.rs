use super::*;
use egui::{ColorImage, TextureHandle, TextureOptions};
use std::path::PathBuf;

const SCENES: [(&str, &[u8]); 4] = [
    ("Neon arcade", include_bytes!("../../../data/arcade.jpg")),
    (
        "Moonlit causeway",
        include_bytes!("../../../data/bridge.jpg"),
    ),
    ("Marina", include_bytes!("../../../data/marina.jpg")),
    ("Ocean Drive", include_bytes!("../../../data/hotel.jpg")),
];
const LABELS: [&str; 14] = [
    "Primary buttons",
    "Links & highlights",
    "Main text",
    "Secondary text",
    "Workspace",
    "Sidebar",
    "Cards & dialogs",
    "Controls",
    "Borders & dividers",
    "Ready",
    "Never run",
    "Updated, untested",
    "Update available",
    "Destructive actions",
];
pub(crate) const MIDNIGHT: [[u8; 3]; 14] = [
    [255, 171, 173],
    [141, 223, 200],
    [241, 243, 250],
    [165, 178, 199],
    [17, 21, 29],
    [23, 27, 38],
    [28, 34, 48],
    [36, 43, 57],
    [53, 61, 78],
    [141, 223, 200],
    [188, 165, 247],
    [109, 202, 247],
    [245, 200, 134],
    [255, 182, 184],
];
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Appearance {
    scene: usize,
    custom_wall: PathBuf,
    glass: bool,
    glass_darkness: u8,
    palette: usize,
    custom: [[u8; 3]; 14],
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            scene: 0,
            custom_wall: PathBuf::new(),
            glass: true,
            glass_darkness: 35,
            palette: 0,
            custom: MIDNIGHT,
        }
    }
}
impl Appearance {
    pub(crate) fn adopt_slideshow(&mut self) {
        self.scene = 6;
    }
    pub(super) fn slideshow(&self) -> bool {
        self.scene == 6
    }
    fn colors(&self) -> [[u8; 3]; 14] {
        let mut colors = MIDNIGHT;
        match self.palette {
            1 => {
                colors[0] = [148, 207, 255];
                colors[1] = [145, 234, 216];
                colors[4] = [16, 26, 37];
                colors[5] = [23, 40, 51];
                colors[6] = [32, 50, 63];
            }
            2 => {
                colors[0] = [255, 194, 139];
                colors[1] = [219, 181, 255];
                colors[4] = [32, 22, 33];
                colors[5] = [41, 29, 43];
                colors[6] = [51, 37, 51];
            }
            3 => return self.custom,
            _ => {}
        }
        colors
    }
    pub(super) fn never(&self) -> Color32 {
        theme::palette_color(10)
    }
    pub(super) fn untested(&self) -> Color32 {
        theme::palette_color(11)
    }
    pub(super) fn apply(&self, ctx: &egui::Context) {
        theme::set_palette(ctx, self.colors(), self.glass, self.glass_darkness);
    }
    pub(super) fn controls(&mut self, ui: &mut Ui, art: &Artwork) {
        theme::card(ui, 16.0, |ui| {
            ui.heading("Workspace background");
            let width = ((ui.available_width() - 32.0) / 5.0).clamp(76.0, 165.0);
            ui.horizontal_wrapped(|ui| {
                for (i, name) in SCENES
                    .iter()
                    .map(|s| s.0)
                    .chain(["Solid color"])
                    .enumerate()
                {
                    let (r, b) = ui.allocate_exact_size(vec2(width, 83.0), Sense::click());
                    ui.painter().rect(
                        r,
                        8.0,
                        theme::raised(),
                        Stroke::new(
                            1.0_f32,
                            if self.scene == i {
                                theme::lilac()
                            } else {
                                theme::line()
                            },
                        ),
                    );
                    let pic = Rect::from_min_size(r.min + vec2(6.0, 6.0), vec2(width - 12.0, 50.0));
                    if let Some(t) = art.scenes.get(i) {
                        Artwork::cover(ui.painter(), pic, t, 4.0);
                    } else {
                        ui.painter().rect_filled(pic, 4.0, theme::bg());
                    }
                    ui.painter().text(
                        r.left_bottom() + vec2(6.0, -6.0),
                        egui::Align2::LEFT_BOTTOM,
                        name,
                        theme::regular(11.5),
                        if self.scene == i {
                            theme::lilac()
                        } else {
                            theme::white()
                        },
                    );
                    if b.clicked() {
                        self.scene = i;
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.scene, 6, "My slideshow");
                if !self.custom_wall.as_os_str().is_empty() {
                    ui.selectable_value(&mut self.scene, 5, "My image");
                }
                if ui.button("Choose image…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Image", &["jpg", "jpeg", "png"])
                        .pick_file()
                    {
                        self.custom_wall = p;
                        self.scene = 5;
                    }
                }
            });
            ui.checkbox(&mut self.glass, "Frosted panels");
            ui.add_enabled(
                self.glass,
                egui::Slider::new(&mut self.glass_darkness, 0..=100)
                    .text("Glass darkness")
                    .suffix("%"),
            );
        });
        theme::card(ui, 16.0, |ui| {
            ui.heading("Color palette");
            ui.horizontal_wrapped(|ui| {
                for (i, name) in [
                    "PalmVI Midnight",
                    "Ocean Glass",
                    "Sunset Drive",
                    "My colors",
                ]
                .iter()
                .enumerate()
                {
                    ui.selectable_value(&mut self.palette, i, *name);
                }
            });
            for (group, range) in [
                ("Accents & text", 0..4),
                ("Backgrounds & borders", 4..9),
                ("Status colors", 9..14),
            ] {
                egui::CollapsingHeader::new(group)
                    .default_open(range.start == 0)
                    .show(ui, |ui| {
                        let columns =
                            (ui.available_width() / 145.0).floor().clamp(1.0, 4.0) as usize;
                        for chunk in range.collect::<Vec<_>>().chunks(columns) {
                            ui.columns(columns, |cols| {
                                for (ui, &index) in cols.iter_mut().zip(chunk) {
                                    Frame::none()
                                        .inner_margin(6.0)
                                        .stroke(Stroke::new(1.0_f32, theme::line()))
                                        .rounding(7.0)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let mut c = self.colors()[index];
                                                if ui.color_edit_button_srgb(&mut c).changed() {
                                                    self.custom = self.colors();
                                                    self.custom[index] = c;
                                                    self.palette = 3;
                                                }
                                                ui.vertical(|ui| {
                                                    ui.spacing_mut().item_spacing.y = 0.0;
                                                    ui.label(
                                                        RichText::new(LABELS[index]).size(12.0),
                                                    );
                                                    ui.small(format!(
                                                        "#{:02X}{:02X}{:02X}",
                                                        c[0], c[1], c[2]
                                                    ));
                                                });
                                            })
                                        });
                                }
                            });
                        }
                    });
            }
            if ui.button("Restore default theme").clicked() {
                *self = Self::default();
            }
        });
    }
}
#[derive(Default)]
pub(super) struct Artwork {
    scenes: Vec<TextureHandle>,
    blurred: Vec<TextureHandle>,
    custom: Option<(TextureHandle, TextureHandle)>,
    path: PathBuf,
    active: Option<usize>,
}
fn texture(ctx: &egui::Context, img: &image::DynamicImage, name: &str) -> TextureHandle {
    let rgba = img.thumbnail(1920, 1920).to_rgba8();
    ctx.load_texture(
        name,
        ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        ),
        TextureOptions::LINEAR,
    )
}
impl Artwork {
    pub(super) fn prepare(&mut self, ctx: &egui::Context, prefs: &Appearance, private: bool) {
        if self.scenes.is_empty() {
            for (name, bytes) in SCENES {
                let img = image::load_from_memory(bytes).expect("bundled PalmVI artwork");
                self.scenes.push(texture(ctx, &img, name));
                self.blurred.push(texture(
                    ctx,
                    &img.thumbnail(180, 180).blur(4.0),
                    &format!("{name}-glass"),
                ));
            }
        }
        self.active = match prefs.scene {
            4 => None,
            5 if !private => Some(5),
            0..=3 => Some(prefs.scene),
            _ => Some(0),
        };
        if self.active == Some(5) && self.path != prefs.custom_wall {
            self.custom = image::open(&prefs.custom_wall).ok().map(|img| {
                (
                    texture(ctx, &img, "custom-wall"),
                    texture(ctx, &img.thumbnail(180, 180).blur(4.0), "custom-wall-glass"),
                )
            });
            self.path = prefs.custom_wall.clone();
        }
    }
    fn cover(p: &egui::Painter, r: Rect, tex: &TextureHandle, round: f32) -> Rect {
        let size = tex.size_vec2();
        let ratio = (r.width() / r.height()) / (size.x / size.y);
        let uv =
            Rect::from_center_size(pos2(0.5, 0.5), vec2(ratio.min(1.0), (1.0 / ratio).min(1.0)));
        let mut shape = egui::epaint::RectShape::filled(r, round, Color32::WHITE);
        shape.fill_texture_id = tex.id();
        shape.uv = uv;
        p.add(shape);
        uv
    }
    pub(super) fn wall(&self, p: &egui::Painter, r: Rect) {
        theme::set_wall_glass(None);
        if let Some(i) = self.active {
            if let Some(t) = if i == 5 {
                self.custom.as_ref().map(|p| &p.0)
            } else {
                self.scenes.get(i)
            } {
                let uv = Self::cover(p, r, t, 0.0);
                if let Some(blur) = if i == 5 {
                    self.custom.as_ref().map(|p| &p.1)
                } else {
                    self.blurred.get(i)
                } {
                    theme::set_wall_glass(Some((blur.id(), r, uv)));
                }
                let c = theme::bg();
                p.rect_filled(
                    r,
                    0.0,
                    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 138),
                );
                theme::fade_bottom(p, r, c);
            }
        }
    }
    pub(super) fn thumbnail(&self, p: &egui::Painter, r: Rect, key: &str) {
        let i = key
            .bytes()
            .fold(0usize, |v, b| v.wrapping_mul(31).wrapping_add(b as usize))
            % 4;
        if let Some(t) = self.scenes.get(i) {
            Self::cover(p, r, t, 7.0);
        }
    }
}
