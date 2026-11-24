use super::*;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages(ctx);
        self.poll_running();
        if !self.running.is_empty() || !self.busy.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        self.background(ctx);
        let mut actions = vec![];
        self.title_bar(ctx);
        egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20, 0))).show(ctx, |ui| match self.page {
            Page::Sessions => self.sessions_page(ui, &mut actions),
            Page::Preferences => self.preferences_page(ui),
        });
        self.corner_buttons(ctx);
        self.dialogs(ctx, &mut actions);
        for a in actions {
            self.act(ctx, a);
        }
        if let Some((t, at)) = self.toast.clone() {
            if at.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            } else {
                egui::Area::new("toast".into()).anchor(Align2::CENTER_BOTTOM, [0.0, -30.0]).show(ctx, |ui| {
                    egui::Frame::new().fill(Color32::from_rgb(0x1E, 0x23, 0x2E)).corner_radius(8).inner_margin(12).stroke(Stroke::new(1.0, BORDER)).show(ui, |ui| {
                        ui.label(RichText::new(t).color(TEXT));
                    });
                });
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
        if self.dirty && self.shot.is_none() {
            self.dirty = false;
            if let Err(e) = self.store.save() {
                self.toast(format!("Could not save: {e}"));
            }
        }
        self.screenshot(ctx);
    }
}

impl App {
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        self.frames += 1;
        // Dev aid: NEBULA_SHOT_EDIT=1 opens the new-session dialog so it can be screenshotted.
        if self.frames == 5 && std::env::var_os("NEBULA_SHOT_EDIT").is_some() {
            self.new_session(false);
        }
        ctx.request_repaint();
        if self.frames == 120 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let img = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = img {
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, bytes) {
                let _ = buf.save(&path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn background(&self, ctx: &egui::Context) {
        let r = ctx.screen_rect();
        let p = ctx.layer_painter(egui::LayerId::background());
        p.rect_filled(r, 0, Color32::from_rgb(0x03, 0x04, 0x07));
        for (fx, fy, rad, col) in [
            (0.12, 0.18, 0.22, Color32::from_rgba_premultiplied(1, 1, 2, 1)),
            (0.62, 0.32, 0.26, Color32::from_rgba_premultiplied(1, 1, 2, 1)),
            (0.85, 0.85, 0.3, Color32::from_rgba_premultiplied(1, 1, 2, 1)),
            (0.35, 0.75, 0.18, Color32::from_rgba_premultiplied(1, 1, 2, 1)),
        ] {
            let c = Pos2::new(r.left() + fx * r.width(), r.top() + fy * r.height());
            for i in 0..16 {
                p.circle_filled(c, rad * r.width() * (1.0 - i as f32 / 16.0), col);
            }
        }
        for &(x, y, s, a) in &self.stars {
            p.circle_filled(Pos2::new(r.left() + x * r.width(), r.top() + y * r.height()), s * 0.75, Color32::from_rgba_unmultiplied(220, 228, 255, a));
        }
        for (fx, fy) in [(0.115, 0.155), (0.92, 0.31), (0.14, 0.9)] {
            let c = Pos2::new(r.left() + fx * r.width(), r.top() + fy * r.height());
            p.circle_filled(c, 6.0, Color32::from_rgba_unmultiplied(120, 160, 255, 25));
            p.circle_filled(c, 2.0, Color32::WHITE);
        }
    }

    fn title_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("title").exact_height(34.0).frame(egui::Frame::new().fill(BAR)).show(ctx, |ui| {
            let full = ui.max_rect();
            let drag = ui.interact(full, ui.id().with("drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let m = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!m));
            }
            let p = ui.painter().clone();
            let y = full.center().y;
            // logo: a ringed planet (own design)
            let lc = Pos2::new(full.left() + 20.0, y);
            p.circle_filled(lc, 9.0, TEXT);
            p.circle_filled(lc + Vec2::new(2.5, -2.5), 4.2, BAR);
            p.line_segment([lc + Vec2::new(-11.0, 5.5), lc + Vec2::new(11.0, -5.5)], Stroke::new(2.0, BAR));
            let home = Rect::from_min_size(Pos2::new(full.left() + 36.0, full.top()), Vec2::new(140.0, 34.0));
            if ui.interact(home, ui.id().with("home"), Sense::click()).clicked() {
                self.page = Page::Sessions;
            }
            p.text(Pos2::new(full.left() + 44.0, y), Align2::LEFT_CENTER, "Nebula Sessions", bold(14.0), TEXT);
            let sep = |x: f32| p.line_segment([Pos2::new(full.left() + x, full.top() + 6.0), Pos2::new(full.left() + x, full.bottom() - 6.0)], Stroke::new(1.0, BORDER));
            sep(182.0);
            if icon_click(ui, Rect::from_center_size(Pos2::new(full.left() + 206.0, y), Vec2::splat(28.0)), "bell", 17.0, TEXT, "Notifications") {
                self.bell = !self.bell;
            }
            sep(230.0);
            let gear = if self.page == Page::Preferences { ICON } else { TEXT };
            if icon_click(ui, Rect::from_center_size(Pos2::new(full.left() + 252.0, y), Vec2::splat(28.0)), "⚙", 17.0, gear, "Preferences") {
                self.page = if self.page == Page::Preferences { Page::Sessions } else { Page::Preferences };
            }
            // desktop tab
            let name = self.store.desktop_name().to_string();
            let tw = text_w(ui, &name, bold(14.0));
            let tab = Rect::from_min_size(Pos2::new(full.left() + 276.0, full.top() + 2.0), Vec2::new(tw + 96.0, 32.0));
            p.rect_filled(tab, egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, Color32::from_rgb(0x17, 0x1A, 0x22));
            p.text(Pos2::new(tab.left() + 18.0, y + 1.0), Align2::CENTER_CENTER, "🖥", body(14.0), ICON);
            p.text(Pos2::new(tab.left() + 36.0, y + 1.0), Align2::LEFT_CENTER, &name, bold(14.0), TEXT);
            let mut dk = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(tab.right() - 34.0, tab.top() + 3.0), Vec2::new(30.0, 26.0))));
            flat(&mut dk);
            dk.menu_button(RichText::new("▾").color(ICON).size(13.0), |ui| {
                ui.set_min_width(200.0);
                let mut pick = None;
                for d in &self.store.desktops {
                    if ui.selectable_label(d.uuid == self.store.current_desktop, &d.name).clicked() {
                        pick = Some(d.uuid.clone());
                    }
                }
                if let Some(u) = pick {
                    self.store.current_desktop = u;
                    self.selected.clear();
                    self.dirty = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("New desktop…").clicked() {
                    self.new_desktop = Some(String::new());
                    ui.close();
                }
            });
            // window buttons, painted
            let mut x = full.right() - 23.0;
            for what in 0..3 {
                let br = Rect::from_center_size(Pos2::new(x, y), Vec2::new(44.0, 34.0));
                let resp = ui.interact(br, ui.id().with(("wb", what)), Sense::click());
                if resp.hovered() {
                    p.rect_filled(br, 0, if what == 0 { Color32::from_rgb(0xC4, 0x2B, 0x1C) } else { Color32::from_rgb(0x22, 0x26, 0x30) });
                }
                let c = br.center();
                let st = Stroke::new(1.2, TEXT);
                match what {
                    0 => {
                        p.line_segment([c + Vec2::new(-5.5, -5.5), c + Vec2::new(5.5, 5.5)], st);
                        p.line_segment([c + Vec2::new(-5.5, 5.5), c + Vec2::new(5.5, -5.5)], st);
                    }
                    1 => {
                        p.rect_stroke(Rect::from_center_size(c, Vec2::splat(10.0)), 0, st, StrokeKind::Inside);
                    }
                    _ => {
                        p.line_segment([c + Vec2::new(-5.5, 0.0), c + Vec2::new(5.5, 0.0)], st);
                    }
                }
                if resp.clicked() {
                    match what {
                        0 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        1 => {
                            let m = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!m));
                        }
                        _ => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                    }
                }
                x -= 44.0;
            }
        });
    }

    fn corner_buttons(&mut self, ctx: &egui::Context) {
        egui::Area::new("help".into()).anchor(Align2::RIGHT_BOTTOM, [-8.0, -76.0]).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(54.0, 50.0), Sense::click());
            ui.painter().rect_filled(r, egui::CornerRadius { nw: 14, sw: 14, ne: 0, se: 0 }, Color32::from_rgb(0x15, 0x18, 0x1F));
            ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(1.6, TEXT));
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "?", bold(13.0), TEXT);
            if resp.on_hover_text("How it works").clicked() {
                self.help = !self.help;
            }
        });
    }

    fn sessions_page(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        ui.add_space(18.0);
        if !self.store.banner_closed {
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 50.0), Sense::hover());
            ui.painter().rect_filled(r, 4, Color32::from_rgba_premultiplied(18, 20, 26, 235));
            let msg = match &self.browser {
                Some((_, m)) => format!(
                    "Sessions open in Nebula Chromium{} with their own data, proxy and real-device fingerprint",
                    m.map(|m| format!(" {m}")).unwrap_or_default()
                ),
                None => "Nebula Chromium not found: put the chromium folder built by build.ps1 beside this program".into(),
            };
            let end = ui.painter().text(Pos2::new(r.left() + 27.0, r.center().y), Align2::LEFT_CENTER, msg, bold(14.0), TEXT);
            let lr = Rect::from_min_size(Pos2::new(end.right() + 20.0, r.top() + 10.0), Vec2::new(140.0, 30.0));
            let lresp = ui.interact(lr, ui.id().with("banner-link"), Sense::click());
            ui.painter().circle_stroke(Pos2::new(lr.left() + 10.0, r.center().y), 9.5, Stroke::new(1.5, ICON));
            ui.painter().text(Pos2::new(lr.left() + 10.0, r.center().y), Align2::CENTER_CENTER, "⚙", body(11.0), ICON);
            ui.painter().text(Pos2::new(lr.left() + 28.0, r.center().y), Align2::LEFT_CENTER, "PREFERENCES", bold(14.0), if lresp.hovered() { Color32::WHITE } else { TEXT });
            if lresp.clicked() {
                self.page = Page::Preferences;
            }
            let cr = Rect::from_center_size(Pos2::new(r.right() - 22.0, r.top() + 16.0), Vec2::splat(22.0));
            let cresp = ui.interact(cr, ui.id().with("banner-x"), Sense::click());
            let cc = if cresp.hovered() { TEXT } else { DIM };
            let c = cr.center();
            ui.painter().line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], Stroke::new(1.3, cc));
            ui.painter().line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], Stroke::new(1.3, cc));
            if cresp.clicked() {
                self.store.banner_closed = true;
                self.dirty = true;
            }
        }
        ui.add_space(22.0);

        ui.horizontal(|ui| {
            ui.set_height(36.0);
            let (r, resp) = ui.allocate_exact_size(Vec2::new(128.0, 30.0), Sense::click());
            let c = Pos2::new(r.left() + 12.0, r.center().y);
            ui.painter().circle_stroke(c, 11.5, Stroke::new(1.5, ICON));
            ui.painter().line_segment([c - Vec2::new(5.0, 0.0), c + Vec2::new(5.0, 0.0)], Stroke::new(1.5, ICON));
            ui.painter().line_segment([c - Vec2::new(0.0, 5.0), c + Vec2::new(0.0, 5.0)], Stroke::new(1.5, ICON));
            ui.painter().text(Pos2::new(r.left() + 34.0, r.center().y), Align2::LEFT_CENTER, "NEW SESSION", bold(14.5), if resp.hovered() { Color32::WHITE } else { TEXT });
            if resp.clicked() {
                self.new_session(self.quick);
            }
            ui.add_space(14.0);
            toggle(ui, &mut self.quick).on_hover_text("Quick: create with a random real-device fingerprint without opening the editor");
            ui.label(RichText::new("Quick").font(bold(14.5)).color(TEXT));
            ui.add_space(14.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 30.0), Sense::hover());
            person(ui.painter(), r.center() + Vec2::new(0.0, -1.0), ICON);
            ui.label(RichText::new(self.store.preset_label(self.preset)).font(bold(14.5)).color(TEXT));
            flat(ui);
            ui.menu_button(RichText::new("▾").color(TEXT), |ui| {
                self.preset_list(ui);
            });
            ui.add_space(30.0);
            let (sr, _) = ui.allocate_exact_size(Vec2::new(410.0, 36.0), Sense::hover());
            ui.painter().rect_filled(sr, 18, Color32::from_rgb(0x1A, 0x1D, 0x24));
            let sc = Pos2::new(sr.left() + 25.0, sr.center().y - 1.0);
            ui.painter().circle_stroke(sc, 6.0, Stroke::new(1.6, TEXT));
            ui.painter().line_segment([sc + Vec2::new(4.3, 4.3), sc + Vec2::new(8.5, 8.5)], Stroke::new(1.8, TEXT));
            let inner = Rect::from_min_max(Pos2::new(sr.left() + 46.0, sr.top() + 8.0), Pos2::new(sr.right() - 16.0, sr.bottom() - 8.0));
            ui.put(inner, egui::TextEdit::singleline(&mut self.search).hint_text("Search everything").frame(false));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let trash_count = self.store.sessions.iter().filter(|s| s.trashed && s.desktop == self.store.current_desktop).count();
                if tool_button(ui, "🗑", "TRASH", if self.trash_view { ICON } else { DIM }, trash_count > 0 || self.trash_view).clicked() {
                    self.trash_view = !self.trash_view;
                    self.selected.clear();
                }
                ui.add_space(22.0);
                if tool_button(ui, "multi", "MULTIPLE", ICON, true).clicked() {
                    self.multiple = Some(MassDlg::new());
                }
                ui.add_space(22.0);
                if tool_button(ui, "proxy", "PROXY", ICON, true).clicked() && self.checker.is_none() {
                    self.checker = Some(Checker { text: self.store.proxy_list.clone(), kind: ProxyKind::Http, rows: vec![], gen: 0 });
                }
                ui.add_space(22.0);
                if tool_button(ui, "🏷", "TAGS", ICON, true).clicked() {
                    self.tags_dlg = Some(String::new());
                }
            });
        });
        ui.add_space(10.0);
        self.table(ui, actions);
    }

    fn table(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let ids = self.visible();
        let avail = ui.available_rect_before_wrap();
        let row_h = 66.0;
        let head_h = 90.0;
        let h = (head_h + row_h * ids.len().max(1) as f32 + 2.0).min(avail.height() - 10.0);
        let tr = Rect::from_min_size(avail.min, Vec2::new(avail.width(), h));
        ui.allocate_rect(tr, Sense::hover());
        let p = ui.painter().clone();
        p.rect_filled(tr, 12, PANEL);
        p.rect_stroke(tr, 12, Stroke::new(1.0, BORDER), StrokeKind::Inside);
        let (l, w) = (tr.left(), tr.width());
        // column anchors measured from the reference picture (table 1329 px wide)
        let cols = [l + 39.0, l + 110.0, l + 172.0, l + 195.0, l + w * 0.377, l + w * 0.667, l + w * 0.744, tr.right() - 75.0, tr.right() - 34.0];
        let [col_cb, _, col_icon, col_name, col_proxy, col_act, col_desc, _, col_more] = cols;

        let y1 = tr.top() + 22.0;
        let mut x = l + 22.0;
        for (s, label) in [(Sort::Name, "Name"), (Sort::Created, "Created"), (Sort::Used, "Used"), (Sort::Modified, "Modified"), (Sort::Timer, "Timer")] {
            let on = self.sort == s;
            let f = if on { bold(12.5) } else { body(12.5) };
            let tw = text_w(ui, label, f.clone());
            let r = Rect::from_min_size(Pos2::new(x - 4.0, y1 - 11.0), Vec2::new(tw + 8.0, 22.0));
            let resp = ui.interact(r, ui.id().with(("sort", label)), Sense::click());
            p.text(Pos2::new(x, y1), Align2::LEFT_CENTER, label, f, if on || resp.hovered() { TEXT } else { DIM });
            if resp.clicked() {
                if on {
                    self.desc = !self.desc;
                } else {
                    self.sort = s;
                }
            }
            x += tw + 28.0;
        }
        let pr = Rect::from_min_size(Pos2::new(tr.right() - 84.0, y1 - 10.0), Vec2::new(20.0, 20.0));
        if check_box(ui, pr, self.pinned_only) {
            self.pinned_only = !self.pinned_only;
        }
        p.text(Pos2::new(pr.right() + 4.0, y1), Align2::LEFT_CENTER, "Pinned", body(12.5), DIM);
        let rr = Rect::from_min_size(Pos2::new(tr.right() - 230.0, y1 - 10.0), Vec2::new(20.0, 20.0));
        if check_box(ui, rr, self.running_only) {
            self.running_only = !self.running_only;
        }
        p.text(Pos2::new(rr.right() + 4.0, y1), Align2::LEFT_CENTER, "Running only", body(12.5), DIM);
        p.line_segment([Pos2::new(l, tr.top() + 43.0), Pos2::new(tr.right(), tr.top() + 43.0)], Stroke::new(1.0, BORDER));

        let y2 = tr.top() + 67.0;
        let all_sel = !ids.is_empty() && ids.iter().all(|i| self.selected.contains(i));
        if check_box(ui, Rect::from_center_size(Pos2::new(col_cb, y2), Vec2::splat(24.0)), all_sel) {
            if all_sel {
                self.selected.clear();
            } else {
                self.selected.extend(ids.iter().cloned());
            }
        }
        let sort_label = match self.sort {
            Sort::Name => "Name",
            Sort::Created => "Created",
            Sort::Used => "Used",
            Sort::Modified => "Modified",
            Sort::Timer => "Timer",
        };
        if ui.interact(Rect::from_min_size(Pos2::new(col_icon - 10.0, y2 - 12.0), Vec2::new(120.0, 24.0)), ui.id().with("hsort"), Sense::click()).clicked() {
            self.desc = !self.desc;
        }
        p.text(Pos2::new(col_icon + 2.0, y2), Align2::CENTER_CENTER, if self.desc { "↓↑" } else { "↑↓" }, body(13.0), TEXT);
        let e = p.text(Pos2::new(col_name - 3.0, y2), Align2::LEFT_CENTER, sort_label, bold(14.0), TEXT);
        let cc = Pos2::new(e.right() + 13.0, y2);
        p.circle_stroke(cc, 5.5, Stroke::new(1.2, DIM));
        p.line_segment([cc + Vec2::new(0.0, -3.0), cc], Stroke::new(1.2, DIM));
        p.line_segment([cc, cc + Vec2::new(2.5, 1.5)], Stroke::new(1.2, DIM));

        let e = p.text(Pos2::new(col_proxy, y2), Align2::LEFT_CENTER, "Proxy", body(14.0), DIM);
        let mut m = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(e.right() + 2.0, y2 - 13.0), Vec2::new(100.0, 26.0))));
        flat(&mut m);
        let pf = self.proxy_filter.map(|k| k.label()).unwrap_or("All");
        m.menu_button(RichText::new(format!("{pf} ▾")).font(bold(14.0)).color(TEXT), |ui| {
            if ui.selectable_label(self.proxy_filter.is_none(), "All").clicked() {
                self.proxy_filter = None;
                ui.close();
            }
            for k in ProxyKind::ALL {
                if ui.selectable_label(self.proxy_filter == Some(k), k.label()).clicked() {
                    self.proxy_filter = Some(k);
                    ui.close();
                }
            }
        });
        let mut m = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(col_act - 10.0, y2 - 13.0), Vec2::new(120.0, 26.0))));
        let sel: Vec<String> = ids.iter().filter(|i| self.selected.contains(*i)).cloned().collect();
        flat(&mut m);
        m.menu_button(RichText::new("Actions ▾").size(14.0).color(DIM), |ui| {
            ui.set_min_width(200.0);
            if sel.is_empty() {
                ui.label(RichText::new("Tick sessions first").color(DIM));
                return;
            }
            ui.label(RichText::new(format!("{} selected", sel.len())).color(DIM));
            let each: [(&str, fn(String) -> Action); 4] = [("Start", Action::Start), ("Stop", Action::Stop), ("Check proxy", Action::Check), ("Move to trash", Action::Trash)];
            for (label, f) in each {
                if ui.button(label).clicked() {
                    actions.extend(sel.iter().cloned().map(f));
                    ui.close();
                }
            }
            if ui.button("Set proxy").clicked() {
                actions.push(Action::SetProxy(sel.clone()));
                ui.close();
            }
        });
        p.text(Pos2::new(col_desc, y2), Align2::LEFT_CENTER, "Description", body(14.0), DIM);
        for dx in [-4.0, 0.0, 4.0] {
            p.line_segment([Pos2::new(col_more + dx, y2 - 7.0), Pos2::new(col_more + dx, y2 + 7.0)], Stroke::new(1.2, DIM));
        }
        p.line_segment([Pos2::new(l, tr.top() + head_h), Pos2::new(tr.right(), tr.top() + head_h)], Stroke::new(1.0, BORDER));

        let body_rect = Rect::from_min_max(Pos2::new(l, tr.top() + head_h), tr.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
        child.set_clip_rect(body_rect);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
            if ids.is_empty() {
                let (r, _) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::hover());
                let t = if self.trash_view { "The trash is empty" } else { "No sessions here. Press NEW SESSION to create one." };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, t, body(14.0), DIM);
            }
            for (i, id) in ids.iter().enumerate() {
                let (r, _) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::hover());
                let Some(s) = self.store.sessions.iter().find(|s| &s.uuid == id).cloned() else { continue };
                self.row(ui, r, &s, actions, cols);
                if i + 1 < ids.len() {
                    ui.painter().line_segment([Pos2::new(r.left(), r.bottom()), Pos2::new(r.right(), r.bottom())], Stroke::new(1.0, BORDER));
                }
            }
        });
    }

    fn row(&mut self, ui: &mut Ui, r: Rect, s: &Session, actions: &mut Vec<Action>, c: [f32; 9]) {
        let [col_cb, col_play, col_icon, col_name, col_proxy, col_act, col_desc, col_pin, col_more] = c;
        let y = r.center().y;
        let p = ui.painter().clone();
        let id = s.uuid.clone();
        if check_box(ui, Rect::from_center_size(Pos2::new(col_cb, y), Vec2::splat(24.0)), self.selected.contains(&id)) && !self.selected.remove(&id) {
            self.selected.insert(id.clone());
        }
        let running = self.running.get(&id).map(|r| r.started.elapsed().as_secs());
        let busy = self.busy.get(&id).cloned();
        let pc = Pos2::new(col_play, y);
        if self.trash_view {
            if icon_click(ui, Rect::from_center_size(pc, Vec2::splat(36.0)), "↺", 20.0, GREEN, "Restore") {
                actions.push(Action::Restore(id.clone()));
            }
        } else {
            let resp = ui.interact(Rect::from_center_size(pc, Vec2::splat(46.0)), ui.id().with(("play", &id)), Sense::click());
            p.circle_filled(pc, 23.0, Color32::from_rgb(0x10, 0x13, 0x18));
            p.circle_stroke(pc, 23.0, Stroke::new(1.5, if resp.hovered() { Color32::from_rgb(0x4A, 0x52, 0x60) } else { Color32::from_rgb(0x2C, 0x31, 0x3B) }));
            if running.is_some() {
                p.rect_filled(Rect::from_center_size(pc, Vec2::splat(13.0)), 2, RED);
            } else {
                let t = vec![pc + Vec2::new(-5.0, -9.0), pc + Vec2::new(9.0, 0.0), pc + Vec2::new(-5.0, 9.0), pc + Vec2::new(-5.0, -9.0)];
                p.add(egui::Shape::line(t, Stroke::new(1.8, GREEN)));
            }
            if resp.on_hover_text(if running.is_some() { "Stop" } else { "Start" }).clicked() {
                actions.push(if running.is_some() { Action::Stop(id.clone()) } else { Action::Start(id.clone()) });
            }
        }
        let d = devices::get(&s.fp.device);
        let icol = match d.os {
            devices::Os::Mac => Color32::from_rgb(0xC0, 0x6B, 0xE0),
            devices::Os::Windows => Color32::from_rgb(0xB3, 0x6B, 0xD6),
            devices::Os::Linux => Color32::from_rgb(0xD6, 0x9B, 0x6B),
        };
        person(&p, Pos2::new(col_icon, y - 2.0), icol);
        let name_r = p.text(Pos2::new(col_name, y - 9.0), Align2::LEFT_CENTER, &s.name, bold(14.5), TEXT);
        let nr = ui.interact(name_r.expand(4.0), ui.id().with(("name", &id)), Sense::click());
        if nr.on_hover_text(format!("{}\nDouble-click to edit", d.label)).double_clicked() {
            actions.push(Action::Edit(id.clone()));
        }
        let sub = match (&running, &busy) {
            (_, Some(b)) => format!("{b}…"),
            (Some(secs), _) => format!("running {}", duration(*secs)),
            _ => {
                let t = match self.sort {
                    Sort::Used => s.used,
                    Sort::Modified => s.modified,
                    _ => s.created,
                };
                let mut t = if self.sort == Sort::Timer { duration(s.run_secs) } else { ago(t) };
                if s.warmed {
                    t += "  ·  warmed";
                }
                t
            }
        };
        p.text(Pos2::new(col_name, y + 12.0), Align2::LEFT_CENTER, sub, body(12.5), if running.is_some() || busy.is_some() { GREEN } else { DIM });

        let (bg, fg) = match s.proxy.kind {
            ProxyKind::Direct => (Color32::from_rgb(0x5A, 0x1E, 0x14), Color32::from_rgb(0xFF, 0x8A, 0x65)),
            ProxyKind::Http => (Color32::from_rgb(0x14, 0x2E, 0x5A), Color32::from_rgb(0x7F, 0xB0, 0xFF)),
            ProxyKind::Socks5 => (Color32::from_rgb(0x12, 0x45, 0x3C), Color32::from_rgb(0x6F, 0xE0, 0xC0)),
            ProxyKind::Tor => (Color32::from_rgb(0x3A, 0x1E, 0x55), Color32::from_rgb(0xC8, 0x9B, 0xFF)),
            ProxyKind::Localhost | ProxyKind::Ssh => (Color32::from_rgb(0x4A, 0x3A, 0x10), Color32::from_rgb(0xFF, 0xD0, 0x6B)),
        };
        let bw = text_w(ui, s.proxy.kind.label(), bold(10.5)) + 10.0;
        let br = Rect::from_min_size(Pos2::new(col_proxy, y - 17.0), Vec2::new(bw, 15.0));
        p.rect_filled(br, 2, bg);
        p.text(br.center(), Align2::CENTER_CENTER, s.proxy.kind.label(), bold(10.5), fg);
        let dot = Pos2::new(br.right() + 12.0, br.center().y);
        let (dc, ip, place) = match &s.geo {
            Some(g) if g.ok => (if g.fraud_score.is_some_and(|f| f >= 75) { RED } else { GREEN }, g.ip.clone(), g.place()),
            Some(g) => (RED, s.proxy.short(), g.error.clone()),
            None => (DIM, if s.proxy.kind == ProxyKind::Direct { "not checked".into() } else { s.proxy.short() }, "check the proxy to see its location".into()),
        };
        p.circle_stroke(dot, 5.0, Stroke::new(1.5, dc));
        p.text(Pos2::new(dot.x + 10.0, dot.y), Align2::LEFT_CENTER, ip, bold(14.0), TEXT);
        if let Some(cc) = s.geo.as_ref().filter(|g| g.ok).map(|g| g.country_code.clone()) {
            // two-tone country chip (not a real flag)
            let fr = Rect::from_min_size(Pos2::new(col_proxy, y + 6.0), Vec2::new(14.0, 10.0));
            let h = devices::model_seed(&cc);
            p.rect_filled(Rect::from_min_max(fr.min, Pos2::new(fr.right(), fr.center().y)), 0, Color32::from_rgb((h >> 16) as u8 | 0x40, (h >> 8) as u8 | 0x40, h as u8 | 0x40));
            p.rect_filled(Rect::from_min_max(Pos2::new(fr.left(), fr.center().y), fr.max), 0, Color32::from_rgb((h >> 3) as u8 | 0x30, (h >> 11) as u8 | 0x30, (h >> 19) as u8 | 0x30));
        }
        p.text(Pos2::new(col_proxy + 22.0, y + 11.0), Align2::LEFT_CENTER, place, body(12.0), DIM);

        if !self.trash_view {
            if icon_click(ui, Rect::from_center_size(Pos2::new(col_act + 10.0, y), Vec2::splat(26.0)), "🔁", 16.0, ICON, "Check proxy") {
                actions.push(Action::Check(id.clone()));
            }
            if icon_click(ui, Rect::from_center_size(Pos2::new(col_act + 38.0, y), Vec2::splat(26.0)), "proxy", 18.0, ICON, "Set proxy") {
                actions.push(Action::SetProxy(vec![id.clone()]));
            }
            let rot = !s.proxy.change_ip_url.trim().is_empty();
            if icon_click(ui, Rect::from_center_size(Pos2::new(col_act + 66.0, y), Vec2::splat(26.0)), "🔄", 16.0, if rot { ICON } else { FAINT }, "Change IP") {
                actions.push(Action::ChangeIp(id.clone()));
            }
        }
        if s.description.is_empty() {
            if icon_click(ui, Rect::from_center_size(Pos2::new(col_desc + 20.0, y), Vec2::splat(26.0)), "✏", 15.0, DIM, "Add a description") {
                actions.push(Action::Desc(id.clone()));
            }
        } else {
            let dr = Rect::from_min_size(Pos2::new(col_desc, y - 14.0), Vec2::new((col_pin - col_desc - 30.0).max(60.0), 28.0));
            let first: String = s.description.lines().next().unwrap_or("").chars().take(48).collect();
            let resp = ui.interact(dr, ui.id().with(("desc", &id)), Sense::click());
            p.text(Pos2::new(dr.left(), y - 2.0), Align2::LEFT_CENTER, first, body(13.0), if resp.hovered() { TEXT } else { DIM });
            if resp.on_hover_text(&s.description).clicked() {
                actions.push(Action::Desc(id.clone()));
            }
        }
        if !s.tags.is_empty() {
            p.text(Pos2::new(col_desc, y + 18.0), Align2::LEFT_CENTER, format!("# {}", s.tags.join("  # ")), body(11.0), ICON);
        }
        if icon_click(ui, Rect::from_center_size(Pos2::new(col_pin, y), Vec2::splat(26.0)), "📌", 16.0, if s.pinned { ICON } else { Color32::from_rgb(0x3A, 0x55, 0x90) }, if s.pinned { "Unpin" } else { "Pin to top" }) {
            actions.push(Action::Pin(id.clone()));
        }
        let mr = Rect::from_center_size(Pos2::new(col_more, y), Vec2::new(22.0, 30.0));
        let mut mu = ui.new_child(egui::UiBuilder::new().max_rect(mr));
        flat(&mut mu);
        let resp = mu.menu_button(RichText::new("  ").size(14.0), |ui| self.row_menu(ui, s, actions));
        dots(ui, mr.center(), if resp.response.hovered() { Color32::WHITE } else { ICON });
    }

    fn row_menu(&self, ui: &mut Ui, s: &Session, actions: &mut Vec<Action>) {
        ui.set_min_width(240.0);
        let id = s.uuid.clone();
        let item = |ui: &mut Ui, actions: &mut Vec<Action>, label: &str, icon: &str, enabled: bool, a: Action| {
            let b = egui::Button::new(RichText::new(label).font(bold(14.0))).shortcut_text(RichText::new(icon).color(ICON)).min_size(Vec2::new(230.0, 28.0)).frame(false);
            if ui.add_enabled(enabled, b).clicked() {
                actions.push(a);
                ui.close();
            }
        };
        if self.trash_view {
            item(ui, actions, "Restore", "↺", true, Action::Restore(id.clone()));
            item(ui, actions, "Delete forever", "✖", true, Action::Purge(id));
            return;
        }
        let stopped = !self.running.contains_key(&id) && !self.busy.contains_key(&id);
        let has_data = profile_dir(&id).join("data").exists();
        item(ui, actions, "Edit session", "✏", true, Action::Edit(id.clone()));
        item(ui, actions, "Action log", "📋", !s.log.is_empty(), Action::Log(id.clone()));
        ui.separator();
        item(ui, actions, "Import cookies", "⬇", true, Action::ImportCookies(id.clone()));
        item(ui, actions, "Export cookies", "⬆", has_data, Action::ExportCookies(id.clone()));
        item(ui, actions, "Remove all data", "🗑", has_data && stopped, Action::RemoveData(id.clone()));
        ui.separator();
        item(ui, actions, "Check proxy", "🔁", true, Action::Check(id.clone()));
        item(ui, actions, "Set proxy", "⑂", true, Action::SetProxy(vec![id.clone()]));
        item(ui, actions, "Change IP", "🔄", !s.proxy.change_ip_url.trim().is_empty(), Action::ChangeIp(id.clone()));
        ui.separator();
        item(ui, actions, "Warm up", "🔥", true, Action::Warm(id.clone()));
        let others: Vec<Desktop> = self.store.desktops.iter().filter(|d| d.uuid != s.desktop).cloned().collect();
        ui.add_enabled_ui(!others.is_empty(), |ui| {
            ui.menu_button(RichText::new("Move to desktop").font(bold(14.0)), |ui| {
                for d in &others {
                    if ui.button(&d.name).clicked() {
                        actions.push(Action::MoveTo(id.clone(), d.uuid.clone()));
                        ui.close();
                    }
                }
            });
        });
        item(ui, actions, "Clone", "⧉", true, Action::Clone(id.clone()));
        item(ui, actions, "Move to trash", "🗑", true, Action::Trash(id));
    }
}
