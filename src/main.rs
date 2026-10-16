mod config;
mod db;
mod idgen;
mod repo_settings;
mod routes;
mod scanner;
mod state;

use crate::state::AppState;
use actix_web::{web, App, HttpServer};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn print_help() {
    println!(
        "closetag - local tag management system

USAGE:
    closetag [OPTIONS]

OPTIONS:
    --port <PORT>       Override the configured port (0 = auto)
    --no-browser        Do not open the browser automatically
    --data-dir <PATH>   Override the data directory (default: user data dir)
    -h, --help          Show this help
"
    );
}

fn bind_listener(port: u16) -> anyhow::Result<TcpListener> {
    if port == 0 {
        return Ok(TcpListener::bind(("127.0.0.1", 0))?);
    }
    for p in port..=port.saturating_add(50) {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", p)) {
            if p != port {
                println!("[closetag] port {port} is busy, using {p} instead");
            }
            return Ok(l);
        }
    }
    anyhow::bail!("no free port found in range {port}..{}", port + 50)
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let mut port_override: Option<u16> = None;
    let mut no_browser = false;
    let mut data_dir_override: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port_override = args.next().and_then(|v| v.parse().ok()),
            "--no-browser" => no_browser = true,
            "--data-dir" => data_dir_override = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            other => eprintln!("[closetag] ignoring unknown argument: {other}"),
        }
    }

    let data_dir = match data_dir_override {
        Some(d) => d,
        None => config::data_dir()?,
    };
    let mut cfg = config::load(&data_dir)?;
    if let Some(p) = port_override {
        cfg.server.port = p;
    }
    if no_browser {
        cfg.server.open_browser = false;
    }

    let state = Arc::new(AppState::new(data_dir.clone(), cfg));

    let listener = bind_listener(state.config.lock().unwrap().server.port)?;
    let port = listener.local_addr()?.port();
    state.port.store(port, std::sync::atomic::Ordering::Relaxed);

    if state.config.lock().unwrap().server.open_browser {
        let url = format!("http://127.0.0.1:{port}/");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            if let Err(e) = open::that(&url) {
                eprintln!("[closetag] could not open the browser automatically: {e}");
                eprintln!("[closetag] please open {url} manually");
            }
        });
    }

    if state.config.lock().unwrap().server.exit_when_closed {
        let st = state.clone();
        std::thread::spawn(move || {
            let no_connect_deadline = Duration::from_secs(90);
            loop {
                std::thread::sleep(Duration::from_millis(500));
                if st.ws.clients() == 0
                    && !st.ws.ever_connected()
                    && st.started.elapsed() >= no_connect_deadline
                {
                    println!("[closetag] no page connected within 90s, exiting");
                    std::process::exit(0);
                }
                if st.ws.should_exit(Duration::from_secs(3)) {
                    println!("[closetag] page closed, exiting");
                    std::process::exit(0);
                }
            }
        });
    }

    println!(
        "[closetag] v{} listening on http://127.0.0.1:{port}",
        config::VERSION
    );
    println!("[closetag] data dir: {}", state.data_dir.display());

    let server_state = state.clone();
    HttpServer::new(move || {
        let st = server_state.clone();
        App::new()
            .app_data(web::Data::from(st))
            .configure(routes::config)
    })
    .listen(listener)?
    .run()
    .await?;

    Ok(())
}
