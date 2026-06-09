use crate::config::Config;
use crate::news::{self, NewsState};
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(PartialEq)]
enum Tab {
    News,
    Settings,
}

pub struct LauncherApp {
    config: Config,
    active_tab: Tab,
    news_state: Arc<Mutex<NewsState>>,
    // Settings tab buffers (committed on Save)
    edit_news_url: String,
    edit_game_binary: String,
    launch_error: Option<String>,
}

impl LauncherApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = Config::load();
        let news_state: Arc<Mutex<NewsState>> = Arc::new(Mutex::new(NewsState::Idle));
        news::start_fetch(config.news_url.clone(), Arc::clone(&news_state));
        Self {
            edit_news_url: config.news_url.clone(),
            edit_game_binary: config.game_binary.clone(),
            config,
            active_tab: Tab::News,
            news_state,
            launch_error: None,
        }
    }

    fn ui_header(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.heading(
                egui::RichText::new("Georgikon")
                    .size(28.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Social MMORPG")
                    .size(13.0)
                    .weak(),
            );
            ui.add_space(4.0);
        });
    }

    fn ui_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_tab, Tab::News, "📰  News");
            ui.selectable_value(&mut self.active_tab, Tab::Settings, "⚙  Settings");
        });
    }

    fn ui_news(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let state = self.news_state.lock().unwrap();
        match &*state {
            NewsState::Idle => {
                ui.vertical_centered(|ui| {
                    ui.add_space(32.0);
                    ui.label("No news feed configured.");
                    ui.small("Set a URL in Settings to receive updates.");
                });
            }
            NewsState::Loading => {
                ui.vertical_centered(|ui| {
                    ui.add_space(32.0);
                    ui.spinner();
                    ui.small("Fetching news…");
                });
                // Poll until the background thread finishes
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            NewsState::Error(e) => {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 80), "Failed to load news.");
                    ui.small(e);
                });
            }
            NewsState::Loaded(items) if items.is_empty() => {
                ui.vertical_centered(|ui| {
                    ui.add_space(32.0);
                    ui.label("No posts yet — check back later.");
                });
            }
            NewsState::Loaded(items) => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for item in items {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(8, 8))
                                .outer_margin(egui::Margin::symmetric(0, 4))
                                .corner_radius(4)
                                .fill(ui.visuals().extreme_bg_color)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.strong(&item.title);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.small(&item.date);
                                            },
                                        );
                                    });
                                    ui.add_space(4.0);
                                    ui.label(&item.body);
                                    if let Some(url) = &item.url {
                                        ui.add_space(4.0);
                                        ui.hyperlink_to("Read more →", url);
                                    }
                                });
                        }
                    });
            }
        }
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .min_col_width(120.0)
            .show(ui, |ui| {
                ui.label("News feed URL");
                ui.add(
                    egui::TextEdit::singleline(&mut self.edit_news_url)
                        .hint_text("https://example.com/news.json")
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();

                ui.label("Game binary");
                ui.add(
                    egui::TextEdit::singleline(&mut self.edit_game_binary)
                        .hint_text("auto-detect")
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
            });

        ui.add_space(12.0);

        let detected = crate::game::resolve_binary(&self.config.game_binary);
        match detected {
            Some(p) => ui.small(format!("Binary: {}", p.display())),
            None => ui.small(
                egui::RichText::new("Binary not found — install georgikon or set a path above.")
                    .color(egui::Color32::from_rgb(220, 150, 50)),
            ),
        };

        ui.add_space(12.0);

        if ui.button("  Save  ").clicked() {
            self.config.news_url = self.edit_news_url.trim().to_string();
            self.config.game_binary = self.edit_game_binary.trim().to_string();
            self.config.save();
            news::start_fetch(self.config.news_url.clone(), Arc::clone(&self.news_state));
        }
    }

    fn ui_play_bar(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let can_play = crate::game::resolve_binary(&self.config.game_binary).is_some();
            let btn = ui.add_enabled(
                can_play,
                egui::Button::new(
                    egui::RichText::new("  ▶  PLAY  ")
                        .size(16.0)
                        .strong(),
                )
                .min_size(egui::vec2(120.0, 36.0)),
            );
            if btn.clicked() {
                match crate::game::launch(&self.config.game_binary) {
                    Ok(()) => self.launch_error = None,
                    Err(e) => self.launch_error = Some(e),
                }
            }
            if let Some(err) = &self.launch_error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
            }
        });
        ui.add_space(6.0);
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.ui_header(ui);
            ui.separator();
            self.ui_tabs(ui);
            ui.separator();
            ui.add_space(4.0);

            // Content area grows to fill available space above the play bar
            let play_bar_height = 60.0;
            let content_height = ui.available_height() - play_bar_height;
            egui::ScrollArea::vertical()
                .max_height(content_height)
                .show(ui, |ui| match self.active_tab {
                    Tab::News => self.ui_news(ui, ctx),
                    Tab::Settings => self.ui_settings(ui),
                });

            self.ui_play_bar(ui);
        });
    }
}
