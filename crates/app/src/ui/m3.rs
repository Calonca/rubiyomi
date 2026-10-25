// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::{
    Align, Align2, Color32, CornerRadius, FontFamily, FontId, Layout, Margin, Rect, Response, Sense, Stroke, TextStyle, Ui, pos2, vec2,
};
use egui_material_icons::MaterialIcon;
use rubiyomi_core::theme::{Color, Scheme};

pub fn c(color: Color) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
}

pub fn alpha(color: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub mod text {
    pub const HEADLINE_SMALL: f32 = 24.0;
    pub const TITLE_LARGE: f32 = 22.0;
    pub const TITLE_MEDIUM: f32 = 16.0;
    pub const BODY_LARGE: f32 = 16.0;
    pub const BODY_MEDIUM: f32 = 14.0;
    pub const BODY_SMALL: f32 = 12.0;
    pub const LABEL_LARGE: f32 = 14.0;
    pub const LABEL_MEDIUM: f32 = 12.0;
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn apply(ctx: &egui::Context, s: &Scheme) {
    let theme = if s.dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.style_mut_of(theme, |style| {
        let mut v = if s.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = c(s.surface);
        v.window_fill = c(s.surface_container_high);
        v.window_stroke = Stroke::NONE;
        v.window_corner_radius = CornerRadius::same(28);
        v.menu_corner_radius = CornerRadius::same(4);
        v.extreme_bg_color = c(s.surface_container_highest);
        v.faint_bg_color = c(s.surface_container_low);
        v.override_text_color = Some(c(s.on_surface));
        v.hyperlink_color = c(s.primary);
        v.error_fg_color = c(s.error);
        v.selection.bg_fill = alpha(c(s.primary), 0.35);
        v.selection.stroke = Stroke::new(1.0, c(s.primary));
        v.slider_trailing_fill = true;
        let radius = CornerRadius::same(20);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = c(s.surface);
        w.noninteractive.weak_bg_fill = c(s.surface);
        w.noninteractive.bg_stroke = Stroke::new(1.0, c(s.outline_variant));
        w.noninteractive.fg_stroke = Stroke::new(1.0, c(s.on_surface));
        for (state, layer) in [(&mut w.inactive, 0.0), (&mut w.hovered, 0.08), (&mut w.active, 0.12), (&mut w.open, 0.12)] {
            let base = c(s.secondary_container);
            state.bg_fill = blend(base, c(s.on_secondary_container), layer);
            state.weak_bg_fill = blend(c(s.surface_container_highest), c(s.on_surface), layer);
            state.bg_stroke = Stroke::NONE;
            state.fg_stroke = Stroke::new(1.5, c(s.on_surface));
            state.corner_radius = radius;
            state.expansion = 0.0;
        }
        w.inactive.bg_fill = c(s.primary);
        w.hovered.bg_fill = c(s.primary);
        w.active.bg_fill = c(s.primary);
        v.widgets.inactive.fg_stroke = Stroke::new(1.5, c(s.on_surface_variant));
        style.visuals = v;
        style.spacing.button_padding = vec2(16.0, 10.0);
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.interact_size.y = 40.0;
        style.spacing.slider_rail_height = 4.0;
        style.text_styles = [
            (TextStyle::Small, font(text::BODY_SMALL)),
            (TextStyle::Body, font(text::BODY_MEDIUM)),
            (TextStyle::Button, font(text::LABEL_LARGE)),
            (TextStyle::Heading, font(text::TITLE_LARGE)),
            (TextStyle::Monospace, FontId::new(text::BODY_MEDIUM, FontFamily::Monospace)),
        ]
        .into();
    });
}

pub fn blend(top: Color32, over: Color32, a: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * a).round() as u8;
    Color32::from_rgb(mix(top.r(), over.r()), mix(top.g(), over.g()), mix(top.b(), over.b()))
}

pub fn insets(ctx: &egui::Context) -> Margin {
    ctx.data(|d| d.get_temp::<Margin>(egui::Id::new("system-insets"))).unwrap_or_default()
}

pub fn set_insets(ctx: &egui::Context, m: Margin) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("system-insets"), m));
}

pub fn paint_icon(ui: &Ui, icon: MaterialIcon, center: egui::Pos2, size: f32, color: Color32) {
    ui.painter().text(center, Align2::CENTER_CENTER, icon.codepoint, FontId::new(size, icon.font_family()), color);
}

pub fn top_app_bar(ui: &mut Ui, s: &Scheme, title: &str) {
    let inset = insets(ui.ctx());
    egui::Panel::top(egui::Id::new(("top-app-bar", title)))
        .exact_size(64.0 + inset.top as f32)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(c(s.surface)).inner_margin(Margin {
            top: inset.top,
            left: inset.left + 16,
            right: inset.right + 4,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.label(egui::RichText::new(title).font(font(text::TITLE_LARGE)).color(c(s.on_surface)));
            });
        });
}

pub fn navigation_bar(ui: &mut Ui, s: &Scheme, items: &[(MaterialIcon, &str)], selected: &mut usize) {
    let inset = insets(ui.ctx());
    egui::Panel::bottom(egui::Id::new("navigation-bar"))
        .exact_size(80.0 + inset.bottom as f32)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(c(s.surface_container)).inner_margin(Margin {
            left: inset.left,
            right: inset.right,
            bottom: inset.bottom,
            top: 0,
        }))
        .show(ui, |ui| {
            let area = ui.max_rect();
            let width = area.width() / items.len() as f32;
            for (i, (icon, label)) in items.iter().enumerate() {
                let rect = Rect::from_min_size(pos2(area.left() + width * i as f32, area.top()), vec2(width, 80.0));
                let response = ui.interact(rect, ui.id().with(("nav", i)), Sense::click()).on_hover_text(*label);
                if response.clicked() {
                    *selected = i;
                }
                let on = *selected == i;
                let pill = Rect::from_center_size(pos2(rect.center().x, rect.top() + 12.0 + 16.0), vec2(64.0, 32.0));
                let layer = if response.is_pointer_button_down_on() {
                    0.12
                } else if response.hovered() {
                    0.08
                } else {
                    0.0
                };
                let p = ui.painter();
                if on {
                    p.rect_filled(pill, CornerRadius::same(16), blend(c(s.secondary_container), c(s.on_secondary_container), layer));
                } else if layer > 0.0 {
                    p.rect_filled(pill, CornerRadius::same(16), alpha(c(s.on_surface), layer));
                }
                let ink = if on { c(s.on_secondary_container) } else { c(s.on_surface_variant) };
                paint_icon(ui, *icon, pill.center(), 24.0, ink);
                let text = if on { c(s.on_surface) } else { c(s.on_surface_variant) };
                p.text(pos2(rect.center().x, pill.bottom() + 4.0), Align2::CENTER_TOP, *label, font(text::LABEL_MEDIUM), text);
            }
        });
}

pub fn empty_state(ui: &mut Ui, s: &Scheme, icon: MaterialIcon, title: &str, body: &str) {
    let rect = ui.max_rect();
    let center = rect.center() - vec2(0.0, 40.0);
    paint_icon(ui, icon, center - vec2(0.0, 56.0), 64.0, c(s.on_surface_variant));
    ui.painter().text(center, Align2::CENTER_CENTER, title, font(text::HEADLINE_SMALL), c(s.on_surface));
    let wrap = (rect.width() - 48.0).max(120.0);
    let mut job = egui::text::LayoutJob::simple(body.to_owned(), font(text::BODY_LARGE), c(s.on_surface_variant), wrap);
    job.halign = Align::Center;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(center.x, center.y + 24.0), galley, c(s.on_surface_variant));
}

pub fn section_header(ui: &mut Ui, s: &Scheme, title: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new(title).font(font(text::LABEL_LARGE)).color(c(s.primary)));
    });
    ui.add_space(4.0);
}

pub fn list_item(ui: &mut Ui, s: &Scheme, icon: MaterialIcon, headline: &str, supporting: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::hover());
    let p = ui.painter();
    paint_icon(ui, icon, pos2(rect.left() + 16.0 + 12.0, rect.center().y), 24.0, c(s.on_surface_variant));
    let x = rect.left() + 16.0 + 24.0 + 16.0;
    let wrap = (rect.right() - 16.0 - x).max(40.0);
    let head = p.layout(headline.to_owned(), font(text::BODY_LARGE), c(s.on_surface), wrap);
    let sub = p.layout(supporting.to_owned(), font(text::BODY_MEDIUM), c(s.on_surface_variant), wrap);
    let top = rect.center().y - (head.size().y + sub.size().y) / 2.0;
    let head_height = head.size().y;
    p.galley(pos2(x, top), head, c(s.on_surface));
    p.galley(pos2(x, top + head_height), sub, c(s.on_surface_variant));
    response
}

pub fn state_layer(response: &Response) -> f32 {
    if response.is_pointer_button_down_on() {
        0.12
    } else if response.hovered() {
        0.08
    } else {
        0.0
    }
}

pub fn icon_button(ui: &mut Ui, s: &Scheme, icon: MaterialIcon, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
    let layer = state_layer(&response);
    if layer > 0.0 {
        ui.painter().circle_filled(rect.center(), 20.0, alpha(c(s.on_surface), layer));
    }
    paint_icon(ui, icon, rect.center(), 24.0, color);
    response
}

pub fn fab(ui: &mut Ui, s: &Scheme, icon: MaterialIcon, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(text::LABEL_LARGE), c(s.on_primary_container));
    let size = vec2(16.0 + 24.0 + 8.0 + galley.size().x + 20.0, 56.0);
    let area = ui.max_rect();
    let rect = Rect::from_min_size(pos2(area.right() - 16.0 - size.x, area.bottom() - 16.0 - size.y), size);
    let response = ui.interact(rect, ui.id().with(("fab", label)), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect.translate(vec2(0.0, 2.0)), CornerRadius::same(16), alpha(c(s.scrim), 0.18));
    p.rect_filled(rect, CornerRadius::same(16), blend(c(s.primary_container), c(s.on_primary_container), state_layer(&response)));
    paint_icon(ui, icon, pos2(rect.left() + 16.0 + 12.0, rect.center().y), 24.0, c(s.on_primary_container));
    p.galley(pos2(rect.left() + 48.0, rect.center().y - galley.size().y / 2.0), galley, c(s.on_primary_container));
    response
}

pub fn text_button(ui: &mut Ui, s: &Scheme, label: &str) -> Response {
    let ink = c(s.primary);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font(text::LABEL_LARGE), ink);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 24.0, 40.0), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(20), alpha(ink, state_layer(&response)));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, ink);
    response
}

pub fn row(ui: &mut Ui, s: &Scheme, headline: &str, supporting: &str, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    let base = if selected { alpha(c(s.secondary_container), 1.0) } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, CornerRadius::same(28), base);
    let layer = state_layer(&response);
    if layer > 0.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(28), alpha(c(s.on_surface), layer));
    }
    let ink = if selected { c(s.on_secondary_container) } else { c(s.on_surface) };
    let right = ui.painter().layout_no_wrap(supporting.to_owned(), font(text::LABEL_MEDIUM), c(s.on_surface_variant));
    let wrap = (rect.width() - 48.0 - right.size().x).max(40.0);
    let mut job = egui::text::LayoutJob::simple_singleline(headline.to_owned(), font(text::LABEL_LARGE), ink);
    job.wrap = egui::text::TextWrapping::truncate_at_width(wrap);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0), galley, ink);
    ui.painter().galley(pos2(rect.right() - 16.0 - right.size().x, rect.center().y - right.size().y / 2.0), right, c(s.on_surface_variant));
    response
}

#[derive(Default)]
pub struct Snackbar {
    message: Option<(String, f64)>,
}

impl Snackbar {
    pub fn show(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        let until = ctx.input(|i| i.time) + 4.0;
        self.message = Some((message.into(), until));
        ctx.request_repaint();
    }

    pub fn ui(&mut self, ctx: &egui::Context, s: &Scheme, lift: f32) {
        let now = ctx.input(|i| i.time);
        let Some((message, until)) = &self.message else { return };
        if now > *until {
            self.message = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(until - now));
        let inset = insets(ctx);
        let screen = ctx.content_rect();
        let width = (screen.width() - 32.0).min(560.0);
        egui::Area::new(egui::Id::new("snackbar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(screen.center().x, screen.bottom() - inset.bottom as f32 - 16.0 - lift))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(c(s.inverse_surface))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(16, 14))
                    .show(ui, |ui| {
                        ui.set_width(width - 32.0);
                        ui.label(egui::RichText::new(message.as_str()).font(font(text::BODY_MEDIUM)).color(c(s.inverse_on_surface)));
                    });
            });
    }
}
