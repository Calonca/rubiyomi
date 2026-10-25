// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;

use egui::{Align2, CornerRadius, Rect, Sense, Ui, pos2, vec2};
use egui_material_icons::icons::{ICON_ADD, ICON_DELETE, ICON_LOCAL_LIBRARY};
use rubiyomi_core::library::{BookId, BookMeta, Library, Progress};
use rubiyomi_core::theme::Scheme;

use super::Ctx;
use super::reader::ReaderScreen;
use crate::task::Task;
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
        let cell_h = cover_h + 8.0 + 2.0 * 20.0 + 18.0;
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
                paint_cover(ui, s, cover, self.covers.get(&meta.id), &meta.title);
                let layer = m3::state_layer(&response);
                if layer > 0.0 {
                    ui.painter().rect_filled(cover, CornerRadius::same(12), m3::alpha(c(s.on_surface), layer));
                }
                let title = ui.painter().layout(meta.title.clone(), font(text::TITLE_MEDIUM - 2.0), c(s.on_surface), cell_w);
                let title_rows = title.rows.len().min(2);
                let title_rect = Rect::from_min_size(pos2(min.x, cover.bottom() + 8.0), vec2(cell_w, 40.0));
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
                    match ReaderScreen::open(cx.library, meta.clone()) {
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

fn paint_cover(ui: &Ui, s: &Scheme, rect: Rect, texture: Option<&egui::TextureHandle>, title: &str) {
    let radius = CornerRadius::same(12);
    match texture {
        Some(t) => {
            let [w, h] = t.size().map(|v| v as f32);
            let (cell_ratio, img_ratio) = (rect.width() / rect.height(), w / h);
            let uv = if img_ratio > cell_ratio {
                let span = cell_ratio / img_ratio;
                Rect::from_min_max(pos2((1.0 - span) / 2.0, 0.0), pos2((1.0 + span) / 2.0, 1.0))
            } else {
                let span = img_ratio / cell_ratio;
                Rect::from_min_max(pos2(0.0, (1.0 - span) / 2.0), pos2(1.0, (1.0 + span) / 2.0))
            };
            egui::Image::new(t).uv(uv).corner_radius(radius).paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, radius, c(s.secondary_container));
            let galley = ui.painter().layout(title.to_owned(), font(text::TITLE_MEDIUM), c(s.on_secondary_container), rect.width() - 24.0);
            ui.painter().with_clip_rect(rect.shrink(12.0)).galley(rect.min + vec2(12.0, 16.0), galley, c(s.on_secondary_container));
        }
    }
}
