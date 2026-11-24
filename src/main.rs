#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod browser;
mod devices;
mod import;
mod model;
mod proxy;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    // Dev aid for tests/probe: `--print-args <win|mac|linux> <out.txt> [device id]` writes the exact browser switches a fresh
    // session of that preset would get (one per line; a release build has no console, hence the file).
    if let Some(i) = args.iter().position(|a| a == "--print-args") {
        let preset = match args.get(i + 1).map(String::as_str) {
            Some("mac") => devices::Preset::Mac,
            Some("linux") => devices::Preset::Linux,
            _ => devices::Preset::Windows,
        };
        let out = args.get(i + 2).expect("--print-args needs an output file");
        let (settings, mut s) = (model::Settings::default(), model::Session::new("probe".into(), "Desktop", preset));
        // Optional third argument: a device template id (e.g. mba-m2), instead of a random one of the preset.
        if let Some(id) = args.get(i + 3).filter(|x| !x.starts_with("--")) {
            s.fp.device = id.clone();
            s.fp.screen = devices::get(id).screens[0];
        }
        let ua = devices::user_agent(&s.fp, 158);
        let a = browser::build_args(&s, None, 0, &settings.closed_ports, &ua, false).args;
        // The probe adds its own debugging port and profile; the session's would clash.
        let a: Vec<String> = a.into_iter().filter(|x| !x.starts_with("--remote-debugging-port=") && !x.starts_with("--user-data-dir=")).collect();
        std::fs::write(out, a.join("\n")).expect("cannot write the output file");
        return Ok(());
    }
    // Dev aid: `--launch-test <out.txt>` starts a session the way the manager does (sandbox included), waits for the
    // DevTools port, writes "ok <version>" or the error, and closes it.
    if let Some(i) = args.iter().position(|a| a == "--launch-test") {
        let out = args.get(i + 1).expect("--launch-test needs an output file");
        let (settings, s) = (model::Settings::default(), model::Session::new("launch-test".into(), "Desktop", devices::Preset::Windows));
        let res = browser::launch(&settings, &s, true, None).and_then(|mut r| {
            let v = browser::wait_ready(r.port, 30);
            let alive = r.alive();
            r.stop();
            v.map(|_| format!("ok alive_after_ready={alive}"))
        });
        std::fs::write(out, res.unwrap_or_else(|e| format!("FAILED: {e}"))).expect("cannot write the output file");
        return Ok(());
    }
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);

    let mut vp = egui::ViewportBuilder::default()
        .with_title("Nebula Sessions")
        .with_inner_size([1369.0, 681.0])
        .with_min_inner_size([1100.0, 560.0])
        .with_decorations(false);
    if shot.is_some() {
        vp = vp.with_active(false);
        #[cfg(windows)]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native("Nebula Sessions", options, Box::new(move |cc| Ok(Box::new(ui::App::new(cc, shot)))))
}
