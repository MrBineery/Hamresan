#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod server;
mod storage;

use anyhow::Result;
use clap::Parser;
use qrcode::render::unicode;
use qrcode::QrCode;
use rand::Rng;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Icon, WindowBuilder},
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, TrayIconBuilder, TrayIconEvent,
};
use wry::WebViewBuilder;

const TELEGRAM_URL: &str = "https://t.me/iwSajaD";

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    Tray(TrayIconEvent),
}

#[derive(Parser, Debug)]
#[command(
    name = "hamresan",
    version,
    about = "Local network file sharing. Open on any browser. No app needed."
)]
struct Cli {
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    #[arg(short, long)]
    folder: Option<PathBuf>,

    #[arg(long)]
    headless: bool,

    #[arg(long)]
    no_qr: bool,

    #[arg(long)]
    pin: Option<String>,

    #[arg(long)]
    no_pin: bool,

    /// Start minimized to system tray
    #[arg(long)]
    minimized: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let folder = cli.folder.clone().unwrap_or_else(|| {
        dirs::download_dir()
            .map(|d| d.join("Hamresan"))
            .unwrap_or_else(|| PathBuf::from("./shared"))
    });

    std::fs::create_dir_all(&folder)?;
    let folder = folder.canonicalize().unwrap_or(folder);
    let folder = strip_extended_prefix(folder);

    let ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".into());
    let local_url = format!("http://localhost:{}", cli.port);
    let network_url = format!("http://{}:{}", ip, cli.port);

    let pin_opt: Option<String> = if cli.no_pin {
        None
    } else if let Some(p) = &cli.pin {
        Some(p.clone())
    } else {
        Some(generate_pin())
    };

    println!();
    println!("  📡 Hamresan — هم‌رسان");
    println!("  ──────────────────────────────────────────");
    println!("  Folder : {}", folder.display());
    println!("  Local  : {}", local_url);
    println!("  Network: {}", network_url);
    println!();

    if let Some(pin) = &pin_opt {
        println!("  🔐 PIN:  {}", format_pin(pin));
        println!("     Enter this on any phone/tablet that connects.");
        println!("     Devices on this PC (localhost) skip the PIN.");
        println!();
    } else {
        println!("  ⚠️  PIN protection is DISABLED");
        println!();
    }

    if !cli.no_qr && ip != "127.0.0.1" {
        println!("  Scan this QR code with your phone:");
        println!();
        if let Ok(code) = QrCode::new(network_url.as_bytes()) {
            let qr = code
                .render::<unicode::Dense1x2>()
                .dark_color(unicode::Dense1x2::Light)
                .light_color(unicode::Dense1x2::Dark)
                .build();
            println!("{}", qr);
        }
        println!();
    }

    println!("  💬 انتقاد و پیشنهاد: {}", TELEGRAM_URL);
    println!();
    println!("  Press Ctrl+C to stop, or use tray menu > Exit.");
    println!();

    let server_folder = folder.clone();
    let server_url = network_url.clone();
    let server_port = cli.port;
    let server_pin = pin_opt.clone();
    thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("failed to create runtime: {}", e);
                return;
            }
        };
        rt.block_on(async move {
            let state = server::AppState::new(server_folder, server_url, server_pin);
            if let Err(e) = server::run(server_port, state).await {
                eprintln!("server error: {:#}", e);
            }
        });
    });

    thread::sleep(Duration::from_millis(400));

    if cli.headless {
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    }

    let title = match &pin_opt {
        Some(p) => format!("Hamresan  —  PIN: {}", format_pin(p)),
        None => "Hamresan".to_string(),
    };
    run_gui(&local_url, &title, cli.minimized)
}

fn run_gui(local_url: &str, title: &str, start_minimized: bool) -> Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let proxy_menu = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy_menu.send_event(UserEvent::Menu(event));
    }));

    let proxy_tray = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy_tray.send_event(UserEvent::Tray(event));
    }));

    let window_icon = load_window_icon().ok();

    let mut builder = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 720.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(600.0, 480.0))
        .with_visible(!start_minimized);

    if let Some(icon) = window_icon {
        builder = builder.with_window_icon(Some(icon));
    }

    let window = builder.build(&event_loop)?;

    let _webview = WebViewBuilder::new()
        .with_url(local_url)
        .with_devtools(false)
        .build(&window)?;

    // ---------- Tray menu ----------
    let menu = Menu::new();
    let open_item     = MenuItem::new("Open Hamresan", true, None);
    let feedback_item = MenuItem::new("💬 انتقاد و پیشنهاد", true, None);
    let exit_item     = MenuItem::new("Exit", true, None);

    let _ = menu.append(&open_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&feedback_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&exit_item);

    let open_id     = open_item.id().clone();
    let feedback_id = feedback_item.id().clone();
    let exit_id     = exit_item.id().clone();

    let tray_icon_image = load_tray_icon().ok();
    let _tray = match tray_icon_image {
        Some(icon) => TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(title)
            .with_icon(icon)
            .build()
            .ok(),
        None => None,
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                window.set_visible(false);
            }

            Event::UserEvent(UserEvent::Menu(e)) => {
                if e.id == open_id {
                    window.set_visible(true);
                    window.set_focus();
                } else if e.id == feedback_id {
                    let _ = open::that(TELEGRAM_URL);
                } else if e.id == exit_id {
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::UserEvent(UserEvent::Tray(e)) => {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    ..
                } = e
                {
                    window.set_visible(true);
                    window.set_focus();
                }
            }

            _ => {}
        }
    });
}

fn load_window_icon() -> Result<Icon> {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icon.ico"
    ));
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico)?;
    let img = img.into_rgba8();
    let (w, h) = img.dimensions();
    let icon = Icon::from_rgba(img.into_raw(), w, h)?;
    Ok(icon)
}

fn load_tray_icon() -> Result<tray_icon::Icon> {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icon.ico"
    ));
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico)?;
    let img = img.into_rgba8();
    let (w, h) = img.dimensions();
    let icon = tray_icon::Icon::from_rgba(img.into_raw(), w, h)
        .map_err(|e| anyhow::anyhow!("tray icon: {:?}", e))?;
    Ok(icon)
}

fn generate_pin() -> String {
    let mut rng = rand::thread_rng();
    (0..6).map(|_| rng.gen_range(0..10).to_string()).collect()
}

fn format_pin(pin: &str) -> String {
    if pin.len() == 6 {
        format!("{} {} {}", &pin[0..2], &pin[2..4], &pin[4..6])
    } else {
        pin.to_string()
    }
}

fn strip_extended_prefix(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p
    }
}