use super::*;
use egui::{Align2, Response};

/// Fade just the scroll content so the glass underneath stays continuous.
pub(super) fn fade_edges(
    ui: &Ui,
    start: egui::layers::ShapeIdx,
    rect: Rect,
    top: f32,
    bottom: f32,
) {
    let middle = Rect::from_min_max(
        pos2(rect.left(), rect.top() + top.min(rect.height() * 0.45)),
        pos2(
            rect.right(),
            rect.bottom() - bottom.min(rect.height() * 0.55),
        ),
    );
    ui.ctx().graphics_mut(|graphics| {
        let Some(list) = graphics.get_mut(ui.layer_id()) else {
            return;
        };
        let end = list.next_idx().0;
        for index in start.0 + 1..end {
            let mut fragments = Vec::new();
            list.mutate_shape(egui::layers::ShapeIdx(index), |item| {
                let bounds = item
                    .shape
                    .visual_bounding_rect()
                    .intersect(item.clip_rect)
                    .intersect(rect);
                if !bounds.is_positive() || middle.contains_rect(bounds) {
                    return;
                }
                for (begin, end, reverse) in [
                    (rect.top(), middle.top(), false),
                    (middle.bottom(), rect.bottom(), true),
                ] {
                    if end <= bounds.top() || begin >= bounds.bottom() {
                        continue;
                    }
                    let steps = ((end - begin) / 2.0).ceil().max(1.0) as usize;
                    for step in 0..steps {
                        let a = step as f32 / steps as f32;
                        let b = (step + 1) as f32 / steps as f32;
                        let clip = Rect::from_min_max(
                            pos2(rect.left(), egui::lerp(begin..=end, a)),
                            pos2(rect.right(), egui::lerp(begin..=end, b)),
                        )
                        .intersect(item.clip_rect);
                        if !clip.intersects(bounds) {
                            continue;
                        }
                        let opacity = if reverse {
                            1.0 - (a + b) * 0.5
                        } else {
                            (a + b) * 0.5
                        };
                        let mut shape = item.shape.clone();
                        if let egui::Shape::Text(text) = &mut shape {
                            text.opacity_factor *= opacity;
                        } else {
                            egui::epaint::shape_transform::adjust_colors(&mut shape, move |c| {
                                *c = c.gamma_multiply(opacity)
                            });
                        }
                        fragments.push((clip, shape));
                    }
                }
                item.clip_rect = item.clip_rect.intersect(middle);
            });
            for (clip, shape) in fragments {
                list.add(clip, shape);
            }
        }
    });
}

pub(super) fn palm(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(42.0, 43.0), Sense::hover());
    let id = egui::Id::new("palmvi-sidebar-logo");
    let colors = [theme::pink(), theme::good()];
    let cached = ui.ctx().data(|d| d.get_temp::<([Color32; 2], egui::TextureHandle)>(id));
    let texture = match cached {
        Some((palette, texture)) if palette == colors => texture,
        _ => {
            let mut mark = image::load_from_memory(include_bytes!("../../../data/palmvi-mark.png"))
                .expect("bundled PalmVI logo")
                .into_rgba8();
            for pixel in mark.pixels_mut() {
                let color = if pixel[1] > pixel[0] { colors[1] } else { colors[0] };
                pixel.0 = [color.r(), color.g(), color.b(), pixel[3]];
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [mark.width() as usize, mark.height() as usize],
                mark.as_raw(),
            );
            let texture = ui.ctx().load_texture("PalmVI logo", image, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|d| d.insert_temp(id, (colors, texture.clone())));
            texture
        }
    };
    let bounds = egui::Rect::from_center_size(r.center(), vec2(42.0, 42.0));
    egui::Image::new(&texture).paint_at(ui, bounds);
}
pub(super) fn nav_item(
    ui: &mut Ui,
    label: &str,
    icon: char,
    active: bool,
    count: Option<usize>,
) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let p = ui.painter();
    if active || resp.hovered() {
        p.rect_filled(
            r,
            7.0,
            if active {
                theme::mix(theme::deep(), theme::pink(), 0.19)
            } else {
                theme::raised()
            },
        );
    }
    if active {
        p.vline(
            r.left() + 1.0,
            r.top() + 4.0..=r.bottom() - 4.0,
            Stroke::new(2.0_f32, theme::pink()),
        );
    }
    let c = if active {
        theme::white()
    } else {
        theme::muted()
    };
    let at = pos2(r.left() + 20.0, r.center().y);
    if label == "Activity" {
        pulse(p, at, c);
    } else if label == "Discover" {
        p.circle_stroke(at, 7.0, Stroke::new(1.3_f32, c));
        p.line_segment(
            [at + vec2(-3.0, 3.0), at + vec2(3.0, -3.0)],
            Stroke::new(1.8_f32, c),
        );
    } else {
        p.text(at, Align2::CENTER_CENTER, icon, theme::glyphs(16.0), c);
    }
    p.text(
        pos2(r.left() + 40.0, r.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::strong(14.0),
        c,
    );
    if let Some(n) = count.filter(|n| *n > 0) {
        let badge = Rect::from_center_size(pos2(r.right() - 19.0, r.center().y), vec2(27.0, 21.0));
        p.rect_filled(badge, 4.0, theme::deep());
        p.text(
            badge.center(),
            Align2::CENTER_CENTER,
            n,
            theme::regular(11.0),
            theme::muted(),
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
pub(in crate::ui) fn tabs(
    ui: &mut Ui,
    items: &[(usize, String)],
    selected: usize,
) -> Option<usize> {
    theme::glass_frame(ui, 9.0, 3.0, theme::deep(), |ui| tab_row(ui, items, selected)).inner
}

pub(in crate::ui) fn tab_row(ui: &mut Ui, items: &[(usize, String)], selected: usize) -> Option<usize> {
    let mut next = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, label) in items {
            let g = ui.painter().layout_no_wrap(
                label.clone(),
                theme::strong(13.0),
                Color32::PLACEHOLDER,
            );
            let (r, b) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 31.0), Sense::click());
            if selected == *i || b.hovered() {
                ui.painter().rect_filled(r, 6.0, theme::raised());
            }
            ui.painter().galley(
                r.center() - g.size() / 2.0,
                g,
                if selected == *i {
                    theme::white()
                } else {
                    theme::muted()
                },
            );
            if b.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                next = Some(*i);
            }
        }
    });
    next
}
pub(super) fn sharing_button(ui: &mut Ui, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(120.0, 32.0), Sense::click());
    let painter = ui.painter();
    let color = if active {
        theme::lilac()
    } else {
        theme::muted()
    };
    if active || response.hovered() {
        painter.rect_filled(rect, 6.0, theme::raised());
    }
    let label = painter.layout_no_wrap("Sharing mode".into(), theme::strong(13.0), color);
    let eye = pos2(
        rect.center().x - (label.size().x + 7.0) * 0.5 + 4.0,
        rect.center().y,
    );
    for bend in [-10.0, 10.0] {
        painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            [
                eye - vec2(8.0, 0.0),
                eye + vec2(0.0, bend),
                eye + vec2(8.0, 0.0),
            ],
            false,
            Color32::TRANSPARENT,
            Stroke::new(1.4_f32, color),
        ));
    }
    painter.circle_filled(eye, 2.1, color);
    painter.galley(
        pos2(eye.x + 15.0, eye.y - label.mesh_bounds.center().y),
        label,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, active, "Sharing mode")
    });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if active {
            "Sharing mode on"
        } else {
            "Hide project names and links for screenshots"
        })
}

pub(in crate::ui) fn repo_link(ui: &mut Ui, url: &str, browser_link: bool) -> Response {
    let width = ui.available_width().max(0.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        let label = theme::one_line(
            ui,
            url,
            theme::regular(12.0),
            theme::lilac(),
            rect.width() - 4.0,
        );
        let origin = pos2(
            rect.left(),
            rect.center().y - label.mesh_bounds.center().y,
        );
        let elided = label.elided;
        painter.galley(origin, label, theme::lilac());
        if elided {
            response.clone().on_hover_text(url);
        }
    } else {
        // One pre-blurred sample asset for every repo, independent of the address and its length.
        let id = egui::Id::new("repo-placeholder-texture");
        let texture = ui
            .ctx()
            .data(|d| d.get_temp::<egui::TextureHandle>(id))
            .unwrap_or_else(|| {
                let pixels =
                    image::load_from_memory(include_bytes!("../../../data/repo-placeholder.png"))
                        .expect("bundled repository placeholder")
                        .into_rgba8();
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [pixels.width() as usize, pixels.height() as usize],
                    pixels.as_raw(),
                );
                let texture =
                    ui.ctx()
                        .load_texture("repo-placeholder", image, egui::TextureOptions::LINEAR);
                ui.ctx().data_mut(|d| d.insert_temp(id, texture.clone()));
                texture
            });
        let size = texture.size_vec2() * 0.25;
        let anchor = rect.left_center() - vec2(0.0, size.y * 0.5);
        painter.image(
            texture.id(),
            Rect::from_min_size(anchor, size),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            theme::muted(),
        );
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, "Repository link"));
    if browser_link {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

pub(super) fn utility(ui: &mut Ui, width: f32, icon: char, label: &str, active: bool) -> Response {
    let (r, b) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
    let c = if active || b.hovered() {
        theme::lilac()
    } else {
        theme::muted()
    };
    ui.painter().rect(
        r,
        6.0,
        if active || b.hovered() {
            theme::raised()
        } else {
            Color32::from_white_alpha(5)
        },
        Stroke::new(1.0_f32, theme::line()),
    );
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        icon,
        theme::glyphs(16.0),
        c,
    );
    b.on_hover_text(label)
}
pub(in crate::ui) fn pulse(p: &egui::Painter, at: egui::Pos2, c: Color32) {
    let points = [
        (-8., 0.),
        (-4., 0.),
        (-2., -6.),
        (1., 6.),
        (4., -1.),
        (7., -1.),
    ]
    .map(|(x, y)| at + vec2(x, y));
    p.add(egui::Shape::line(points.to_vec(), Stroke::new(1.4_f32, c)));
}
