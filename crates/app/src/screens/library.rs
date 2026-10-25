// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};
use egui_material_icons::icons::{ICON_ADD, ICON_DELETE, ICON_LOCAL_LIBRARY};
use rubiyomi_core::library::{BookId, BookMeta, Library, Progress};

use super::Ctx;
use super::reader::ReaderScreen;
use crate::task::Task;
use crate::ui::fonts::BOOK_SERIF;
use crate::ui::m3::{self, c, font, text};

type Decoded = (BookId, egui::ColorImage);

#[derive(Default)]
pub struct LibraryScreen {
    books: Vec<(BookMeta, Progress)>,
    loaded: bool,
    covers: HashMap<BookId, egui::TextureHandle>,
    decoding: Option<Task<Vec<Decoded>>>,
    confirm_remove: Option<BookMeta>,
}

impl LibraryScreen {
    pub fn refresh(&mut self, ctx: &egui::Context, library: &Library) {
        self.books = library.books().into_iter().map(|m| (library.progress(&m.id), m)).map(|(p, m)| (m, p)).collect();
        self.loaded = true;
        let wanted: Vec<(BookId, std::path::PathBuf)> = self
            .books
            .iter()
            .filter(|(m, _)| m.has_cover && !self.covers.contains_key(&m.id))
            .map(|(m, _)| (m.id.clone(), library.cover_path(&m.id)))
            .collect();
        if !wanted.is_empty() {
            self.decoding = Some(Task::spawn(ctx, move || {
                wanted
                    .into_iter()
                    .filter_map(|(id, path)| {
                        let img = image::open(&path).ok()?.thumbnail(480, 720).to_rgba8();
                        let size = [img.width() as usize, img.height() as usize];
                        Some((id, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw())))
                    })
                    .collect()
            }));
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Ctx<'_>) -> Option<ReaderScreen> {
        let s = cx.scheme;
        if !self.loaded {
            self.refresh(ui.ctx(), cx.library);
        }
        if let Some(covers) = crate::task::take(&mut self.decoding) {
            for (id, image) in covers {
                let texture = ui.ctx().load_texture(format!("cover-{id}"), image, egui::TextureOptions::LINEAR);
                self.covers.insert(id, texture);
            }
        }
        let mut open = None;
        m3::top_app_bar(ui, s, "Library", false);
        egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
            if self.books.is_empty() {
                m3::empty_state(ui, s, ICON_LOCAL_LIBRARY, "No books yet", "Add an EPUB to start reading.");
            } else {
                open = self.grid(ui, cx);
            }
            if m3::fab(ui, s, ICON_ADD, "Add book").clicked() {
                cx.platform.pick_epub();
            }
        });
        self.confirm(ui.ctx(), cx);
        open
    }

    fn grid(&mut self, ui: &mut Ui, cx: &mut Ctx<'_>) -> Option<ReaderScreen> {
        let s = cx.scheme;
        let mut open = None;
        let inset = m3::insets(ui.ctx());
        let width = ui.available_width() - 32.0 - (inset.left + inset.right) as f32;
        let columns = ((width + 16.0) / (104.0 + 16.0)).floor().max(2.0) as usize;
        let cell_w = (width - 16.0 * (columns as f32 - 1.0)) / columns as f32;
        let cover_h = cell_w * 1.5;
        let cell_h = cover_h + 8.0 + 8.0 + 2.0 * 20.0 + 18.0;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(8.0);
            let rows = self.books.len().div_ceil(columns);
            let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * (cell_h + 16.0) + 88.0), Sense::hover());
            let left = area.left() + 16.0 + inset.left as f32;
            for (i, (meta, progress)) in self.books.iter().enumerate() {
                let (row, col) = (i / columns, i % columns);
                let min = pos2(left + col as f32 * (cell_w + 16.0), area.top() + row as f32 * (cell_h + 16.0));
                let rect = Rect::from_min_size(min, vec2(cell_w, cell_h));
                let response = ui.interact(rect, ui.id().with(("book", &meta.id)), Sense::click());
                response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &meta.title));
                let cover = Rect::from_min_size(min, vec2(cell_w, cover_h));
                let front = paint_book(ui, cover, self.covers.get(&meta.id), meta);
                let layer = m3::state_layer(&response);
                if layer > 0.0 {
                    ui.painter().rect_filled(front, COVER_RADIUS, m3::alpha(c(s.on_surface), layer));
                }
                let bar = Rect::from_min_size(pos2(min.x, cover.bottom() + 8.0), vec2(cell_w, 4.0));
                if progress.updated != 0 {
                    m3::progress(ui.painter(), s, bar, progress.fraction);
                }
                let title = ui.painter().layout(meta.title.clone(), font(text::TITLE_MEDIUM - 2.0), c(s.on_surface), cell_w);
                let title_rows = title.rows.len().min(2);
                let title_rect = Rect::from_min_size(pos2(min.x, bar.bottom() + 8.0), vec2(cell_w, 40.0));
                ui.painter().with_clip_rect(title_rect).galley(title_rect.min, title, c(s.on_surface));
                let percent = (progress.fraction * 100.0).round() as u32;
                let line = if progress.updated == 0 { meta.author.clone() } else { format!("{percent}% · {}", meta.author) };
                let at = pos2(min.x, title_rect.top() + 20.0 * title_rows as f32);
                ui.painter().with_clip_rect(Rect::from_min_size(at, vec2(cell_w, 18.0))).text(
                    at,
                    Align2::LEFT_TOP,
                    line,
                    font(text::BODY_SMALL),
                    c(s.on_surface_variant),
                );
                if response.clicked() {
                    match ReaderScreen::open(cx.library, meta.clone(), None) {
                        Ok(reader) => open = Some(reader),
                        Err(e) => cx.snackbar.show(ui.ctx(), e),
                    }
                }
                response.context_menu(|ui| {
                    if ui.button(format!("{}  Remove", ICON_DELETE.codepoint)).clicked() {
                        self.confirm_remove = Some(meta.clone());
                        ui.close();
                    }
                });
            }
        });
        open
    }

    fn confirm(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>) {
        let s = cx.scheme;
        let Some(meta) = self.confirm_remove.clone() else { return };
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("remove-book")).show(ctx, |ui| {
            ui.set_width(320.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.label(egui::RichText::new("Remove this book?").font(font(text::HEADLINE_SMALL)));
            ui.add_space(8.0);
            ui.label(format!("{} and your place in it will be removed from this device.", meta.title));
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if m3::text_button(ui, s, "Remove").clicked() {
                    match cx.library.delete(&meta.id) {
                        Ok(()) => cx.snackbar.show(ctx, format!("Removed {}", meta.title)),
                        Err(e) => cx.snackbar.show(ctx, format!("Couldn't remove it: {e}")),
                    }
                    self.covers.remove(&meta.id);
                    close = true;
                }
                if m3::text_button(ui, s, "Cancel").clicked() {
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            self.confirm_remove = None;
            self.refresh(ctx, cx.library);
        }
    }
}

// The spine is on the left, so its corners are the sharper ones.
const COVER_RADIUS: CornerRadius = CornerRadius { nw: 2, sw: 2, ne: 5, se: 5 };

fn gradient(p: &egui::Painter, r: Rect, left: Color32, right: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), left);
    mesh.colored_vertex(r.right_top(), right);
    mesh.colored_vertex(r.right_bottom(), right);
    mesh.colored_vertex(r.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

// A cloth colour for a book without a cover, the same for the same title every time.
fn cloth(title: &str) -> Color32 {
    let hash = title.chars().fold(2166136261u32, |h, ch| (h ^ ch as u32).wrapping_mul(16777619));
    let hue = (hash % 360) as f32 / 360.0;
    egui::ecolor::Hsva::new(hue, 0.45, 0.42, 1.0).into()
}

// A book seen from the front: a shadow under it, the edges of its pages on the right,
// the cover, and the spine's fold. Returns the cover's rect.
fn paint_book(ui: &Ui, rect: Rect, texture: Option<&egui::TextureHandle>, meta: &BookMeta) -> Rect {
    let p = ui.painter();
    let front = Rect::from_min_max(rect.min, pos2(rect.right() - 4.0, rect.bottom()));
    let pages = Rect::from_min_max(pos2(front.right() - 6.0, rect.top() + 3.0), pos2(rect.right(), rect.bottom() - 3.0));
    let shadow = egui::epaint::Shadow { offset: [2, 4], blur: 10, spread: 0, color: Color32::from_black_alpha(90) };
    p.add(shadow.as_shape(front, COVER_RADIUS));
    p.rect_filled(pages, CornerRadius { nw: 0, sw: 0, ne: 3, se: 3 }, Color32::from_rgb(0xee, 0xe6, 0xd3));
    for k in 1..3 {
        let x = pages.right() - k as f32 * 1.5;
        p.vline(x, pages.top() + 1.0..=pages.bottom() - 1.0, egui::Stroke::new(0.5, Color32::from_rgb(0xc9, 0xbf, 0xa8)));
    }
    match texture {
        Some(t) => {
            let [w, h] = t.size().map(|v| v as f32);
            let (cell_ratio, img_ratio) = (front.width() / front.height(), w / h);
            let uv = if img_ratio > cell_ratio {
                let span = cell_ratio / img_ratio;
                Rect::from_min_max(pos2((1.0 - span) / 2.0, 0.0), pos2((1.0 + span) / 2.0, 1.0))
            } else {
                let span = img_ratio / cell_ratio;
                Rect::from_min_max(pos2(0.0, (1.0 - span) / 2.0), pos2(1.0, (1.0 + span) / 2.0))
            };
            egui::Image::new(t).uv(uv).corner_radius(COVER_RADIUS).paint_at(ui, front);
        }
        None => {
            let base = cloth(&meta.title);
            p.rect_filled(front, COVER_RADIUS, base);
            let ink = Color32::from_rgb(0xf3, 0xec, 0xdc);
            let frame = front.shrink2(vec2(front.width() * 0.1 + 4.0, front.width() * 0.1));
            p.rect_stroke(frame, CornerRadius::ZERO, egui::Stroke::new(1.0, m3::alpha(ink, 0.45)), egui::StrokeKind::Inside);
            let wrap = frame.width() - 12.0;
            let family = egui::FontFamily::Name(BOOK_SERIF.into());
            let title = p.layout(meta.title.clone(), egui::FontId::new((front.width() / 7.0).clamp(12.0, 20.0), family.clone()), ink, wrap);
            let at = pos2(frame.center().x - title.size().x / 2.0, frame.top() + frame.height() * 0.22);
            p.with_clip_rect(frame.shrink(4.0)).galley(at, title, ink);
            if !meta.author.is_empty() {
                let author = p.layout(meta.author.clone(), egui::FontId::new(11.0, family), m3::alpha(ink, 0.8), wrap);
                let at = pos2(frame.center().x - author.size().x / 2.0, frame.bottom() - 10.0 - author.size().y);
                p.with_clip_rect(frame.shrink(4.0)).galley(at, author, m3::alpha(ink, 0.8));
            }
        }
    }
    // The fold where the cover bends round the spine, and light falling across it.
    let fold = front.left() + front.width() * 0.07;
    gradient(p, Rect::from_min_max(front.min, pos2(fold, front.bottom())), Color32::from_black_alpha(70), Color32::from_black_alpha(10));
    p.vline(fold, front.y_range(), egui::Stroke::new(1.0, Color32::from_white_alpha(28)));
    p.vline(fold + 1.5, front.y_range(), egui::Stroke::new(1.0, Color32::from_black_alpha(40)));
    gradient(p, Rect::from_min_max(pos2(fold + 2.0, front.top()), front.max), Color32::from_white_alpha(14), Color32::TRANSPARENT);
    front
}
