use super::*;
use crate::browser::Warmup;
use crate::devices::DEVICES;
use std::collections::VecDeque;
use std::sync::Mutex;

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
                        pref_row(ui, "IPQualityScore API key", true, |ui| {
                            egui::Frame::new().fill(Color32::from_rgb(0x1C, 0x1F, 0x26)).corner_radius(3).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                                changed |= ui.add(egui::TextEdit::singleline(&mut st.ipqs_key).hint_text("Your private key").password(true).frame(false).desired_width(168.0)).changed();
                            });
                        });
                    });
                    section(ui, "🌐", "BROWSER", |ui| {
                        let found = browser::find_browser(&st.browser_path);
                        pref_row(ui, "Browser program", false, |ui| {
                            if ui.button("Choose…").clicked() {
                                if let Some(p) = rfd::FileDialog::new().add_filter("Program", &["exe"]).pick_file() {
                                    st.browser_path = p.display().to_string();
                                    changed = true;
                                }
                            }
                            if !st.browser_path.is_empty() && ui.button("Auto-detect").clicked() {
                                st.browser_path.clear();
                                changed = true;
                            }
                            let t = found.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "not found".into());
                            ui.label(RichText::new(t).color(if found.is_some() { DIM } else { RED }));
                        });
                        ui.label(
                            RichText::new("The bundled Nebula Chromium (built by build.ps1) reports cores, memory, graphics card and client hints natively, in workers too. Other browsers get the values from a small extension: Chromium and Edge accept it, recent Google Chrome ignores it.")
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
            self.browser = detect(&self.store.settings);
            self.dirty = true;
        }
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        self.edit_dialog(ctx);
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

        if let Some(mut n) = self.multiple.take() {
            let mut keep = true;
            let sel: Vec<String> = self.selected.iter().cloned().collect();
            center("Multiple sessions".into()).show(ctx, |ui| {
                ui.label(RichText::new("Create several sessions at once").font(bold(15.0)));
                ui.horizontal(|ui| {
                    ui.label("How many");
                    // 1..=250 from the original's "Create quick sessions" schema (0x14279b0d0)
                    ui.add(egui::DragValue::new(&mut n).range(1..=250));
                    egui::ComboBox::from_id_salt("mpreset").selected_text(self.preset.label()).show_ui(ui, |ui| {
                        for p in Preset::ALL {
                            ui.selectable_value(&mut self.preset, p, p.label());
                        }
                    });
                    if ui.button("Create").clicked() {
                        for _ in 0..n {
                            self.new_session(true);
                        }
                        self.toast(format!("Created {n} sessions"));
                        keep = false;
                    }
                });
                ui.label(RichText::new("Each one gets its own real-device fingerprint from the chosen preset.").color(DIM));
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
                if ui.button("Close").clicked() {
                    keep = false;
                }
            });
            if keep {
                self.multiple = Some(n);
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
                    ui.add(egui::TextEdit::singleline(&mut t).hint_text("tag name").desired_width(180.0));
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

    fn edit_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.edit.take() else { return };
        let mut keep = true;
        let mut save = false;
        let major = self.browser.as_ref().and_then(|b| b.1).unwrap_or(140);
        let edge = self.browser.as_ref().is_some_and(|b| browser::is_edge(&b.0));
        let preset = self.preset;
        let title = if e.new { "New session" } else { "Edit session" };
        egui::Window::new(title).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(ctx.screen_rect().height() - 170.0).show(ui, |ui| {
                ui.set_width(650.0);
                let s = &mut e.s;
                egui::Grid::new("gen").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut s.name).char_limit(50).desired_width(400.0));
                    ui.end_row();
                    ui.label("Description");
                    ui.add(egui::TextEdit::multiline(&mut s.description).desired_rows(2).desired_width(400.0));
                    ui.end_row();
                    ui.label("Tags");
                    ui.add(egui::TextEdit::singleline(&mut e.tags).hint_text("comma separated").desired_width(400.0));
                    ui.end_row();
                });
                ui.separator();
                ui.label(RichText::new("Real device fingerprint").font(bold(15.5)).color(TEXT));
                ui.label(RichText::new("Values are taken from one machine model so they agree with each other. Known limits: not undetectable; IP reputation, behaviour and network-level traits are outside this setting.").color(DIM).size(12.5));
                let cur = devices::get(&s.fp.device);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("dev").width(370.0).selected_text(cur.label).show_ui(ui, |ui| {
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
                    if ui.button("🎲 Random").on_hover_text(format!("Another device from the {}", preset.label())).clicked() {
                        let r = devices::random_fingerprint(preset);
                        s.fp.device = r.device;
                        s.fp.screen = r.screen;
                    }
                });
                let d = devices::get(&s.fp.device);
                egui::Grid::new("devfacts").num_columns(4).spacing([16.0, 3.0]).show(ui, |ui| {
                    let f = |ui: &mut Ui, k: &str, v: String| {
                        ui.label(RichText::new(k).color(DIM).size(12.5));
                        ui.label(RichText::new(v).size(12.5));
                    };
                    f(ui, "System", format!("{} ({})", d.os.label(), d.os.platform()));
                    f(ui, "CPU cores", d.cores.to_string());
                    ui.end_row();
                    f(ui, "Memory", format!("{} GB (as reported)", d.ram));
                    f(ui, "Pixel ratio", format!("{}", d.dpr));
                    ui.end_row();
                    f(ui, "Audio rate", format!("{} Hz", d.sample_rate));
                    f(ui, "Fonts", format!("{} system fonts", d.os.fonts().len()));
                    ui.end_row();
                });
                ui.label(RichText::new(format!("Graphics: {}", d.gpu_renderer)).color(DIM).size(12.0));
                ui.add_space(4.0);
                egui::Grid::new("modes").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
                    let mode = |ui: &mut Ui, label: &str, m: &mut FpMode| {
                        ui.label(label);
                        egui::ComboBox::from_id_salt(label).width(130.0).selected_text(m.label()).show_ui(ui, |ui| {
                            for x in FpMode::ALL {
                                ui.selectable_value(m, x, x.label());
                            }
                        });
                    };
                    mode(ui, "Canvas", &mut s.fp.canvas);
                    mode(ui, "WebGL", &mut s.fp.webgl);
                    ui.end_row();
                    mode(ui, "Audio", &mut s.fp.audio);
                    mode(ui, "Client rects", &mut s.fp.client_rects);
                    ui.end_row();
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut s.fp.fonts, "Only this model's fonts");
                    ui.checkbox(&mut s.fp.fake_media_devices, "Fake media devices");
                    ui.checkbox(&mut s.fp.hide_webgpu, "Hide WebGPU");
                    ui.checkbox(&mut s.fp.webrtc_protect, "WebRTC leak protection");
                });
                egui::Grid::new("loc").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Language");
                    ui.add(egui::TextEdit::singleline(&mut s.fp.language).hint_text("en-US").desired_width(120.0));
                    ui.end_row();
                    ui.label("Time zone");
                    ui.horizontal(|ui| {
                        ui.add_enabled(!s.fp.geo_from_proxy, egui::TextEdit::singleline(&mut s.fp.timezone).hint_text("Europe/Warsaw").desired_width(160.0));
                        ui.checkbox(&mut s.fp.geo_from_proxy, "follow the proxy's location");
                    });
                    ui.end_row();
                    ui.label("User agent");
                    let auto = devices::user_agent(&Fingerprint { user_agent: String::new(), ..s.fp.clone() }, major, edge);
                    ui.add(egui::TextEdit::singleline(&mut s.fp.user_agent).hint_text(auto).desired_width(480.0));
                    ui.end_row();
                });
                let issues = devices::consistency(s);
                egui::Frame::new().fill(Color32::from_rgb(0x15, 0x19, 0x21)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.set_width(630.0);
                    if !issues.iter().any(|(b, _)| *b) {
                        ui.label(RichText::new(format!("✔ All main signals match one {}", d.label)).color(GREEN));
                    }
                    for (bad, t) in issues {
                        ui.label(RichText::new(format!("{} {t}", if bad { "✖" } else { "ⓘ" })).color(if bad { RED } else { DIM }).size(12.5));
                    }
                });
                ui.separator();
                ui.label(RichText::new("Connection").font(bold(15.5)).color(TEXT));
                proxy_editor(ui, &mut s.proxy, &mut e.line);
                ui.separator();
                ui.label(RichText::new("Browser").font(bold(15.5)).color(TEXT));
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut s.misc.restore_open_tabs, "Restore open tabs");
                    ui.checkbox(&mut s.misc.hardware_acceleration, "Hardware acceleration");
                    ui.checkbox(&mut s.misc.disable_images, "Disable images");
                    ui.checkbox(&mut s.misc.enable_quic, "Enable QUIC");
                    ui.checkbox(&mut s.misc.do_not_track, "Do not track");
                });
                ui.horizontal(|ui| {
                    ui.label("Start page");
                    ui.add(egui::TextEdit::singleline(&mut s.misc.start_url).hint_text("optional").desired_width(400.0));
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let ok = !e.s.name.trim().is_empty();
                if ui.add_enabled(ok, egui::Button::new(RichText::new("Save").font(bold(14.0))).fill(ACCENT)).clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
                if !ok {
                    ui.label(RichText::new("The name needs 1 to 50 characters").color(RED));
                }
            });
        });
        if save {
            let mut s = e.s;
            s.name = s.name.trim().chars().take(50).collect();
            s.tags = e.tags.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
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
            proxy_editor(ui, &mut d.proxy, &mut d.line);
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
            ui.label(RichText::new("The cookie bot visits sites in this session's browser, accepts cookie banners, scrolls and follows links, so the profile collects ordinary cookies.").color(DIM));
            ui.label("Sites (one per line)");
            ui.add(egui::TextEdit::multiline(&mut w.urls).desired_rows(6).desired_width(500.0));
            egui::Grid::new("wg").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Sites to visit");
                ui.add(egui::DragValue::new(&mut w.count).range(browser::URL_COUNT.0..=browser::URL_COUNT.1));
                ui.end_row();
                ui.label("Seconds per page");
                ui.add(egui::DragValue::new(&mut w.time).range(browser::TIME_PER_URL.0..=browser::TIME_PER_URL.1));
                ui.end_row();
                ui.label("Pages deep per site");
                ui.add(egui::DragValue::new(&mut w.depth).range(browser::VIEW_DEPTH.0..=browser::VIEW_DEPTH.1));
                ui.end_row();
            });
            ui.checkbox(&mut w.accept, "Accept cookie banners");
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Start warm-up").font(bold(14.0))).fill(ACCENT)).clicked() {
                    let job = Warmup { urls: w.urls.lines().map(String::from).collect(), url_count: w.count, time_per_url: w.time, view_depth: w.depth, accept_cookies: w.accept };
                    let id = w.uuid.clone();
                    self.with_browser(ctx, &w.uuid, "warming up", false, move |port, tx| {
                        let (tx2, id2) = (tx.clone(), id.clone());
                        let n = browser::run_warmup(port, &job, &move |m| {
                            let _ = tx2.send(Msg::Busy(id2.clone(), Some(m)));
                        })?;
                        let _ = tx.send(Msg::Warmed(id));
                        Ok(format!("Warm-up finished: {n} sites visited"))
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
        if open {
            self.checker = Some(c);
        }
    }
}
