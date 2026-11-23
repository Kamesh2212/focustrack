//! `focustrack-server` — run the sync server.
//!
//! Environment:
//!   FT_SYNC_ADDR         listen address (default 127.0.0.1:8787)
//!   FT_SYNC_DB           SQLite file (default ./focustrack-sync.db)
//!   FT_SYNC_TRUST_PROXY  1 when behind a reverse proxy such as Caddy
//!   FT_SYNC_QUOTA_MB     most data one account may store (default 500)
//!   FT_SYNC_MAX_ACCOUNTS sign-ups stop at this many accounts (default 1000)
//!
//! `focustrack-server backup <file>` writes a consistent copy of the database
//! (safe while the server runs) and exits.

use focustrack_server::{app, serve, store, Config};

#[tokio::main]
async fn main() {
    let addr = std::env::var("FT_SYNC_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let db_path = std::env::var("FT_SYNC_DB").unwrap_or_else(|_| "focustrack-sync.db".into());
    let trust_proxy = std::env::var("FT_SYNC_TRUST_PROXY").is_ok_and(|v| v == "1");
    let num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<i64>().ok());
    let defaults = Config::default();
    let config = Config {
        trust_proxy,
        quota_bytes: num("FT_SYNC_QUOTA_MB").map_or(defaults.quota_bytes, |mb| mb * 1024 * 1024),
        max_accounts: num("FT_SYNC_MAX_ACCOUNTS").unwrap_or(defaults.max_accounts),
    };

    let conn = match store::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("can't open database {db_path}: {e}");
            std::process::exit(1);
        }
    };
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("backup") {
        let Some(to) = args.get(2) else {
            eprintln!("usage: focustrack-server backup <file>");
            std::process::exit(2);
        };
        match conn.execute("VACUUM INTO ?1", [to]) {
            Ok(_) => println!("backed up {db_path} to {to}"),
            Err(e) => {
                eprintln!("backup failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
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
    if let Err(e) = serve(listener, app(conn, config), shutdown).await {
        eprintln!("server error: {e}");
        std::process::exit(1);
    }
}
