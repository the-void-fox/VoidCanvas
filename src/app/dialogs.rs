//! Modal windows: new document, resize, about, unsaved changes.

use eframe::egui::{self, *};

use super::{AsciiArtEditor, PendingAction, VERSION};
use crate::theme;
use crate::document::Document;

impl AsciiArtEditor {
    pub(super) fn new_doc_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_new_dialog;
        egui::Window::new("New Document")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(DragValue::new(&mut self.new_width).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(DragValue::new(&mut self.new_height).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    for (label, w, h) in [("40×25",40,25),("80×25",80,25),("80×40",80,40),("120×50",120,50)] {
                        if ui.button(label).clicked() { self.new_width = w; self.new_height = h; }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        self.load_document(Document::new(self.new_width, self.new_height));
                        self.show_new_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_new_dialog = false;
                    }
                });
            });
        self.show_new_dialog &= open;
    }

    pub(super) fn resize_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_resize_dialog;
        egui::Window::new("Resize Canvas")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Current: {}×{}", self.doc.width, self.doc.height));
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(DragValue::new(&mut self.new_width).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(DragValue::new(&mut self.new_height).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    if ui.button("Resize").clicked() {
                        self.resize_canvas(self.new_width, self.new_height);
                        self.show_resize_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_resize_dialog = false;
                    }
                });
            });
        self.show_resize_dialog &= open;
    }

    pub(super) fn about_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about_dialog;
        egui::Window::new("About Void Canvas")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .min_width(720.0)
            .show(ctx, |ui| {
                // ── Tab bar ──────────────────────────────────────────────────
                ui.horizontal(|ui| {
                    for (idx, label) in ["About & Shortcuts", "Author"].iter().enumerate() {
                        let active = self.about_tab == idx;
                        let btn = egui::Button::new(
                            egui::RichText::new(*label).strong()
                        )
                        .fill(if active {
                            crate::theme::ACCENT_STRONG
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        });
                        if ui.add(btn).clicked() {
                            self.about_tab = idx;
                        }
                    }
                });
                ui.separator();

                match self.about_tab {
                    // ── Tab 0: About & Shortcuts ─────────────────────────────
                    0 => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            let mut title = text::LayoutJob::default();
                            title.append("▞  ", 0.0, TextFormat::simple(theme::mono(26.0), theme::ACCENT));
                            title.append("Void Canvas", 0.0, TextFormat::simple(theme::semibold(26.0), theme::TEXT));
                            ui.label(title);
                            ui.label(RichText::new(format!("v{VERSION}")).color(theme::TEXT_DIM));
                            ui.add_space(4.0);
                            ui.label("ASCII art editor with layers, history & rich tools.");
                            ui.add_space(12.0);
                        });

                        ui.separator();
                        ui.add_space(6.0);

                        const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
                            ("File", &[
                                ("Ctrl+N", "New document"),
                                ("Ctrl+O", "Open…"),
                                ("Ctrl+S", "Save"),
                                ("Ctrl+Shift+S", "Save as…"),
                            ]),
                            ("Edit", &[
                                ("Ctrl+Z", "Undo"),
                                ("Ctrl+Y, Ctrl+Shift+Z", "Redo"),
                                ("Ctrl+C / X / V", "Copy / cut / paste"),
                                ("Ctrl+A  /  Ctrl+D", "Select all / deselect"),
                                ("Ctrl+H", "Replace glyphs / colours"),
                                ("Delete", "Delete selection"),
                                ("H  /  V", "Flip selection"),
                                ("Arrows (+Shift)", "Move selection 1 (5)"),
                                ("X", "Swap primary / secondary"),
                            ]),
                            ("Tools", &[
                                ("S", "Select"),
                                ("B  P  G", "Brush, pixel, shade"),
                                ("E  F  I", "Eraser, fill, eyedropper"),
                                ("T", "Text"),
                                ("L  R  O  D", "Line, rect, ellipse, border"),
                                ("[  ]  or  –  +", "Brush size"),
                                ("Right button", "Use the secondary slot"),
                                ("Alt+click", "Pick glyph and colours"),
                                ("Shift  /  Alt", "Square shape / from centre"),
                            ]),
                            ("Canvas", &[
                                ("Scroll, Space+drag", "Pan"),
                                ("Ctrl+Scroll, Ctrl ±", "Zoom"),
                                ("Ctrl+0", "Fit to window"),
                                ("Ctrl+1", "Actual size"),
                            ]),
                        ];
                        ui.columns(2, |columns| {
                            for (column, sections) in columns.iter_mut().zip(SHORTCUTS.chunks(2)) {
                                egui::Grid::new(("shortcuts_grid", sections[0].0))
                                    .num_columns(2)
                                    .spacing([18.0, 5.0])
                                    .show(column, |ui| {
                                        for (i, (section, rows)) in sections.iter().enumerate() {
                                            if i > 0 {
                                                ui.label("");
                                                ui.end_row();
                                            }
                                            ui.label(RichText::new(*section).font(theme::semibold(13.0)).color(theme::ACCENT));
                                            ui.end_row();
                                            for (keys, action) in rows.iter() {
                                                ui.label(RichText::new(*keys).font(theme::mono(12.5)).color(theme::TEXT_DIM));
                                                ui.label(*action);
                                                ui.end_row();
                                            }
                                        }
                                    });
                            }
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("Built with Rust · egui · eframe")
                                    .size(11.0)
                                    .color(egui::Color32::from_gray(120)),
                            );
                        });
                    }

                    // ── Tab 1: Author ────────────────────────────────────────
                    _ => {
                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("✏ Author")
                                    .size(22.0)
                                    .strong(),
                            );
                        });
                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // ── Fill in your own details below ──────────────────
                        egui::Grid::new("author_grid")
                            .num_columns(2)
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Name").strong());
                                ui.label("Fantomm");
                                ui.end_row();

                                ui.label(egui::RichText::new("GitHub").strong());
                                ui.label("https://github.com/fantommripper");
                                ui.end_row();

                                ui.label(egui::RichText::new("License").strong());
                                ui.label("Modified MIT");
                                ui.end_row();
                            });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Did you actually go to that tab? Okay, thanks for that! From here on out, I'll switch to a language I'm more comfortable using, but trust me, there's nothing important there.")
                                .size(11.0)
                                .color(egui::Color32::from_gray(100))
                                .italics(),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Чтож, если вы нарисуете хоть один рисунок в этой программе, вы меня уже очень обрадуете! Честно говоря, это, наверное, самое интересное, что я смог сделать. Я благодарен всем разработчикам инструментов, которые помогли мне в этом. Хотя я чувствую себя паршиво за использование ИИ в этом проекте. Да, конечно, вою архитектуру придумал я, выбрал инструменты я... но код всё же я своим не считаю. Я вносил правки, но это только правки. Мне не нравится, что люди думают, будто я \"отличный программист\", предполагают, что я \"мидл\", и так далее... Да, я что-то знаю и понимаю, что пишет ИИ и как этот код работает, но всё же лично я не считаю себя программистом. Если вы дочитали до сюда, я ещё раз поблагодарю за всё! А сейчас я пойду и выпью таблетки, пока я снова не придумал очередной никому не нужный проект)")
                                .size(11.0)
                                .color(egui::Color32::from_gray(100))
                                .italics(),
                        );
                    }
                }
            });
        self.show_about_dialog = open;
    }

    pub(super) fn unsaved_changes_modal(&mut self, ctx: &egui::Context) {
        enum Choice { Save, Discard, Cancel }

        let Some(action) = self.pending_action.clone() else { return };
        let what = match action {
            PendingAction::New  => "creating a new document",
            PendingAction::Open | PendingAction::OpenPath(_) => "opening another file",
            PendingAction::Quit => "quitting",
        };
        let name = self.doc_name();

        let mut choice = None;
        let modal = Modal::new(Id::new("unsaved_changes")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Unsaved changes");
            ui.add_space(6.0);
            ui.label(format!("Save changes to \"{name}\" before {what}?"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked()       { choice = Some(Choice::Save); }
                if ui.button("Don't Save").clicked() { choice = Some(Choice::Discard); }
                if ui.button("Cancel").clicked()     { choice = Some(Choice::Cancel); }
            });
        });
        if choice.is_none() && modal.should_close() {
            choice = Some(Choice::Cancel);
        }

        match choice {
            Some(Choice::Save) => {
                self.pending_action = None;
                self.save_then(Some(action), ctx);
            }
            Some(Choice::Discard) => {
                self.pending_action = None;
                self.perform(action, ctx);
            }
            Some(Choice::Cancel) => self.pending_action = None,
            None => {}
        }
    }

    /// After a crash: offers back the unsaved work found in the autosave.
    pub(super) fn recovery_modal(&mut self, ctx: &egui::Context) {
        let Some(autosave) = &self.recovery else { return };
        let name = autosave
            .file_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_owned());
        let when = match autosave.age.map(|age| age.as_secs() / 60) {
            Some(0) => "less than a minute ago".to_owned(),
            Some(1) => "a minute ago".to_owned(),
            Some(m) if m < 120 => format!("{m} minutes ago"),
            Some(m) => format!("{} hours ago", m / 60),
            None => "earlier".to_owned(),
        };

        let mut recover = None;
        Modal::new(Id::new("recover_autosave")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.heading("Recover unsaved work?");
            ui.add_space(6.0);
            ui.label(format!("Void Canvas didn't close properly. Unsaved changes to \"{name}\" were kept {when}."));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Recover").clicked() { recover = Some(true); }
                if ui.button("Discard").clicked() { recover = Some(false); }
            });
        });

        match recover {
            Some(true) => {
                let autosave = self.recovery.take().unwrap();
                let mut doc = autosave.document;
                doc.file_path = autosave.file_path;
                self.load_document(doc);
                self.doc.modified = true;
                self.show_status("Recovered unsaved work");
            }
            Some(false) => {
                self.recovery = None;
                super::storage::Autosave::remove();
            }
            None => {}
        }
    }
}
