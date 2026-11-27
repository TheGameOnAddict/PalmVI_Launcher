//! Mandyhub's Newer Retail Games style uses a deep navy night,
//! a pastel sunset strip along the top, soft pink pill buttons with plum
//! type, lavender spaced capitals for labels and pale blush for reading.
//! Rockstar's typeface is theirs, so the look-alikes are free ones built into
//! the exe (fonts/, SIL Open Font License): Josefin Sans for spaced capitals
//! and buttons, Big Shoulders Display for big names and figures. Reading
//! text and icons are Windows' own (Segoe UI, Segoe Fluent Icons, Cascadia).
//! Hover steps in over 150 ms and out over 120 ms, a press sinks 1 px,
//! indicators slide in 240 ms.

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, FontFamily, FontId, Frame, Galley, Id, Margin, Pos2, Rect,
    Response, RichText, Rounding, Sense, Shadow, Stroke, TextStyle, Ui,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

const PEACH:Color32=Color32::from_rgb(255,189,173);
const MAUVE:Color32=Color32::from_rgb(206,139,181);
const LAVENDER:Color32=Color32::from_rgb(145,111,180);
const ON_BAD:Color32=Color32::from_rgb(42,12,18);
pub const R: f32 = 10.0;
pub const R_LG: f32 = 16.0;
/// The height of every field and normal button.
pub const CONTROL_H: f32 = 34.0;

thread_local!{
    static PALETTE:std::cell::Cell<([[u8;3];14],bool)>=const{std::cell::Cell::new((crate::ui::MIDNIGHT,true))};
    static GLASS_DARKNESS:std::cell::Cell<u8> = const { std::cell::Cell::new(35) };
}
pub fn set_palette(ctx:&egui::Context,colors:[[u8;3];14],glass:bool,darkness:u8){
    GLASS_DARKNESS.set(darkness.min(100));
    if PALETTE.replace((colors,glass))!=(colors,glass){refresh_style(ctx);}
}
pub fn bg()->Color32{palette_color(4)}
pub fn deep()->Color32{palette_color(5)}
pub fn surface()->Color32{palette_color(6)}
pub fn raised()->Color32{palette_color(7)}
pub fn line()->Color32{palette_color(8)}
pub fn line_strong()->Color32{palette_color(8)}
pub fn white()->Color32{palette_color(2)}
pub fn blush()->Color32{palette_color(2)}
pub fn lilac()->Color32{palette_color(1)}
pub fn muted()->Color32{palette_color(3)}
pub fn faint()->Color32{palette_color(3)}
pub fn pink()->Color32{palette_color(0)}
pub fn action_text()->Color32{let color=egui::Rgba::from(pink());if color.r()*0.2126+color.g()*0.7152+color.b()*0.0722>0.18{Color32::from_rgb(39,28,38)}else{Color32::WHITE}}
pub fn good()->Color32{palette_color(9)}
pub fn warn()->Color32{palette_color(12)}
pub fn bad()->Color32{palette_color(13)}
pub fn palette_color(i:usize)->Color32{
    let [r,g,b]=PALETTE.get().0[i];Color32::from_rgb(r,g,b)
}

/// Segoe Fluent Icons (Windows 11) and Segoe MDL2 Assets (Windows 10) share these.
pub mod glyph {
    pub const ADD: char = '\u{E710}';
    pub const CHECK: char = '\u{E73E}';
    pub const COPY: char = '\u{E8C8}';
    pub const DELETE: char = '\u{E74D}';
    pub const DOWNLOAD: char = '\u{E896}';
    pub const FOLDER: char = '\u{E8B7}';
    pub const LINK: char = '\u{E71B}';
    pub const OPEN: char = '\u{E8A7}';
    pub const PLAY: char = '\u{E768}';
    pub const REFRESH: char = '\u{E72C}';
    pub const REPAIR: char = '\u{E90F}';
    pub const SEARCH: char = '\u{E721}';
    pub const SETTINGS: char = '\u{E713}';
    pub const SYNC: char = '\u{E895}';
    pub const WARNING: char = '\u{E7BA}';
    pub const STOP: char = '\u{E71A}';
    pub const WARM: char = '\u{E945}';
}

/// Whether an icon font was found (without one, controls show words only).
static ICONS: AtomicBool = AtomicBool::new(false);

pub fn has_icons() -> bool {
    ICONS.load(Ordering::Relaxed)
}

fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}
/// Reading text: Segoe UI.
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
/// Names in lists: Segoe UI Semibold.
pub fn strong(size: f32) -> FontId {
    FontId::new(size, family("strong"))
}
/// Big condensed names and figures: Big Shoulders Display ExtraBold.
pub fn display(size: f32) -> FontId {
    FontId::new(size, family("display"))
}
/// Spaced capitals: Josefin Sans Bold.
pub fn deco(size: f32) -> FontId {
    FontId::new(size, family("deco"))
}
/// Buttons: Josefin Sans SemiBold.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
pub fn glyphs(size: f32) -> FontId {
    FontId::new(size, family("icons"))
}

/// The bundled faces, then Windows' own, each with egui's behind it for
/// anything it lacks.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("Fonts");
    let mut load = |key: &str, files: &[&str]| {
        for f in files {
            if let Ok(bytes) = std::fs::read(dir.join(f)) {
                fonts.font_data.insert(key.to_owned(), egui::FontData::from_owned(bytes));
                return true;
            }
        }
        false
    };
    let has_text = load("text", &["segoeui.ttf"]);
    let has_strong = load("strong", &["seguisb.ttf", "segoeuib.ttf"]);
    let has_mono = load("mono", &["CascadiaMono.ttf", "consola.ttf"]);
    let has_glyphs = load("icons", &["SegoeIcons.ttf", "segmdl2.ttf"]);
    let bundled: [(&str, &'static [u8]); 3] = [
        ("deco", include_bytes!("../fonts/JosefinSans-Bold.ttf")),
        ("deco-semi", include_bytes!("../fonts/JosefinSans-SemiBold.ttf")),
        ("display", include_bytes!("../fonts/BigShouldersDisplay-ExtraBold.ttf")),
    ];
    for (key, bytes) in bundled {
        fonts.font_data.insert(key.to_owned(), egui::FontData::from_static(bytes));
    }

    if has_text {
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "text".to_owned());
    }
    if has_mono {
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "mono".to_owned());
    }
    let text = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let chain = |first: Option<&str>, rest: &[String]| -> Vec<String> {
        first.map(str::to_owned).into_iter().chain(rest.iter().cloned()).collect()
    };
    let strong_chain = chain(has_strong.then_some("strong"), &text);
    for key in ["deco", "deco-semi", "display"] {
        fonts.families.insert(family(key), chain(Some(key), &strong_chain));
    }
    fonts.families.insert(family("strong"), strong_chain);
    fonts.families.insert(family("icons"), chain(has_glyphs.then_some("icons"), &text));
    ICONS.store(has_glyphs, Ordering::Relaxed);
    ctx.set_fonts(fonts);
}

/// The site's colour runs as small pictures that shapes are filled with,
/// which keeps rounded corners smooth: the sunset strip (across), the sky
/// (down, peach to lavender) and the panel light (down, a lit top).
static SKY: OnceLock<egui::TextureHandle> = OnceLock::new();
const SKY_STOPS: [(f32, Color32); 3] = [(0.0, PEACH), (0.5, MAUVE), (1.0, LAVENDER)];


fn sample(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        if t <= w[1].0 {
            return mix(w[0].1, w[1].1, (t - w[0].0) / (w[1].0 - w[0].0).max(1e-6));
        }
    }
    stops.last().map_or(pink(), |s| s.1)
}

fn gradient_image(len: usize, stops: &[(f32, Color32)], down: bool) -> egui::ColorImage {
    let pixels = (0..len).map(|i| sample(stops, i as f32 / (len - 1) as f32)).collect();
    egui::ColorImage { size: if down { [1, len] } else { [len, 1] }, pixels }
}

/// The night theme, always (it does not follow the system's light mode).
pub fn apply(ctx: &egui::Context) {
    let linear = egui::TextureOptions::LINEAR;
    let _ = SKY.set(ctx.load_texture("sky", gradient_image(256, &SKY_STOPS, true), linear));
    refresh_style(ctx);
}

fn refresh_style(ctx:&egui::Context){
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        let v = &mut s.visuals;
        v.override_text_color = None;
        v.panel_fill = bg();
        v.window_fill = surface();
        v.window_stroke = Stroke::new(1.0_f32, line_strong());
        v.window_rounding = Rounding::same(R_LG);
        v.window_shadow = shadow();
        v.popup_shadow = Shadow { offset: vec2(0.0, 6.0), blur: 18.0, spread: 0.0, color: Color32::from_black_alpha(140) };
        v.menu_rounding = Rounding::same(R);
        v.extreme_bg_color = deep();
        v.faint_bg_color = surface();
        v.code_bg_color = surface();
        v.hyperlink_color = pink();
        v.selection.bg_fill = pink().gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0_f32, pink());
        v.text_cursor.stroke = Stroke::new(1.5_f32, white());
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = surface();
        w.noninteractive.weak_bg_fill = surface();
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, line());
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, muted());
        w.inactive.bg_fill = raised();
        w.inactive.weak_bg_fill = raised();
        w.inactive.bg_stroke = Stroke::new(1.0_f32, line_strong());
        w.inactive.fg_stroke = Stroke::new(1.0_f32, white());
        w.hovered.bg_fill = mix(raised(), white(), 0.07);
        w.hovered.weak_bg_fill = mix(raised(), white(), 0.07);
        w.hovered.bg_stroke = Stroke::new(1.0_f32, line_strong());
        w.hovered.fg_stroke = Stroke::new(1.0_f32, white());
        w.hovered.expansion = 0.0;
        w.active.bg_fill = mix(raised(), white(), 0.07);
        w.active.weak_bg_fill = mix(raised(), white(), 0.07);
        w.active.bg_stroke = Stroke::new(1.0_f32, pink());
        w.active.fg_stroke = Stroke::new(1.0_f32, white());
        w.active.expansion = 0.0;
        w.open = w.hovered;
        for x in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            x.rounding = Rounding::same(R);
        }
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(14.0, 6.0);
        s.spacing.interact_size = vec2(34.0, CONTROL_H);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.text_styles = [
            (TextStyle::Small, regular(12.0)),
            (TextStyle::Body, regular(14.0)),
            (TextStyle::Button, strong(13.0)),
            (TextStyle::Heading, strong(20.0)),
            (TextStyle::Monospace, mono(12.5)),
        ]
        .into();
    });
}

fn shadow() -> Shadow {
    Shadow { offset: vec2(0.0, 20.0), blur: 56.0, spread: 0.0, color: Color32::from_rgba_unmultiplied(3, 3, 10, 200) }
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// The one entrance curve: fast out, soft landing.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// 0 at rest, 1 hovered: 150 ms in, 120 ms out.
fn hover(ui: &Ui, id: Id, on: bool) -> f32 {
    ui.ctx().animate_bool_with_time(id.with("hover"), on, if on { 0.15 } else { 0.12 })
}

/// Fills `rect` with one of the colour runs; `tint` white is full strength.
/// The page's ground: the night a little lit at the top, as on the site.
pub fn ground(painter: &egui::Painter, rect: Rect) {
    let lit = Rect::from_min_size(rect.min, vec2(rect.width(), rect.height().min(520.0)));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(lit.left_top(), raised());
    mesh.colored_vertex(lit.right_top(), raised());
    mesh.colored_vertex(lit.left_bottom(), bg());
    mesh.colored_vertex(lit.right_bottom(), bg());
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

/// A round mark in the sky's colours.
pub fn mark(painter: &egui::Painter, center: Pos2, radius: f32) {
    let rect=Rect::from_center_size(center,vec2(radius*2.0,radius*2.0));
    if let Some(tex)=SKY.get(){let mut s=egui::epaint::RectShape::filled(rect,radius,Color32::WHITE);s.fill_texture_id=tex.id();s.uv=Rect::from_min_max(pos2(0.,0.),pos2(1.,1.));painter.add(s);}else{painter.circle_filled(center,radius,pink());}
}

/// PalmVI's window and taskbar mark, matching the executable's icon resource.
pub fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../data/palmvi.png"))
        .expect("bundled PalmVI icon").into_rgba8();
    egui::IconData { width: image.width(), height: image.height(), rgba: image.into_raw() }
}

/// Spaced capitals, the way the site sets its labels.
pub fn caps(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.label(RichText::new(text.to_uppercase()).font(deco(size)).color(color).extra_letter_spacing(size * 0.16))
}

fn caps_galley(painter: &egui::Painter, text: &str, size: f32, track: f32) -> Arc<Galley> {
    let job = egui::text::LayoutJob::single_section(
        text.to_uppercase(),
        egui::TextFormat { font_id: deco(size), color: Color32::PLACEHOLDER, extra_letter_spacing: size * track, ..Default::default() },
    );
    painter.layout_job(job)
}

/// A big white name in condensed capitals, shrunk to fit `max_width`.
pub fn title(ui: &mut Ui, text: &str, size: f32, max_width: f32) -> Response {
    let painter = ui.painter().clone();
    let mut galley = painter.layout_no_wrap(text.to_string(), deco(size), white());
    if galley.size().x > max_width {
        let fit = (size * max_width / galley.size().x).max(size * 0.5);
        galley = painter.layout_no_wrap(text.to_string(), deco(fit), white());
    }
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::hover());
    painter.galley(rect.min, galley, white());
    resp
}

/// `text` on one line, cut with an ellipsis to `max_width`.
pub fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: max_width.max(12.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|f| f.layout_job(job))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// The one main action on a screen: a soft pink pill.
    Primary,
    Secondary,
    Ghost,
    /// A ghost button with a pulsing status colour.
    Signal(Color32, f32),
    /// A destructive action offered next to others.
    Danger,
    /// The confirm button of a destructive dialog.
    DangerSolid,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Size {
    Small,
    Normal,
}

/// Fill, border and text colour of a button.
fn colors(look: Look, t: f32, live: bool) -> (Color32, Stroke, Color32) {
    if !live {
        return match look {
            Look::Ghost | Look::Danger => (Color32::TRANSPARENT, Stroke::NONE, faint().gamma_multiply(0.75)),
            _ => (surface(), Stroke::new(1.0_f32, line()), faint()),
        };
    }
    match look {
        Look::Primary => (mix(pink(), white(), 0.14*t), Stroke::NONE, action_text()),
        Look::Secondary => (mix(raised(), mix(raised(), white(), 0.07), t), Stroke::new(1.0_f32, mix(line_strong(), pink(), 0.5 * t)), white()),
        Look::Ghost => (mix(Color32::TRANSPARENT, raised(), t), Stroke::NONE, mix(lilac(), white(), t)),
        Look::Signal(color, strength) => (
            mix(mix(Color32::TRANSPARENT, raised(), t), color, 0.2 * strength),
            Stroke::new(1.0_f32, color.gamma_multiply(0.8 * strength)),
            mix(mix(lilac(), white(), t), color, strength),
        ),
        Look::Danger => (mix(Color32::TRANSPARENT, bad().gamma_multiply(0.14), t), Stroke::NONE, bad()),
        Look::DangerSolid => (mix(bad(), white(), 0.15 * t), Stroke::NONE, ON_BAD),
    }
}

/// Every button in the launcher: a pill. `busy` shows a spinner in place of
/// the icon and keeps the button's colour while it does not take clicks.
pub fn button_ex(ui: &mut Ui, look: Look, size: Size, icon: Option<char>, label: &str, enabled: bool, busy: bool) -> Response {
    let (h, fs, pad, min_w) = match size {
        Size::Small => (28.0, 13.0, 12.0, 0.0),
        Size::Normal => (CONTROL_H, 14.0, 16.0, 0.0),
    };
    let painter = ui.painter().clone();
    let label_g = (!label.is_empty()).then(|| painter.layout_no_wrap(label.to_owned(), strong(fs), Color32::PLACEHOLDER));
    let icon_g = icon
        .filter(|_| has_icons() && !busy)
        .map(|c| painter.layout_no_wrap(c.to_string(), glyphs(fs), Color32::PLACEHOLDER));
    let lead = if busy { fs } else { icon_g.as_ref().map_or(0.0, |g| g.size().x) };
    let gap = if lead > 0.0 && label_g.is_some() { 8.0 } else { 0.0 };
    let content = lead + gap + label_g.as_ref().map_or(0.0, |g| g.size().x);
    let w = if label_g.is_none() { h } else { (content + pad * 2.0).max(min_w) };
    let live = enabled && !busy;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), if live { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let t = hover(ui, resp.id, live && resp.hovered());
        let down = live && resp.is_pointer_button_down_on();
        let (fill, stroke, fg) = if busy { colors(look, 0.0, true) } else { colors(look, t, live) };
        // Up 1 px on hover; pressed, 1 px below rest and a touch smaller.
        let mut r = rect.translate(vec2(0.0, if down { 1.0 } else { -t }));
        if down {
            r = r.shrink2(r.size() * 0.0075);
        }
        painter.rect(r, 7.0, fill, stroke);
        let mut x = r.center().x - content / 2.0;
        let cy = r.center().y;
        if busy {
            let spin = Rect::from_center_size(pos2(x + lead / 2.0, cy), vec2(lead, lead));
            egui::Spinner::new().size(lead).color(fg).paint_at(ui, spin);
            x += lead + gap;
        } else if let Some(g) = icon_g {
            let gw = g.size().x;
            painter.galley(pos2(x, cy - g.size().y / 2.0), g, fg);
            x += gw + gap;
        }
        if let Some(g) = label_g {
            // Josefin's capitals sit high in its line: a nudge down centres them.
            painter.galley(pos2(x, cy - g.size().y / 2.0 + fs * 0.08), g, fg);
        }
        if resp.has_focus() {
            painter.rect_stroke(r.expand(3.0), 8.0, Stroke::new(2.0_f32, pink()));
        }
    }
    if live {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn button(ui: &mut Ui, look: Look, icon: Option<char>, label: &str) -> Response {
    button_ex(ui, look, Size::Normal, icon, label, true, false)
}

pub fn small(ui: &mut Ui, look: Look, icon: Option<char>, label: &str, enabled: bool) -> Response {
    button_ex(ui, look, Size::Small, icon, label, enabled, false)
}

/// A round button with an icon; the words show instead when no icon font was found.
pub fn icon_button(ui: &mut Ui, look: Look, size: Size, icon: char, tip: &str) -> Response {
    let r = if has_icons() {
        button_ex(ui, look, size, Some(icon), "", true, false)
    } else {
        button_ex(ui, look, size, None, tip, true, false)
    };
    r.on_hover_text(tip)
}

/// A one-line field `width` wide, with an optional icon inside on the left.
pub fn field(ui: &mut Ui, value: &mut String, hint: &str, width: f32, lead: Option<char>) -> Response {
    let lead = lead.filter(|_| has_icons());
    let margin = Margin { left: if lead.is_some() { 38.0 } else { 14.0 }, right: 14.0, top: 0.0, bottom: 0.0 };
    let edit = egui::TextEdit::singleline(value)
        .hint_text(RichText::new(hint).color(faint()))
        .font(regular(14.0))
        .text_color(white())
        .margin(margin)
        .min_size(vec2(width, CONTROL_H))
        .desired_width((width - margin.left - margin.right).max(24.0))
        .vertical_align(egui::Align::Center);
    let pill = 7.0;
    let resp = ui
        .scope(|ui| {
            let w = &mut ui.visuals_mut().widgets;
            for x in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                x.rounding = Rounding::same(pill);
            }
            ui.add(edit)
        })
        .inner;
    // egui 0.29 answers with the text's own box, inside the margin: the field is that plus the margin.
    let outer = resp.rect + margin;
    let focus = ui.ctx().animate_bool_with_time(resp.id.with("focus"), resp.has_focus(), 0.14);
    if focus > 0.0 {
        ui.painter().rect_stroke(outer.expand(2.0), pill + 2.0, Stroke::new(3.0_f32, pink().gamma_multiply(0.35 * focus)));
    }
    if let Some(c) = lead {
        let color = if resp.has_focus() { pink() } else { faint() };
        ui.painter().text(pos2(outer.left() + 19.0, outer.center().y), Align2::CENTER_CENTER, c, glyphs(14.0), color);
    }
    resp
}

/// Options side by side in spaced capitals, one picked; its pill slides to
/// a new pick. `width`: the whole control's width (equal parts), else each
/// option's own.
pub fn segmented(ui: &mut Ui, id_salt: &str, options: &[&str], selected: Option<usize>, width: Option<f32>) -> Option<usize> {
    let id = Id::new(id_salt);
    let pad = 3.0;
    let painter = ui.painter().clone();
    let galleys: Vec<Arc<Galley>> = options.iter().map(|o| caps_galley(&painter, o, 11.5, 0.12)).collect();
    let widths: Vec<f32> = match width {
        Some(w) => vec![(w - pad * 2.0) / options.len().max(1) as f32; options.len()],
        None => galleys.iter().map(|g| g.size().x + 32.0).collect(),
    };
    let total = widths.iter().sum::<f32>() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, CONTROL_H), Sense::hover());
    painter.rect(rect, rect.height() / 2.0, deep(), Stroke::new(1.0_f32, line()));
    let mut x = rect.left() + pad;
    let cells: Vec<Rect> = widths
        .iter()
        .map(|w| {
            let r = Rect::from_min_size(pos2(x, rect.top() + pad), vec2(*w, rect.height() - pad * 2.0));
            x += w;
            r
        })
        .collect();
    if let Some(cell) = selected.and_then(|i| cells.get(i)) {
        let ctx = ui.ctx();
        let ax = ctx.animate_value_with_time(id.with("x"), cell.left() - rect.left(), 0.24);
        let aw = ctx.animate_value_with_time(id.with("w"), cell.width(), 0.24);
        let pill = Rect::from_min_size(pos2(rect.left() + ax, cell.top()), vec2(aw, cell.height()));
        painter.rect(pill, pill.height() / 2.0, raised(), Stroke::new(1.0_f32, line_strong()));
    }
    let mut picked = None;
    for (i, (cell, g)) in cells.iter().zip(galleys).enumerate() {
        let resp = ui.interact(*cell, id.with(i), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let on = selected == Some(i);
        let t = hover(ui, resp.id, resp.hovered());
        let at = cell.center() - g.size() / 2.0 + vec2(0.0, 1.0);
        painter.galley(at, g, if on { white() } else { mix(muted(), white(), t) });
        if resp.has_focus() {
            painter.rect_stroke(cell.expand(1.0), cell.height() / 2.0, Stroke::new(2.0_f32, pink()));
        }
        if resp.clicked() && !on {
            picked = Some(i);
        }
    }
    picked
}

pub fn toggle(ui: &mut Ui, on: &mut bool, title: &str, hint: &str) -> Response {
    let w = ui.available_width();
    let text_x = 54.0;
    let painter = ui.painter().clone();
    let title_g = painter.layout_no_wrap(title.to_owned(), strong(14.0), white());
    let hint_g = painter.layout(hint.to_owned(), regular(13.0), faint(), (w - text_x).max(80.0));
    let h = title_g.size().y + 1.0 + hint_g.size().y;
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t_on = ui.ctx().animate_bool_with_time(resp.id.with("on"), *on, 0.18);
    let t = hover(ui, resp.id, resp.hovered());
    let track = Rect::from_min_size(pos2(rect.left(), rect.top() + 1.0), vec2(40.0, 22.0));
    let off = mix(raised(), mix(raised(), white(), 0.07), t);
    painter.rect(track, 11.0, mix(off, mix(pink(), white(), t), t_on), Stroke::new(1.0_f32, mix(line_strong(), pink(), t_on)));
    let knob = egui::lerp((track.left() + 11.0)..=(track.right() - 11.0), t_on);
    painter.circle_filled(pos2(knob, track.center().y), 8.0, mix(lilac(), action_text(), t_on));
    painter.galley(pos2(rect.left() + text_x, rect.top()), title_g.clone(), white());
    painter.galley(pos2(rect.left() + text_x, rect.top() + title_g.size().y + 1.0), hint_g, faint());
    if resp.has_focus() {
        painter.rect_stroke(track.expand(3.0), 14.0, Stroke::new(2.0_f32, pink()));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// A small label in spaced capitals, in a tinted pill.
pub fn badge(ui: &mut Ui, label: &str, color: Color32) -> Response {
    let painter = ui.painter().clone();
    let g = caps_galley(&painter, label, 10.0, 0.12);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 18.0, 22.0), Sense::hover());
    painter.rect(rect, 11.0, color.gamma_multiply(0.12), Stroke::new(1.0_f32, color.gamma_multiply(0.4)));
    painter.galley(rect.center() - g.size() / 2.0 + vec2(0.0, 1.0), g, color);
    resp
}

/// A status light.
pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 3.5, color);
}

/// A short upright line between groups in a row.

pub fn icon(ui: &mut Ui, c: char, size: f32, color: Color32) {
    if has_icons() {
        ui.label(RichText::new(c.to_string()).font(glyphs(size)).color(color));
    }
}

/// The one card: a panel lit a little at the top, with a fine border.
pub fn card<R>(ui: &mut Ui, margin: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let back = ui.painter().add(egui::Shape::Noop);
    let resp = Frame::none().stroke(Stroke::new(1.0_f32, line())).rounding(R_LG).inner_margin(Margin::same(margin)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui)
    });
    ui.painter().set(back, panel_shape(resp.response.rect));
    resp.inner
}

// Blurred wallpaper in the same coordinate system as the workspace.
thread_local!{
    static WALL_GLASS:std::cell::Cell<Option<(egui::TextureId,Rect,Rect)>> = const {std::cell::Cell::new(None)};
}
pub fn set_wall_glass(wall:Option<(egui::TextureId,Rect,Rect)>){WALL_GLASS.set(wall);}
const GLASS_OPACITY: u8 = 208;

fn wallpaper_uv(point: Pos2, area: Rect, uv: Rect) -> Pos2 {
    pos2(egui::remap(point.x, area.x_range(), uv.x_range()),
         egui::remap(point.y, area.y_range(), uv.y_range()))
}

fn panel_shape(rect: Rect) -> egui::Shape {
    glass_surface(rect, R_LG, surface())
}

/// One glass outline around the header and the shorter tab strip at its foot.
pub fn profile_glass(header: Rect, tabs: Rect) -> egui::Shape {
    let outer = header.union(tabs);
    let radius = R_LG;
    let shoulder = (header.right() - tabs.right() - radius).min(radius);
    if shoulder < 1.0 {
        return egui::Shape::Vec(vec![
            panel_shape(outer),
            egui::Shape::rect_stroke(outer, radius, Stroke::new(1.0_f32, line())),
        ]);
    }

    let mut edge = Vec::new();
    let mut arc = |cx: f32, cy: f32, r: f32, start: f32, end: f32| {
        for step in 0..=8 {
            let angle = (start + (end - start) * step as f32 / 8.0).to_radians();
            edge.push(pos2(cx + r * angle.cos(), cy + r * angle.sin()));
        }
    };
    arc(outer.left() + radius, outer.top() + radius, radius, 180.0, 270.0);
    arc(header.right() - radius, outer.top() + radius, radius, 270.0, 360.0);
    arc(header.right() - radius, header.bottom() - radius, radius, 0.0, 90.0);
    arc(tabs.right() + shoulder, header.bottom() + shoulder, shoulder, 270.0, 180.0);
    arc(tabs.right() - radius, outer.bottom() - radius, radius, 0.0, 90.0);
    arc(outer.left() + radius, outer.bottom() - radius, radius, 90.0, 180.0);

    // The overlap of the header and the tab strip can see the entire contour.
    let center = pos2(tabs.center().x, header.center().y);
    let mesh_layer = |color, wallpaper: Option<(egui::TextureId, Rect, Rect)>| {
        let mut mesh = egui::Mesh::default();
        if let Some((texture, _, _)) = wallpaper { mesh.texture_id = texture; }
        for pos in std::iter::once(&center).chain(&edge) {
            let uv = wallpaper.map_or(egui::epaint::WHITE_UV, |(_, area, uv)| wallpaper_uv(*pos, area, uv));
            mesh.vertices.push(egui::epaint::Vertex { pos: *pos, uv, color });
        }
        for i in 0..edge.len() {
            mesh.add_triangle(0, (1 + i) as u32, (1 + (i + 1) % edge.len()) as u32);
        }
        egui::Shape::mesh(mesh)
    };
    let mut layers = Vec::new();
    let glass = PALETTE.get().1;
    if let Some(wallpaper) = WALL_GLASS.get().filter(|_| glass) {
        layers.push(mesh_layer(Color32::WHITE, Some(wallpaper)));
    }
    let tint = surface();
    let alpha = if glass { GLASS_OPACITY } else { 255 };
    layers.push(mesh_layer(Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), alpha), None));
    if glass {
        layers.push(mesh_layer(Color32::from_black_alpha((GLASS_DARKNESS.get() as f32 * 1.55).round() as u8), None));
    }
    layers.push(egui::Shape::closed_line(edge, Stroke::new(1.0_f32, line())));
    egui::Shape::Vec(layers)
}

pub fn glass_surface(rect: Rect, radius: f32, tint: Color32) -> egui::Shape {
    let mut layers=Vec::new();
    if let Some((texture,area,uv))=WALL_GLASS.get().filter(|_|PALETTE.get().1){
        let mut glass=egui::epaint::RectShape::filled(rect,radius,Color32::WHITE);
        glass.fill_texture_id=texture;glass.uv=Rect::from_min_max(wallpaper_uv(rect.min,area,uv),wallpaper_uv(rect.max,area,uv));layers.push(glass.into());
    }
    let opacity = if PALETTE.get().1 { GLASS_OPACITY } else { 255 };
    let fill = Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), opacity);
    layers.push(egui::epaint::RectShape::filled(rect,radius,fill).into());
    if PALETTE.get().1 {
        let shade = (GLASS_DARKNESS.get() as f32 * 1.55).round() as u8;
        layers.push(egui::epaint::RectShape::filled(rect, radius, Color32::from_black_alpha(shade)).into());
    }
    egui::Shape::Vec(layers)
}

pub fn glass_frame<R>(ui: &mut Ui, radius: f32, margin: f32, tint: Color32, content: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let back = ui.painter().add(egui::Shape::Noop);
    let frame = Frame::none().stroke(Stroke::new(1.0_f32, line()))
        .rounding(radius).inner_margin(margin).show(ui, content);
    ui.painter().set(back, glass_surface(frame.response.rect, radius, tint));
    frame
}

/// A tinted note: an icon in `tone`, then whatever `add` puts beside it.
pub fn callout<R>(ui: &mut Ui, tone: Color32, icon_char: char, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::none()
        .fill(tone.gamma_multiply(0.07))
        .stroke(Stroke::new(1.0_f32, tone.gamma_multiply(0.32)))
        .rounding(R)
        .inner_margin(Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                if has_icons() {
                    ui.label(RichText::new(icon_char.to_string()).font(glyphs(15.0)).color(tone));
                    ui.add_space(2.0);
                }
                ui.vertical(add).inner
            })
            .inner
        })
        .inner
}

pub struct Stat<'a> {
    pub label: &'a str,
    pub value: String,
    pub color: Color32,
    pub tip: Option<&'a str>,
}

/// Figures side by side on one card, each named above its value.
pub fn stats(ui: &mut Ui, items: &[Stat]) {
    let w = ui.available_width();
    let h = 86.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let painter = ui.painter().clone();
    painter.add(panel_shape(rect));
    painter.rect_stroke(rect, R_LG, Stroke::new(1.0_f32, line()));
    let cw = w / items.len().max(1) as f32;
    for (i, s) in items.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(rect.left() + cw * i as f32, rect.top()), vec2(cw, h));
        if i > 0 {
            painter.vline(cell.left(), (rect.top() + 18.0)..=(rect.bottom() - 18.0), Stroke::new(1.0_f32, line_strong()));
        }
        let label = caps_galley(&painter, s.label, 10.5, 0.16);
        painter.galley(pos2(cell.left() + 20.0, cell.top() + 17.0), label, lilac());
        let value = one_line(ui, &s.value.to_uppercase(), display(32.0), s.color, cw - 34.0);
        painter.galley(pos2(cell.left() + 20.0, cell.top() + 36.0), value, s.color);
        if let Some(tip) = s.tip {
            ui.interact(cell, Id::new(("stat", i)), Sense::hover()).on_hover_text(tip);
        }
    }
}

/// The bottom of a scrolled list fading into `ground`: there is more below.
pub fn fade_bottom(painter: &egui::Painter, rect: Rect, ground: Color32) {
    fade_edge(painter,rect,ground,false);
}

pub fn fade_edge(painter: &egui::Painter, rect: Rect, ground: Color32, top:bool) {
    let (start,end)=if top{(ground,Color32::TRANSPARENT)}else{(Color32::TRANSPARENT,ground)};
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), start);
    mesh.colored_vertex(rect.right_top(), start);
    mesh.colored_vertex(rect.left_bottom(), end);
    mesh.colored_vertex(rect.right_bottom(), end);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

pub fn modal<R>(ctx: &egui::Context, id: &str, opened_at: f64, width: f32, add: impl FnOnce(&mut Ui) -> R) -> (R, bool) {
    let now = ctx.input(|i| i.time);
    let a = ease_out(((now - opened_at) / 0.2) as f32);
    if a < 1.0 {
        ctx.request_repaint();
    }
    let screen = ctx.screen_rect();
    let dim = egui::Area::new(Id::new(id).with("dim"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .constrain(false)
        .show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(5, 5, 14, (185.0 * a) as u8));
            resp
        })
        .inner;
    let inner = egui::Area::new(Id::new(id))
        .order(egui::Order::Tooltip)
        .default_size(vec2(width+58.0,screen.height()-26.0))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -8.0 * (1.0 - a)))
        .show(ctx, |ui| {
            ui.set_opacity(a);
            // The shadow and the panel go under the contents: their places are kept first.
            let shadow_at = ui.painter().add(egui::Shape::Noop);
            let back = ui.painter().add(egui::Shape::Noop);
            let resp = Frame::none().stroke(Stroke::new(1.0_f32, line_strong())).rounding(R_LG).inner_margin(Margin::same(28.0)).show(ui, |ui| {
                ui.set_width(width.min(screen.width()-72.0));
                ui.set_max_height((screen.height()-96.0).max(180.0));
                egui::ScrollArea::vertical().id_salt(id).max_height((screen.height()-96.0).max(180.0)).auto_shrink([false,true]).show(ui,add).inner
            });
            let rect = resp.response.rect;
            ui.painter().set(shadow_at, shadow().as_shape(rect, R_LG));
            ui.painter().set(back, panel_shape(rect));
            resp.inner
        })
        .inner;
    let dismissed = dim.clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape));
    (inner, dismissed)
}
