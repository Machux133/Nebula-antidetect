use super::*;
use crate::browser::Warmup;
use crate::devices::DEVICES;
use std::collections::VecDeque;
use std::sync::Mutex;

/// Search pages offered as the warm-up referrer: (name, address).
const REFERRERS: [(&str, &str); 5] = [
    ("None", ""),
    ("Google", "https://www.google.com/"),
    ("Bing", "https://www.bing.com/"),
    ("DuckDuckGo", "https://duckduckgo.com/"),
    ("Yahoo", "https://search.yahoo.com/"),
];

const SECTION_BG: Color32 = Color32::from_rgb(8, 9, 12);

/// A content box with a tab-shaped heading sitting on its top edge, like the original's preference groups.
fn section(ui: &mut Ui, icon: &str, title: &str, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width().min(900.0);
    ui.add_space(26.0);
    let tab_w = 66.0 + text_w(ui, title, bold(16.0));
    let (tab, _) = ui.allocate_exact_size(Vec2::new(tab_w, 36.0), Sense::hover());
    ui.add_space(-ui.spacing().item_spacing.y);
    let r = egui::Frame::new()
        .fill(Color32::from_rgba_premultiplied(8, 9, 12, 225))
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(10)
        .inner_margin(egui::Margin { left: 20, right: 20, top: 8, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(w - 42.0);
            add(ui)
        });
    let p = ui.painter();
    let cr = egui::CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 };
    let tab = Rect::from_min_max(tab.min, Pos2::new(tab.max.x, r.response.rect.top() + 1.0));
    p.rect_filled(tab, cr, SECTION_BG);
    p.rect_stroke(tab, cr, Stroke::new(1.0, BORDER), StrokeKind::Inside);
    p.line_segment([Pos2::new(tab.left() + 1.0, tab.bottom()), Pos2::new(tab.right() - 1.0, tab.bottom())], Stroke::new(2.0, SECTION_BG));
    p.text(Pos2::new(tab.left() + 24.0, tab.center().y), Align2::CENTER_CENTER, icon, body(18.0), ICON);
    p.text(Pos2::new(tab.left() + 44.0, tab.center().y), Align2::LEFT_CENTER, title, bold(16.0), TEXT);
}

fn pref_row(ui: &mut Ui, label: &str, last: bool, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(50.0);
        ui.label(RichText::new(label).font(bold(14.5)).color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    });
    if !last {
        ui.separator();
    }
}

/// Label, switch and its state text, like the original's fingerprint rows.
fn bool_toggle(ui: &mut Ui, label: &str, on: &mut bool, on_text: &str, off_text: &str) {
    ui.label(RichText::new(label).font(bold(14.0)).color(TEXT));
    toggle(ui, on);
    ui.label(RichText::new(if *on { on_text } else { off_text }).color(DIM));
}

/// Off = this computer's own value ("Direct"); on = noise, with a choice between a model-stable and a per-session seed.
fn mode_toggle(ui: &mut Ui, label: &str, m: &mut FpMode) {
    ui.label(RichText::new(label).font(bold(14.0)).color(TEXT));
    let mut on = *m != FpMode::Off;
    if toggle(ui, &mut on).changed() {
        *m = if on { FpMode::Noise } else { FpMode::Off };
    }
    if on {
        egui::ComboBox::from_id_salt(label).width(100.0).selected_text(m.label()).show_ui(ui, |ui| {
            ui.selectable_value(m, FpMode::Real, FpMode::Real.label());
            ui.selectable_value(m, FpMode::Noise, FpMode::Noise.label());
        });
    } else {
        ui.label(RichText::new("Direct").color(DIM));
    }
}

/// "ANGLE (Intel, Intel(R) HD Graphics 630 Direct3D11 ...)" -> the card name.
fn short_gpu(r: &str) -> String {
    let inner = r.strip_prefix("ANGLE (").and_then(|x| x.strip_suffix(')')).unwrap_or(r);
    let name = inner.split(", ").nth(1).unwrap_or(inner);
    // Drop the adapter id and the shader-model tail: "NVIDIA GeForce RTX 3060 (0x...) Direct3D11 vs_5_0 ps_5_0".
    let end = [" (0x", " Direct3D", " OpenGL", " Metal"].iter().filter_map(|m| name.find(m)).min().unwrap_or(name.len());
    name[..end].to_string()
}

impl App {
    pub(super) fn preferences_page(&mut self, ui: &mut Ui) {
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            if ui.add(egui::Label::new(RichText::new("Session manager").color(DIM).size(14.0)).sense(Sense::click())).clicked() {
                self.page = Page::Sessions;
            }
            ui.label(RichText::new(">").color(DIM));
            ui.label(RichText::new("Preferences").color(ICON).size(14.0));
        });
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let left = ((ui.available_width() - 900.0) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(left);
                ui.vertical(|ui| {
                    ui.set_width(900.0_f32.min(ui.available_width()));
                    let st = &mut self.store.settings;
                    let ports_open = &mut self.ports_open;
                    section(ui, "📶", "NETWORK", |ui| {
                        let resp = ui.horizontal(|ui| {
                            ui.set_min_height(50.0);
                            ui.label(RichText::new("Closed websockets ports").font(bold(14.5)).color(TEXT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(if *ports_open { "⌄" } else { "›" }).size(18.0).color(DIM));
                                ui.label(RichText::new(format!("{} ports", st.closed_ports.len())).color(DIM));
                            });
                        });
                        if ui.interact(resp.response.rect, ui.id().with("ports"), Sense::click()).clicked() {
                            *ports_open = !*ports_open;
                        }
                        if *ports_open {
                            ui.label(RichText::new("Pages that probe these local ports (remote-desktop tools) find them closed.").color(DIM));
                            let mut t = st.closed_ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ");
                            if ui.add(egui::TextEdit::multiline(&mut t).desired_rows(2).desired_width(f32::INFINITY)).changed() {
                                st.closed_ports = t.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect();
                                changed = true;
                            }
                            if ui.button("Restore defaults").clicked() {
                                st.closed_ports = DEFAULT_CLOSED_PORTS.to_vec();
                                changed = true;
                            }
                        }
                        ui.separator();
                        pref_row(ui, "Proxy check timeout", true, |ui| {
                            egui::Frame::new().fill(Color32::from_rgb(0x1C, 0x1F, 0x26)).corner_radius(3).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                                ui.set_width(168.0);
                                ui.label(RichText::new(format!("{} seconds", proxy::CHECK_TIMEOUT_SECS)).font(bold(14.0)));
                            });
                        });
                    });
                    section(ui, "⇄", "INTEGRATIONS", |ui| {
                        pref_row(ui, "Enable IP Stat", false, |ui| {
                            changed |= toggle(ui, &mut st.enable_ip_stats).changed();
                        });
                        pref_row(ui, "IPQualityScore API key", false, |ui| {
                            egui::Frame::new().fill(Color32::from_rgb(0x1C, 0x1F, 0x26)).corner_radius(3).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                                changed |= ui.add(egui::TextEdit::singleline(&mut st.ipqs_key).hint_text("Your private key").password(true).frame(false).desired_width(168.0)).changed();
                            });
                        });
                    });
                    section(ui, "⚙", "LOCAL API", |ui| {
                        pref_row(ui, "API port (0 = off, restart to apply)", true, |ui| {
                            changed |= ui.add(egui::DragValue::new(&mut st.api_port).range(0..=65535)).changed();
                        });
                        ui.label(RichText::new("HTTP on 127.0.0.1 only: POST /sessions/create_quick, POST /sessions/start {uuid, headless, debug_port}, POST /sessions/stop {uuid}, GET /sessions. Requests carrying an Origin header (web pages) are refused.").color(DIM).size(12.5));
                    });
                    section(ui, "🌐", "BROWSER", |ui| {
                        let found = browser::find_browser();
                        pref_row(ui, "Nebula Chromium", false, |ui| {
                            let t = match &found {
                                Some(p) => match browser::browser_major(p) {
                                    Some(m) => format!("{m} · {}", p.display()),
                                    None => p.display().to_string(),
                                },
                                None => "not found beside the manager".into(),
                            };
                            ui.label(RichText::new(t).color(if found.is_some() { DIM } else { RED }));
                        });
                        let wv = crate::widevine::find_source();
                        pref_row(ui, "Widevine (DRM video)", true, |ui| {
                            let t = match &wv {
                                Some(c) => format!("{} from {}", c.version, c.from),
                                None => "not found: install Microsoft Edge or Google Chrome".into(),
                            };
                            ui.label(RichText::new(t).color(if wv.is_some() { DIM } else { RED }));
                            changed |= toggle(ui, &mut st.widevine).changed();
                        });
                        ui.label(
                            RichText::new("Widevine is Google's closed DRM module for streaming sites. Nebula does not ship it: it is copied once from your own Edge or Chrome into this program's data folder and linked into each session.")
                                .color(DIM)
                                .size(12.5),
                        );
                        ui.label(
                            RichText::new("Sessions run only in the bundled Nebula Chromium (the chromium folder beside this program, made by build.ps1). It reports cores, memory, graphics card, client hints, fonts, voices and more natively, in workers too. Other browsers are not supported.")
                                .color(DIM)
                                .size(12.5),
                        );
                        ui.add_space(6.0);
                    });
                    section(ui, "🔥", "COOKIES & WARM UP", |ui| {
                        pref_row(ui, "Press \"accept all\" on cookie banners during warm-up", false, |ui| {
                            changed |= toggle(ui, &mut st.warmup_accept_cookies).changed();
                        });
                        ui.label(RichText::new("Default warm-up sites (one per line, at most 50 are used)").font(bold(14.5)).color(TEXT));
                        changed |= ui.add(egui::TextEdit::multiline(&mut st.warmup_urls).desired_rows(5).desired_width(f32::INFINITY)).changed();
                        ui.add_space(6.0);
                    });
                    section(ui, "↺", "RESET SETTINGS", |ui| {
                        pref_row(ui, "Put every preference back to its default", true, |ui| {
                            if ui.button("Reset").clicked() {
                                *st = Settings::default();
                                changed = true;
                            }
                        });
                    });
                    ui.add_space(40.0);
                });
            });
        });
        if changed {
            self.browser = detect();
            self.dirty = true;
        }
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        self.edit_dialog(ctx, actions);
        self.proxy_dialog(ctx);
        self.warm_dialog(ctx);
        self.checker_window(ctx);
        let center = |t: String| egui::Window::new(t).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).resizable(false);

        if let Some(id) = self.log_dlg.clone() {
            let mut open = true;
            let s = self.store.sessions.iter().find(|s| s.uuid == id);
            let title = s.map(|s| format!("Action log — {}", s.name)).unwrap_or_default();
            let entries: Vec<(u64, String)> = s.map(|s| s.log.clone()).unwrap_or_default();
            center(title).open(&mut open).default_width(520.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    for (t, m) in entries.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(ago(*t)).color(DIM).size(12.0));
                            ui.label(m);
                        });
                    }
                });
            });
            if !open {
                self.log_dlg = None;
            }
        }

        if let Some((id, mut text)) = self.desc_dlg.take() {
            let mut keep = true;
            center("Description".into()).show(ctx, |ui| {
                ui.add(egui::TextEdit::multiline(&mut text).desired_rows(5).desired_width(420.0).char_limit(65536));
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if let Some(s) = self.store.find(&id) {
                            s.description = text.clone();
                            s.modified = now();
                        }
                        self.dirty = true;
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if keep {
                self.desc_dlg = Some((id, text));
            }
        }

        if let Some(mut m) = self.multiple.take() {
            let mut keep = true;
            let sel: Vec<String> = self.selected.iter().cloned().collect();
            let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
            let pointer = ctx.input(|i| i.pointer.latest_pos());
            let label = self.store.preset_label(self.preset);
            center("Mass creation".into()).default_width(760.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("mpreset").selected_text(label.clone()).show_ui(ui, |ui| self.preset_list(ui));
                    // 1..=100 per the original's mass creation; 250 from its quick-create schema
                    if ui.button("−").clicked() {
                        m.n = m.n.saturating_sub(1);
                    }
                    ui.add(egui::DragValue::new(&mut m.n).range(0..=250));
                    if ui.button("+").clicked() {
                        m.n = (m.n + 1).min(250);
                    }
                });
                ui.add_space(6.0);
                let w = (ui.available_width() - 12.0) / 2.0;
                let mut zones = vec![];
                ui.horizontal(|ui| {
                    for left in [true, false] {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 170.0), Sense::hover());
                        ui.painter().rect_stroke(rect, 6, Stroke::new(1.0, BORDER), StrokeKind::Inside);
                        zones.push(rect);
                        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)).layout(egui::Layout::top_down(egui::Align::Center)));
                        inner.add_space(18.0);
                        let txt = if left { "Drag and drop TXT or JSON cookie files here, or browse. One file = one session." } else { "Drag and drop an XLSX or CSV file for universal import, or browse." };
                        inner.label(RichText::new(txt).color(DIM));
                        if inner.button("Browse file").clicked() {
                            if left {
                                if let Some(f) = rfd::FileDialog::new().add_filter("Cookies", &["json", "txt"]).pick_files() {
                                    m.cookies = f;
                                }
                            } else if let Some(f) = rfd::FileDialog::new().add_filter("Table", &["xlsx", "csv"]).pick_file() {
                                m.table = Some(f);
                            }
                        }
                        if !left && inner.link("Download universal import template").clicked() {
                            if let Some(p) = rfd::FileDialog::new().set_file_name("template.csv").save_file() {
                                self.toast(match std::fs::write(&p, crate::import::TEMPLATE_CSV) {
                                    Ok(_) => format!("Template saved to {}", p.display()),
                                    Err(e) => e.to_string(),
                                });
                            }
                        }
                        let picked = if left {
                            (!m.cookies.is_empty()).then(|| format!("{} file(s) selected", m.cookies.len()))
                        } else {
                            m.table.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
                        };
                        if let Some(p) = picked {
                            inner.label(RichText::new(p).color(ICON));
                        }
                    }
                });
                for f in dropped {
                    let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                    let right = ext == "xlsx" || ext == "csv" || pointer.is_some_and(|p| zones.get(1).is_some_and(|z| z.contains(p)));
                    if right {
                        m.table = Some(f);
                    } else {
                        m.cookies.push(f);
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    toggle(ui, &mut m.use_file_name);
                    ui.label("Use cookie file name as session name");
                });
                ui.label(RichText::new("A table wins over cookie files; cookie files win over the plain count. Cookies are set on the first start of each session.").color(DIM));
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                    let can = m.n > 0 || !m.cookies.is_empty() || m.table.is_some();
                    if ui.add_enabled(can, egui::Button::new("Create")).clicked() {
                        let desk = self.store.current_desktop.clone();
                        let label = self.store.preset_label(self.preset);
                        let mut next = self.store.sessions.iter().filter(|s| s.name.starts_with(&label)).count();
                        let mut made: Vec<Session> = vec![];
                        let mut notes: Vec<String> = vec![];
                        if let Some(t) = &m.table {
                            match crate::import::read_table(t).and_then(|r| crate::import::sessions_from_table(&r, self.preset, &desk)) {
                                Ok((s, w)) => {
                                    made = s;
                                    notes = w;
                                }
                                Err(e) => notes.push(e),
                            }
                        } else if !m.cookies.is_empty() {
                            for f in &m.cookies {
                                match std::fs::read_to_string(f).map_err(|e| e.to_string()).and_then(|t| browser::parse_cookies(&t)) {
                                    Ok(c) => {
                                        next += 1;
                                        let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                                        let name = if m.use_file_name && !stem.is_empty() { stem } else { format!("{label} {next}") };
                                        let mut s = self.store.session_from_preset(name.chars().take(50).collect(), &desk, self.preset);
                                        crate::import::save_pending(&s.uuid, &c);
                                        s.log(format!("Created from {}", f.display()));
                                        made.push(s);
                                    }
                                    Err(e) => notes.push(format!("{}: {e}", f.display())),
                                }
                            }
                        } else {
                            for _ in 0..m.n {
                                next += 1;
                                let mut s = self.store.session_from_preset(format!("{label} {next}"), &desk, self.preset);
                                s.log("Session created");
                                made.push(s);
                            }
                        }
                        let n = made.len();
                        self.store.sessions.extend(made);
                        self.dirty = true;
                        self.toast(match notes.first() {
                            Some(w) => format!("Created {n} sessions, {} problem(s): {w}", notes.len()),
                            None => format!("Created {n} sessions"),
                        });
                        keep = false;
                    }
                });
                ui.separator();
                ui.label(RichText::new(format!("Selected sessions: {}", sel.len())).font(bold(15.0)));
                ui.add_enabled_ui(!sel.is_empty(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Start all").clicked() {
                            actions.extend(sel.iter().cloned().map(Action::Start));
                        }
                        if ui.button("Stop all").clicked() {
                            actions.extend(sel.iter().cloned().map(Action::Stop));
                        }
                        if ui.button("Check proxies").clicked() {
                            actions.extend(sel.iter().cloned().map(Action::Check));
                        }
                        if ui.button("Set proxy").clicked() {
                            actions.push(Action::SetProxy(sel.clone()));
                            keep = false;
                        }
                        if ui.button("Move to trash").clicked() {
                            actions.extend(sel.iter().cloned().map(Action::Trash));
                        }
                    });
                });
            });
            if keep {
                self.multiple = Some(m);
            }
        }

        if let Some(mut t) = self.tags_dlg.take() {
            let mut keep = true;
            let mut counts: Vec<(String, usize)> = vec![];
            for s in self.store.sessions.iter().filter(|s| !s.trashed) {
                for tag in &s.tags {
                    match counts.iter_mut().find(|(n, _)| n == tag) {
                        Some(c) => c.1 += 1,
                        None => counts.push((tag.clone(), 1)),
                    }
                }
            }
            counts.sort();
            center("Tags".into()).show(ctx, |ui| {
                ui.label(RichText::new("Show only sessions with a tag").color(DIM));
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(self.tag_filter.is_none(), "All").clicked() {
                        self.tag_filter = None;
                    }
                    for (n, c) in &counts {
                        if ui.selectable_label(self.tag_filter.as_ref() == Some(n), format!("# {n} ({c})")).clicked() {
                            self.tag_filter = Some(n.clone());
                        }
                    }
                });
                ui.separator();
                ui.label(RichText::new(format!("Tag the {} selected sessions", self.selected.len())).color(DIM));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut t).hint_text("tag name").char_limit(20).desired_width(180.0));
                    let tag = t.trim().to_string();
                    let ok = !tag.is_empty() && !self.selected.is_empty();
                    if ui.add_enabled(ok, egui::Button::new("Add")).clicked() {
                        for s in self.store.sessions.iter_mut().filter(|s| self.selected.contains(&s.uuid)) {
                            if !s.tags.contains(&tag) {
                                s.tags.push(tag.clone());
                            }
                        }
                        self.dirty = true;
                    }
                    if ui.add_enabled(ok, egui::Button::new("Remove")).clicked() {
                        for s in self.store.sessions.iter_mut().filter(|s| self.selected.contains(&s.uuid)) {
                            s.tags.retain(|x| x != &tag);
                        }
                        self.dirty = true;
                    }
                });
                if ui.button("Close").clicked() {
                    keep = false;
                }
            });
            if keep {
                self.tags_dlg = Some(t);
            }
        }

        if let Some(mut n) = self.new_desktop.take() {
            let mut keep = true;
            center("New desktop".into()).show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut n).hint_text("Name").char_limit(50));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!n.trim().is_empty(), egui::Button::new("Create")).clicked() {
                        let d = Desktop { uuid: uuid::Uuid::new_v4().to_string(), name: n.trim().into() };
                        self.store.current_desktop = d.uuid.clone();
                        self.store.desktops.push(d);
                        self.dirty = true;
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if keep {
                self.new_desktop = Some(n);
            }
        }

        if self.bell {
            let mut open = true;
            let items = self.store.notifications.clone();
            egui::Window::new("Notifications").open(&mut open).anchor(Align2::LEFT_TOP, [180.0, 44.0]).default_width(420.0).collapsible(false).show(ctx, |ui| {
                if items.is_empty() {
                    ui.label(RichText::new("Nothing yet").color(DIM));
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (t, m) in items.iter().rev() {
                        ui.label(RichText::new(ago(*t)).color(DIM).size(11.5));
                        ui.label(m);
                        ui.add_space(4.0);
                    }
                });
                if !items.is_empty() && ui.button("Clear").clicked() {
                    self.store.notifications.clear();
                    self.dirty = true;
                }
            });
            if !open {
                self.bell = false;
            }
        }

        if self.help {
            let mut open = true;
            egui::Window::new("How it works").open(&mut open).anchor(Align2::RIGHT_BOTTOM, [-80.0, -90.0]).default_width(440.0).collapsible(false).show(ctx, |ui| {
                ui.label("• Every session is a separate browser profile with its own cookies, proxy and fingerprint.");
                ui.label("• Fingerprints come from real device models: cores, memory, screen, pixel ratio, graphics card, fonts and audio rate are taken from the same machine model, so they are consistent with each other.");
                ui.label("• \"Real device\" mode gives every session on one model the same canvas and audio result; \"Noise\" makes each session unique.");
                ui.label("• Check proxy shows the exit IP, its location and, with an IPQualityScore key, its fraud score.");
                ui.label("• Warm up visits sites, accepts cookie banners and clicks around so a new profile collects ordinary cookies.");
                ui.label(RichText::new("• Limits: this isolates profiles and keeps device values consistent; it does not make the browser undetectable. Sites can still use IP reputation, behaviour, TLS/HTTP2 traits and fingerprint checks not covered here.").color(DIM));
            });
            if !open {
                self.help = false;
            }
        }
    }

    fn edit_dialog(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(mut e) = self.edit.take() else { return };
        let (mut keep, mut save, mut run, mut check, mut restore, mut as_preset) = (true, false, false, false, false, false);
        let major = self.browser.as_ref().and_then(|b| b.1).unwrap_or(140);
        let preset = self.preset;
        // The saved copy: source for RESTORE and for the result of "check proxy & geo".
        let stored = if e.new { None } else { self.store.find(&e.s.uuid).map(|x| x.clone()) };
        if let Some(st) = &stored {
            e.s.geo = st.geo.clone();
        }
        let title = if e.new { "New session" } else { "Edit session" };
        let scr = ctx.screen_rect().size();
        egui::Window::new(format!("{title}  ·  {}", e.s.name)).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(scr.y - 200.0).show(ui, |ui| {
                ui.set_width((scr.x - 100.0).clamp(640.0, 930.0));
                let EditState { s, tags, line, new, .. } = &mut e;
                let bar = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
                    egui::Frame::new().fill(Color32::from_rgb(0x10, 0x12, 0x17)).corner_radius(6).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| add(ui));
                    });
                };

                section(ui, "📝", "GENERAL", |ui| {
                    ui.add_space(6.0);
                    ui.add(egui::TextEdit::singleline(&mut s.name).char_limit(50).hint_text("Session name").font(bold(15.0)).desired_width(f32::INFINITY));
                    ui.add_space(4.0);
                    ui.add(egui::TextEdit::singleline(tags).hint_text("Tags, comma separated").desired_width(f32::INFINITY));
                    ui.add_space(4.0);
                    ui.add(egui::TextEdit::multiline(&mut s.description).hint_text("Description").desired_rows(2).desired_width(f32::INFINITY));
                    ui.add_space(6.0);
                });

                section(ui, "⇄", "CONNECTION", |ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("kind").width(130.0).selected_text(s.proxy.kind.label()).show_ui(ui, |ui| {
                            for k in ProxyKind::ALL {
                                ui.selectable_value(&mut s.proxy.kind, k, k.label());
                            }
                        });
                        let b = ui.add_enabled(!*new, egui::Button::new(RichText::new("⇄  CHECK PROXY & GEO").font(bold(13.5)).color(ICON)).stroke(Stroke::new(1.0, ACCENT)));
                        if b.on_hover_text("Checks the saved settings: Save first if you changed the proxy").clicked() {
                            check = true;
                        }
                    });
                    ui.add_space(4.0);
                    let g = s.geo.clone().filter(|g| g.ok);
                    bar(ui, &mut |ui| {
                        ui.label(RichText::new(g.as_ref().map_or("IP unknown".into(), |g| g.ip.clone())).font(bold(14.0)));
                        ui.separator();
                        ui.label(g.as_ref().map_or("Not checked yet".to_string(), |g| g.place()));
                        ui.separator();
                        ui.label(RichText::new("Language").color(DIM));
                        ui.add(egui::TextEdit::singleline(&mut s.fp.language).hint_text("en-US").desired_width(60.0));
                        ui.separator();
                        ui.label(RichText::new("Time zone").color(DIM));
                        if s.fp.geo_from_proxy {
                            ui.label(g.as_ref().map_or("system".to_string(), |g| g.timezone.clone()));
                        } else {
                            ui.add(egui::TextEdit::singleline(&mut s.fp.timezone).hint_text("Europe/Warsaw").desired_width(130.0));
                        }
                        ui.separator();
                        ui.label(RichText::new("Geo").font(bold(14.0)));
                        toggle(ui, &mut s.fp.geo_from_proxy);
                        ui.label(RichText::new(if s.fp.geo_from_proxy { "Auto" } else { "Manual" }).color(DIM));
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("WebRTC").font(bold(14.0)));
                        egui::ComboBox::from_id_salt("webrtc").selected_text(s.fp.webrtc.label()).show_ui(ui, |ui| {
                            for m in WebRtc::ALL {
                                ui.selectable_value(&mut s.fp.webrtc, m, m.label());
                            }
                        });
                        ui.label(RichText::new("Fake/N/A block leaks (no address rewriting)").color(DIM).size(12.0));
                    });
                    if s.proxy.kind != ProxyKind::Direct {
                        ui.add_space(4.0);
                        proxy_editor(ui, &mut s.proxy, line, false);
                    } else {
                        ui.label(RichText::new("No proxy: the session uses this computer's own connection.").color(DIM).size(12.5));
                        ui.horizontal(|ui| {
                            ui.label("Custom DNS");
                            ui.add(egui::TextEdit::singleline(&mut s.proxy.dns).hint_text("empty = system, or 1.1.1.1;8.8.8.8").desired_width(220.0));
                        });
                    }
                    ui.add_space(6.0);
                });

                section(ui, "🖥", "FINGERPRINTS", |ui| {
                    ui.add_space(6.0);
                    let cur = devices::get(&s.fp.device);
                    bar(ui, &mut |ui| {
                        if ui.button("🎲").on_hover_text(format!("Another device from the {}", preset.label())).clicked() {
                            let r = devices::random_fingerprint(preset);
                            s.fp.device = r.device;
                            s.fp.screen = r.screen;
                        }
                        egui::ComboBox::from_id_salt("dev").width(260.0).selected_text(format!("{} — {}", cur.os.label(), cur.label)).show_ui(ui, |ui| {
                            for x in DEVICES {
                                if ui.selectable_label(x.id == cur.id, format!("{} — {}", x.os.label(), x.label)).clicked() {
                                    s.fp.device = x.id.into();
                                    s.fp.screen = x.screens[0];
                                }
                            }
                        });
                        egui::ComboBox::from_id_salt("scr").selected_text(format!("{}×{}", s.fp.screen.0, s.fp.screen.1)).show_ui(ui, |ui| {
                            for sc in devices::get(&s.fp.device).screens {
                                ui.selectable_value(&mut s.fp.screen, *sc, format!("{}×{}", sc.0, sc.1));
                            }
                        });
                        let d = devices::get(&s.fp.device);
                        ui.label(RichText::new(format!("Chrome {major}")).color(DIM)).on_hover_text("The bundled Chromium's own version");
                        ui.label(short_gpu(d.gpu_renderer)).on_hover_text(d.gpu_renderer);
                        ui.label(format!("{} cores", d.cores));
                        ui.label(format!("{} GB", d.ram));
                        ui.label(format!("{} fonts", d.os.fonts().len())).on_hover_text("Only this model's fonts are visible");
                    });
                    ui.add_space(4.0);
                    let auto = devices::user_agent(&Fingerprint { user_agent: String::new(), ..s.fp.clone() }, major);
                    ui.add(egui::TextEdit::singleline(&mut s.fp.user_agent).hint_text(auto).desired_width(f32::INFINITY).text_color(DIM));
                    ui.add_space(6.0);
                    bar(ui, &mut |ui| {
                        mode_toggle(ui, "Canvas", &mut s.fp.canvas);
                        ui.separator();
                        mode_toggle(ui, "WebGL", &mut s.fp.webgl);
                        ui.separator();
                        mode_toggle(ui, "ClientRects", &mut s.fp.client_rects);
                        ui.separator();
                        mode_toggle(ui, "Audio", &mut s.fp.audio);
                    });
                    ui.add_space(4.0);
                    bar(ui, &mut |ui| {
                        bool_toggle(ui, "MediaDevices", &mut s.fp.fake_media_devices, "Fake", "Direct");
                        ui.separator();
                        bool_toggle(ui, "Fonts", &mut s.fp.fonts, "Model only", "All");
                    });
                    ui.add_space(6.0);
                    let issues = devices::consistency(s);
                    let d = devices::get(&s.fp.device);
                    if !issues.iter().any(|(b, _)| *b) {
                        ui.label(RichText::new(format!("✔ All main signals match one {}", d.label)).color(GREEN));
                    }
                    for (bad, t) in issues {
                        ui.label(RichText::new(format!("{} {t}", if bad { "✖" } else { "ⓘ" })).color(if bad { RED } else { DIM }).size(12.5));
                    }
                    ui.label(RichText::new("Values come from one machine model so they agree with each other. Not undetectable: IP reputation, behaviour and network-level traits are outside this setting.").color(FAINT).size(12.0));
                    ui.add_space(6.0);
                });

                section(ui, "⚙", "MISC", |ui| {
                    ui.add_space(6.0);
                    bar(ui, &mut |ui| {
                        bool_toggle(ui, "DoNotTrack", &mut s.misc.do_not_track, "On", "Off");
                        ui.separator();
                        bool_toggle(ui, "Restore open tabs", &mut s.misc.restore_open_tabs, "On", "Off");
                        ui.separator();
                        bool_toggle(ui, "Hardware acceleration", &mut s.misc.hardware_acceleration, "On", "Off");
                    });
                    ui.add_space(4.0);
                    bar(ui, &mut |ui| {
                        bool_toggle(ui, "Disable images", &mut s.misc.disable_images, "On", "Off");
                        ui.separator();
                        bool_toggle(ui, "QUIC", &mut s.misc.enable_quic, "On", "Off");
                    });
                    ui.add_space(4.0);
                    ui.add(egui::TextEdit::singleline(&mut s.misc.start_url).hint_text("Start page (optional)").desired_width(f32::INFINITY));
                    ui.add_space(6.0);
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let ok = !e.s.name.trim().is_empty();
                if ui.add_enabled(stored.is_some(), egui::Button::new(RichText::new("RESTORE").font(bold(13.5)))).on_hover_text("Back to the saved settings").clicked() {
                    restore = true;
                }
                if ui.add_enabled(ok, egui::Button::new(RichText::new("SAVE AS PRESET").font(bold(13.5)))).on_hover_text("Saves these settings under the session's name; new sessions can copy them").clicked() {
                    as_preset = true;
                }
                if !ok {
                    ui.label(RichText::new("The name needs 1 to 50 characters").color(RED));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(ok, egui::Button::new(RichText::new("▶  SAVE & RUN").font(bold(13.5)).color(ICON)).stroke(Stroke::new(1.0, ACCENT))).clicked() {
                        save = true;
                        run = true;
                    }
                    if ui.add_enabled(ok, egui::Button::new(RichText::new("SAVE").font(bold(13.5)).color(ICON)).stroke(Stroke::new(1.0, ACCENT))).clicked() {
                        save = true;
                    }
                    if ui.button(RichText::new("CANCEL").font(bold(13.5))).clicked() {
                        keep = false;
                    }
                });
            });
        });
        if as_preset {
            let name: String = e.s.name.trim().chars().take(50).collect();
            let base = match devices::get(&e.s.fp.device).os {
                devices::Os::Windows => Preset::Windows,
                devices::Os::Mac => Preset::Mac,
                devices::Os::Linux => Preset::Linux,
            };
            let mut template = e.s.clone();
            template.tags = clean_tags(&e.tags);
            self.store.presets.retain(|p| p.name != name);
            self.store.presets.push(UserPreset { name: name.clone(), base, template });
            self.dirty = true;
            self.toast(format!("Preset \"{name}\" saved"));
        }
        if restore {
            if let Some(st) = stored {
                e.tags = st.tags.join(", ");
                e.s = st;
            }
        }
        if check && !save {
            actions.push(Action::Check(e.s.uuid.clone()));
        }
        if save {
            let mut s = e.s;
            let uuid = s.uuid.clone();
            s.name = s.name.trim().chars().take(50).collect();
            s.tags = clean_tags(&e.tags);
            s.modified = now();
            if e.new {
                self.store.sessions.push(s);
            } else if let Some(old) = self.store.find(&s.uuid) {
                let proxy_changed = old.proxy != s.proxy;
                *old = s;
                if proxy_changed {
                    old.geo = None;
                }
                old.log("Settings changed");
            }
            if run {
                actions.push(Action::Start(uuid));
            }
            self.dirty = true;
        } else if keep {
            self.edit = Some(e);
        }
    }

    fn proxy_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.proxy_dlg.take() else { return };
        let mut keep = true;
        egui::Window::new("Set proxy").anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).resizable(false).default_width(520.0).show(ctx, |ui| {
            ui.label(RichText::new(format!("For {} session(s)", d.targets.len())).color(DIM));
            proxy_editor(ui, &mut d.proxy, &mut d.line, true);
            ui.horizontal(|ui| {
                if ui.add_enabled(!d.checking, egui::Button::new("Check")).clicked() {
                    d.checking = true;
                    d.result = None;
                    let (p, key, tx, c) = (d.proxy.clone(), self.ipqs_key(), self.tx.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        let _ = tx.send(Msg::DialogGeo(proxy::check(&p, &key)));
                        c.request_repaint();
                    });
                }
                if d.checking {
                    ui.spinner();
                }
            });
            if let Some(g) = &d.result {
                geo_line(ui, g);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Apply").font(bold(14.0))).fill(ACCENT)).clicked() {
                    let geo = d.result.clone().filter(|g| g.ok);
                    for id in &d.targets {
                        if let Some(s) = self.store.find(id) {
                            s.proxy = d.proxy.clone();
                            s.geo = geo.clone();
                            s.modified = now();
                            s.log(format!("Proxy set to {} {}", d.proxy.kind.label(), d.proxy.short()));
                        }
                    }
                    self.dirty = true;
                    keep = false;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
        if keep {
            self.proxy_dlg = Some(d);
        }
    }

    fn warm_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.warm_dlg.take() else { return };
        let mut keep = true;
        let name = self.store.sessions.iter().find(|s| s.uuid == w.uuid).map(|s| s.name.clone()).unwrap_or_default();
        egui::Window::new(format!("Warm up — {name}")).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).resizable(false).default_width(520.0).show(ctx, |ui| {
            ui.label(RichText::new("Opens popular sites in this session's browser: accepts cookie banners, scrolls and follows links, so the profile collects ordinary cookies, local storage and history. It does not solve captchas, and sites that block automated visits give fewer cookies.").color(DIM));
            ui.horizontal(|ui| {
                ui.label("Sites (one per line)");
                if ui.small_button(if w.cc.is_empty() { "Default list".to_string() } else { format!("Popular in {}", w.cc) }).on_hover_text("Fill the list from the country of this session's connection (run Check proxy & geo first)").clicked() {
                    w.urls = crate::model::regional_sites(&w.cc);
                }
                if ui.small_button("Load .txt").clicked() {
                    if let Some(t) = rfd::FileDialog::new().add_filter("Text", &["txt"]).pick_file().and_then(|p| std::fs::read_to_string(p).ok()) {
                        w.urls = t;
                    }
                }
            });
            ui.add(egui::TextEdit::multiline(&mut w.urls).desired_rows(6).desired_width(500.0));
            egui::Grid::new("wg").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Number of sites");
                ui.add(egui::DragValue::new(&mut w.count).range(browser::URL_COUNT.0..=browser::URL_COUNT.1));
                ui.end_row();
                ui.label("Viewing depth (pages per site)");
                ui.add(egui::DragValue::new(&mut w.depth).range(browser::VIEW_DEPTH.0..=browser::VIEW_DEPTH.1));
                ui.end_row();
                ui.label("Time on page (seconds)");
                ui.add(egui::DragValue::new(&mut w.time).range(browser::TIME_PER_URL.0..=browser::TIME_PER_URL.1));
                ui.end_row();
                ui.label("Referrer");
                egui::ComboBox::from_id_salt("ref").selected_text(REFERRERS[w.referrer].0).show_ui(ui, |ui| {
                    for (i, (n, _)) in REFERRERS.iter().enumerate() {
                        ui.selectable_value(&mut w.referrer, i, *n);
                    }
                });
                ui.end_row();
            });
            ui.label(RichText::new("Referrer: the first page of each site is opened with the search engine's address as its Referer. No real search is made.").color(FAINT).size(12.0));
            ui.checkbox(&mut w.accept, "Accept cookie banners");
            ui.checkbox(&mut w.set_warmed, "Set \"Warmed\" status afterwards");
            let secs = w.count as u64 * w.depth as u64 * (3 + w.time as u64);
            ui.label(RichText::new(format!("About {}:{:02} without slow pages or captchas", secs / 60, secs % 60)).color(DIM).size(12.5));
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Start").font(bold(14.0))).fill(ACCENT)).clicked() {
                    let job = Warmup {
                        urls: w.urls.lines().map(String::from).collect(),
                        url_count: w.count,
                        time_per_url: w.time,
                        view_depth: w.depth,
                        accept_cookies: w.accept,
                        referrer: REFERRERS[w.referrer].1.into(),
                    };
                    let (id, set_warmed) = (w.uuid.clone(), w.set_warmed);
                    self.with_browser(ctx, &w.uuid, "warming up", false, move |conn, tx| {
                        let (tx2, id2) = (tx.clone(), id.clone());
                        let (n, cookies) = browser::run_warmup(conn, &job, &move |m| {
                            let _ = tx2.send(Msg::Busy(id2.clone(), Some(m)));
                        })?;
                        if set_warmed {
                            let _ = tx.send(Msg::Warmed(id));
                        }
                        Ok(format!("Warm-up finished: {n} sites visited, {cookies} new cookies"))
                    });
                    keep = false;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
        if keep {
            self.warm_dlg = Some(w);
        }
    }

    fn checker_window(&mut self, ctx: &egui::Context) {
        let Some(mut c) = self.checker.take() else { return };
        let mut open = true;
        egui::Window::new("Proxy checker").open(&mut open).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).default_width(780.0).show(ctx, |ui| {
            ui.label(RichText::new("Paste proxies, one per line: host:port, host:port:login:password, or socks5://login:pass@host:port").color(DIM));
            ui.add(egui::TextEdit::multiline(&mut c.text).desired_rows(5).desired_width(760.0).font(egui::TextStyle::Monospace));
            ui.horizontal(|ui| {
                ui.label("Lines without a type are");
                for k in [ProxyKind::Http, ProxyKind::Socks5] {
                    ui.selectable_value(&mut c.kind, k, k.label());
                }
                if ui.add(egui::Button::new(RichText::new("Check all").font(bold(14.0))).fill(ACCENT)).clicked() {
                    c.gen += 1;
                    c.rows = c.text.lines().filter(|l| !l.trim().is_empty()).take(1000).map(|l| (l.trim().to_string(), Proxy::parse(l, c.kind), None)).collect();
                    let jobs: VecDeque<(usize, Proxy)> = c.rows.iter().enumerate().filter_map(|(i, r)| r.1.clone().map(|p| (i, p))).collect();
                    for r in c.rows.iter_mut().filter(|r| r.1.is_none()) {
                        r.2 = Some(GeoInfo { error: "Can't read this line".into(), ..Default::default() });
                    }
                    let queue = Arc::new(Mutex::new(jobs));
                    let key = self.ipqs_key();
                    for _ in 0..16 {
                        let (q, tx, cx, key, gen) = (queue.clone(), self.tx.clone(), ctx.clone(), key.clone(), c.gen);
                        std::thread::spawn(move || loop {
                            let job = q.lock().ok().and_then(|mut q| q.pop_front());
                            let Some((i, p)) = job else { break };
                            let _ = tx.send(Msg::Bulk(gen, i, proxy::check(&p, &key)));
                            cx.request_repaint();
                        });
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Import file").clicked() {
                    if let Some(t) = rfd::FileDialog::new().add_filter("Proxy list", &["txt", "csv"]).pick_file().and_then(|p| std::fs::read_to_string(p).ok()) {
                        c.text = format!("{}\n{}", c.text.trim_end(), t.trim());
                    }
                }
                if ui.button("Remove duplicates").clicked() {
                    let mut seen = std::collections::HashSet::new();
                    c.text = c.text.lines().map(str::trim).filter(|l| !l.is_empty() && seen.insert(l.to_string())).collect::<Vec<_>>().join("\n");
                }
                let dead: Vec<String> = c.rows.iter().filter(|r| r.2.as_ref().is_some_and(|g| !g.ok)).map(|r| r.0.clone()).collect();
                if ui.add_enabled(!dead.is_empty(), egui::Button::new(format!("Remove {} dead", dead.len()))).clicked() {
                    c.text = c.text.lines().filter(|l| !dead.iter().any(|d| d == l.trim())).collect::<Vec<_>>().join("\n");
                }
                if ui.button("Export list").clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_file_name("proxies.txt").save_file() {
                        let _ = std::fs::write(p, &c.text);
                    }
                }
            });
            let done = c.rows.iter().filter(|r| r.2.is_some()).count();
            let ok: Vec<Proxy> = c.rows.iter().filter(|r| r.2.as_ref().is_some_and(|g| g.ok)).filter_map(|r| r.1.clone()).collect();
            if !c.rows.is_empty() {
                ui.label(RichText::new(format!("{done}/{} checked · {} working", c.rows.len(), ok.len())).color(DIM));
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                egui::Grid::new("chk").striped(true).num_columns(5).spacing([14.0, 4.0]).show(ui, |ui| {
                    for (line, _, g) in &c.rows {
                        ui.label(RichText::new(line.chars().take(42).collect::<String>()).monospace().size(12.0));
                        match g {
                            None => {
                                ui.spinner();
                            }
                            Some(g) if g.ok => {
                                ui.label(RichText::new("● works").color(GREEN));
                                ui.label(&g.ip);
                                ui.label(g.place());
                                ui.label(format!("{} ms{}", g.latency_ms, g.fraud_score.map(|f| format!(" · fraud {f}")).unwrap_or_default()));
                            }
                            Some(g) => {
                                ui.label(RichText::new("● dead").color(RED));
                                ui.label(RichText::new(&g.error).color(DIM));
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            ui.horizontal(|ui| {
                let sel: Vec<String> = self.store.sessions.iter().filter(|s| self.selected.contains(&s.uuid)).map(|s| s.uuid.clone()).collect();
                if ui.add_enabled(!ok.is_empty() && !sel.is_empty(), egui::Button::new(format!("Give working proxies to the {} selected sessions", sel.len()))).clicked() {
                    for (i, id) in sel.iter().enumerate() {
                        let p = ok[i % ok.len()].clone();
                        let geo = c.rows.iter().find(|r| r.1.as_ref() == Some(&p)).and_then(|r| r.2.clone());
                        if let Some(s) = self.store.find(id) {
                            s.log(format!("Proxy set to {} {}", p.kind.label(), p.short()));
                            s.proxy = p;
                            s.geo = geo;
                            s.modified = now();
                        }
                    }
                    self.dirty = true;
                    self.toast("Proxies assigned");
                }
                if ui.add_enabled(!ok.is_empty(), egui::Button::new("Copy working")).clicked() {
                    let t = c.rows.iter().filter(|r| r.2.as_ref().is_some_and(|g| g.ok)).map(|r| r.0.clone()).collect::<Vec<_>>().join("\n");
                    ctx.copy_text(t);
                    self.toast("Working proxies copied");
                }
            });
        });
        if self.store.proxy_list != c.text {
            self.store.proxy_list = c.text.clone();
            self.dirty = true;
        }
        if open {
            self.checker = Some(c);
        }
    }
}
