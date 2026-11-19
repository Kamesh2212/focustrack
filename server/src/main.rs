//! `focustrack-server` — run the sync server.
//!
//! Environment:
//!   FT_SYNC_ADDR         listen address (default 127.0.0.1:8787)
//!   FT_SYNC_DB           SQLite file (default ./focustrack-sync.db)
//!   FT_SYNC_TRUST_PROXY  1 when behind a reverse proxy such as Caddy

use focustrack_server::{app, serve, store, Config};

#[tokio::main]
async fn main() {
    let addr = std::env::var("FT_SYNC_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let db_path = std::env::var("FT_SYNC_DB").unwrap_or_else(|_| "focustrack-sync.db".into());
    let trust_proxy = std::env::var("FT_SYNC_TRUST_PROXY").is_ok_and(|v| v == "1");

    let conn = match store::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("can't open database {db_path}: {e}");
            std::process::exit(1);
        }
    };
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("can't listen on {addr}: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "FocusTrack sync server {} listening on http://{addr} (database {db_path})",
        env!("CARGO_PKG_VERSION")
    );
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        println!("shutting down");
    };
    if let Err(e) = serve(listener, app(conn, Config { trust_proxy }), shutdown).await {
        eprintln!("server error: {e}");
        std::process::exit(1);
    }
}
