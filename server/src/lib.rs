//! FocusTrack sync server (design §6, §9).
//!
//! An authenticated, ordered store of end-to-end-encrypted records. It can't
//! read usage data: it checks who may write what, keeps the newest version of
//! each record, and hands devices what changed since they last asked.

pub mod store;

use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use focustrack_sync_proto::api::*;
use focustrack_sync_proto::crypto::KdfParams;
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

/// Sign-in/sign-up requests allowed per IP per minute.
const AUTH_PER_MINUTE: u32 = 20;

#[derive(Clone, Debug)]
pub struct Config {
    /// Behind a reverse proxy (Caddy): take the client IP from X-Forwarded-For.
    pub trust_proxy: bool,
}

pub struct AppState {
    db: Mutex<Connection>,
    secret: String,
    config: Config,
    limiter: Mutex<HashMap<IpAddr, (i64, u32)>>,
}

type Shared = Arc<AppState>;

#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl ApiError {
    fn new(status: StatusCode, msg: impl Into<String>) -> ApiError {
        ApiError(status, msg.into())
    }
    fn bad(msg: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, msg)
    }
    fn unauthorized() -> ApiError {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "Not signed in, or this device was removed",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorBody { error: self.1 })).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

/// Runs a database operation off the async threads.
async fn db<T: Send + 'static>(
    st: &Shared,
    f: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    let st = st.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = st.db.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut conn)
    })
    .await
    .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
    .map_err(|e| {
        eprintln!("[server] database error: {e}");
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    })
}

/// The signed-in (account, device) for a request.
async fn auth(st: &Shared, headers: &HeaderMap) -> Result<(String, String), ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
        .ok_or_else(ApiError::unauthorized)?;
    db(st, move |c| store::authenticate(c, &token))
        .await?
        .ok_or_else(ApiError::unauthorized)
}

fn client_ip(st: &Shared, headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
    if st.config.trust_proxy {
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .and_then(|v| v.trim().parse().ok())
        {
            return ip;
        }
    }
    peer.ip()
}

fn rate_limit(st: &Shared, ip: IpAddr) -> Result<(), ApiError> {
    let minute = store::now() / 60;
    let mut map = st.limiter.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() > 100_000 {
        map.retain(|_, (m, _)| *m == minute);
    }
    let entry = map.entry(ip).or_insert((minute, 0));
    if entry.0 != minute {
        *entry = (minute, 0);
    }
    entry.1 += 1;
    if entry.1 > AUTH_PER_MINUTE {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts. Wait a minute and try again.",
        ));
    }
    Ok(())
}

fn normalise_email(email: &str) -> Result<String, ApiError> {
    let e = email.trim().to_ascii_lowercase();
    let ok = e.len() <= 254
        && e.split_once('@').is_some_and(|(user, host)| {
            !user.is_empty() && host.contains('.') && !host.starts_with('.')
        });
    if ok {
        Ok(e)
    } else {
        Err(ApiError::bad("Enter a valid email address"))
    }
}

fn check_device(d: &NewDevice) -> Result<(), ApiError> {
    uuid_ok(&d.id)?;
    if !matches!(
        d.platform.as_str(),
        "windows" | "android" | "ios" | "macos" | "linux"
    ) {
        return Err(ApiError::bad("unknown platform"));
    }
    if d.app_version.len() > 40 {
        return Err(ApiError::bad("bad app version"));
    }
    Ok(())
}

fn uuid_ok(s: &str) -> Result<(), ApiError> {
    let ok = s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad("bad id"))
    }
}

fn check_secret_field(name: &str, v: &str, max: usize) -> Result<(), ApiError> {
    if v.is_empty() || v.len() > max {
        return Err(ApiError::bad(format!("bad {name}")));
    }
    Ok(())
}

async fn status() -> Json<Status> {
    Json(Status {
        app: "focustrack-sync".into(),
        protocol: focustrack_sync_proto::PROTOCOL,
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

#[derive(Deserialize)]
struct EmailQuery {
    email: String,
}

async fn auth_params(
    State(st): State<Shared>,
    Query(q): Query<EmailQuery>,
) -> ApiResult<AuthParams> {
    let email = normalise_email(&q.email)?;
    let secret = st.secret.clone();
    let real = db(&st, {
        let email = email.clone();
        move |c| store::account_by_email(c, &email)
    })
    .await?;
    Ok(Json(match real {
        Some(a) => AuthParams {
            salt: a.salt,
            kdf: a.kdf,
        },
        // Unknown email: a stable, fake answer so accounts can't be discovered.
        None => {
            use base64::Engine;
            let h = store::sha256_hex(&format!("{secret}:{email}"));
            AuthParams {
                salt: base64::engine::general_purpose::STANDARD.encode(&h.as_bytes()[..16]),
                kdf: KdfParams::standard(),
            }
        }
    }))
}

fn session(
    account: String,
    device: String,
    reg: store::DeviceRegistration,
    wrapped_key: String,
) -> ApiResult<Session> {
    match reg {
        store::DeviceRegistration::Ok(refresh, access) => Ok(Json(Session {
            account_id: account,
            device_id: device,
            access_token: access,
            access_expires_in: store::ACCESS_TTL_SECS as u64,
            refresh_token: refresh,
            wrapped_key,
        })),
        store::DeviceRegistration::Taken => Err(ApiError::new(
            StatusCode::CONFLICT,
            "This device is registered to another account",
        )),
    }
}

async fn signup(
    State(st): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SignupRequest>,
) -> ApiResult<Session> {
    rate_limit(&st, client_ip(&st, &headers, peer))?;
    let email = normalise_email(&req.email)?;
    check_device(&req.device)?;
    check_secret_field("auth key", &req.auth_key, 100)?;
    check_secret_field("salt", &req.salt, 100)?;
    check_secret_field("wrapped key", &req.wrapped_key, 200)?;
    if !req.kdf.is_acceptable() {
        return Err(ApiError::bad("key derivation settings are too weak"));
    }
    let wrapped = req.wrapped_key.clone();
    let device_id = req.device.id.clone();
    let (account, reg) = db(&st, move |c| {
        let tx = c.transaction()?;
        let Some(account) = store::create_account(
            &tx,
            &email,
            &req.auth_key,
            &req.salt,
            &req.kdf,
            &req.wrapped_key,
        )?
        else {
            return Ok(None);
        };
        let reg = store::register_device(
            &tx,
            &account,
            &req.device.id,
            &req.device.platform,
            &req.device.app_version,
        )?;
        if matches!(reg, store::DeviceRegistration::Taken) {
            return Ok(Some((account, reg))); // dropped transaction rolls back
        }
        tx.commit()?;
        Ok(Some((account, reg)))
    })
    .await?
    .ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            "An account with this email already exists",
        )
    })?;
    session(account, device_id, reg, wrapped)
}

async fn signin(
    State(st): State<Shared>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SigninRequest>,
) -> ApiResult<Session> {
    rate_limit(&st, client_ip(&st, &headers, peer))?;
    let email = normalise_email(&req.email)?;
    check_device(&req.device)?;
    check_secret_field("auth key", &req.auth_key, 100)?;
    let device = req.device.clone();
    let result = db(&st, move |c| {
        let Some(acc) = store::account_by_email(c, &email)? else {
            return Ok(Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "Wrong email or password",
            )));
        };
        if acc.locked_until > store::now() {
            let mins = (acc.locked_until - store::now() + 59) / 60;
            return Ok(Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                format!("Too many wrong passwords. Try again in {mins} minute(s)."),
            )));
        }
        if !store::same(&acc.auth_hash, &store::sha256_hex(&req.auth_key)) {
            store::login_failed(c, &acc.id)?;
            return Ok(Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "Wrong email or password",
            )));
        }
        store::login_ok(c, &acc.id)?;
        let reg = store::register_device(
            c,
            &acc.id,
            &device.id,
            &device.platform,
            &device.app_version,
        )?;
        Ok(Ok((acc.id, acc.wrapped_key, reg)))
    })
    .await??;
    let (account, wrapped, reg) = result;
    session(account, req.device.id, reg, wrapped)
}

async fn refresh(
    State(st): State<Shared>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<AccessToken> {
    uuid_ok(&req.device_id)?;
    let token = db(&st, move |c| {
        store::refresh(c, &req.device_id, &req.refresh_token)
    })
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    Ok(Json(AccessToken {
        access_token: token,
        access_expires_in: store::ACCESS_TTL_SECS as u64,
    }))
}

async fn change_password(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<serde_json::Value> {
    let (account, _) = auth(&st, &headers).await?;
    check_secret_field("auth key", &req.new_auth_key, 100)?;
    check_secret_field("salt", &req.new_salt, 100)?;
    check_secret_field("wrapped key", &req.new_wrapped_key, 200)?;
    if !req.new_kdf.is_acceptable() {
        return Err(ApiError::bad("key derivation settings are too weak"));
    }
    db(&st, move |c| {
        let Some(acc) = store::account_by_id(c, &account)? else {
            return Ok(Err(ApiError::unauthorized()));
        };
        if !store::same(&acc.auth_hash, &store::sha256_hex(&req.auth_key)) {
            store::login_failed(c, &acc.id)?;
            return Ok(Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "Wrong current password",
            )));
        }
        store::change_password(
            c,
            &account,
            &req.new_auth_key,
            &req.new_salt,
            &req.new_kdf,
            &req.new_wrapped_key,
        )?;
        Ok(Ok(()))
    })
    .await??;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_devices(State(st): State<Shared>, headers: HeaderMap) -> ApiResult<Vec<DeviceInfo>> {
    let (account, device) = auth(&st, &headers).await?;
    Ok(Json(
        db(&st, move |c| store::devices(c, &account, &device)).await?,
    ))
}

async fn remove_device(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let (account, _) = auth(&st, &headers).await?;
    uuid_ok(&id)?;
    if !db(&st, move |c| store::revoke_device(c, &account, &id)).await? {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "No such device"));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn push(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<PushRequest>,
) -> ApiResult<PushResponse> {
    let (account, device) = auth(&st, &headers).await?;
    if req.records.len() > MAX_BATCH {
        return Err(ApiError::bad(format!(
            "at most {MAX_BATCH} records per push"
        )));
    }
    let (acked, rejected) = db(&st, move |c| {
        store::push(c, &account, &device, &req.records)
    })
    .await?;
    Ok(Json(PushResponse { acked, rejected }))
}

#[derive(Deserialize)]
struct PullQuery {
    since: i64,
    limit: Option<usize>,
    days: Option<u32>,
    /// 1 = include the caller's own records (export).
    own: Option<u8>,
}

async fn pull(
    State(st): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<PullQuery>,
) -> ApiResult<PullResponse> {
    let (account, device) = auth(&st, &headers).await?;
    // Normally a device never gets its own records back; export wants everything.
    let device = if q.own == Some(1) {
        String::new()
    } else {
        device
    };
    let limit = q.limit.unwrap_or(MAX_BATCH).clamp(1, MAX_BATCH);
    let min_day = q.days.map(|d| {
        (chrono::Utc::now().date_naive() - chrono::Duration::days(d as i64))
            .format("%Y-%m-%d")
            .to_string()
    });
    let (records, cursor, more) = db(&st, move |c| {
        store::pull(
            c,
            &account,
            &device,
            q.since.max(0),
            limit,
            min_day.as_deref(),
        )
    })
    .await?;
    Ok(Json(PullResponse {
        records,
        cursor,
        more,
    }))
}

#[derive(Deserialize)]
struct DeleteQuery {
    before: String,
    device: Option<String>,
}

async fn delete_data(
    State(st): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<DeleteQuery>,
) -> ApiResult<DeleteDataResponse> {
    let (account, _) = auth(&st, &headers).await?;
    if chrono::NaiveDate::parse_from_str(&q.before, "%Y-%m-%d").is_err() {
        return Err(ApiError::bad("before must be YYYY-MM-DD"));
    }
    if let Some(d) = &q.device {
        uuid_ok(d)?;
    }
    let deleted = db(&st, move |c| {
        store::delete_before(c, &account, &q.before, q.device.as_deref())
    })
    .await?;
    Ok(Json(DeleteDataResponse { deleted }))
}

#[derive(Deserialize)]
struct DeleteAccountRequest {
    auth_key: String,
}

async fn account_info(State(st): State<Shared>, headers: HeaderMap) -> ApiResult<AccountInfo> {
    let (account, _) = auth(&st, &headers).await?;
    let retention_days = db(&st, move |c| store::retention(c, &account)).await?;
    Ok(Json(AccountInfo { retention_days }))
}

async fn set_retention(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<RetentionRequest>,
) -> ApiResult<AccountInfo> {
    let (account, _) = auth(&st, &headers).await?;
    if req.days.is_some_and(|d| !RETENTION_CHOICES.contains(&d)) {
        return Err(ApiError::bad("unsupported retention period"));
    }
    let days = req.days;
    db(&st, move |c| {
        store::set_retention(c, &account, days)?;
        // Apply right away, not only at the next hourly sweep.
        store::apply_retention(c, chrono::Utc::now().date_naive())
    })
    .await?;
    Ok(Json(AccountInfo {
        retention_days: days,
    }))
}

/// Applies retention periods every hour.
fn spawn_retention_sweeper(st: Shared) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match db(&st, |c| {
                store::apply_retention(c, chrono::Utc::now().date_naive())
            })
            .await
            {
                Ok(n) if n > 0 => println!("retention: removed {n} old record(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("retention sweep failed: {:?}", e),
            }
        }
    });
}

async fn delete_account(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<DeleteAccountRequest>,
) -> ApiResult<serde_json::Value> {
    let (account, _) = auth(&st, &headers).await?;
    db(&st, move |c| {
        let Some(acc) = store::account_by_id(c, &account)? else {
            return Ok(Err(ApiError::unauthorized()));
        };
        if !store::same(&acc.auth_hash, &store::sha256_hex(&req.auth_key)) {
            store::login_failed(c, &acc.id)?;
            return Ok(Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "Wrong password",
            )));
        }
        store::delete_account(c, &account)?;
        Ok(Ok(()))
    })
    .await??;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// The HTTP application over an open database.
pub fn app(conn: Connection, config: Config) -> Router {
    let secret = store::server_secret(&conn).expect("server secret");
    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        secret,
        config,
        limiter: Mutex::new(HashMap::new()),
    });
    if tokio::runtime::Handle::try_current().is_ok() {
        spawn_retention_sweeper(state.clone());
    }
    Router::new()
        .route("/v1/status", get(status))
        .route("/v1/auth/params", get(auth_params))
        .route("/v1/auth/signup", post(signup))
        .route("/v1/auth/signin", post(signin))
        .route("/v1/auth/refresh", post(refresh))
        .route("/v1/auth/password", post(change_password))
        .route("/v1/devices", get(list_devices))
        .route("/v1/devices/{id}", delete(remove_device))
        .route("/v1/sync/push", post(push))
        .route("/v1/sync/pull", get(pull))
        .route("/v1/data", delete(delete_data))
        .route("/v1/account", get(account_info).delete(delete_account))
        .route("/v1/account/retention", put(set_retention))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(state)
}

/// Serves until `shutdown` resolves.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: Router,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
}
