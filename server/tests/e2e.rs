//! End to end: the real server on a random local port, driven by the real
//! client and real encryption (with cheap key-derivation settings).

use focustrack_server::{app, serve, store, Config};
use focustrack_sync_proto::api::NewDevice;
use focustrack_sync_proto::client::{self, Client};
use focustrack_sync_proto::crypto::{AccountKey, KdfParams};
use focustrack_sync_proto::{hlc, kind, new_id, Envelope};
use serde_json::{json, Value};

/// Starts a server with an in-memory database; returns its base URL.
fn start() -> String {
    start_with(Config::default())
}

fn start_with(config: Config) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            let conn = store::open(":memory:").unwrap();
            serve(listener, app(conn, config), std::future::pending())
                .await
                .unwrap();
        });
    });
    format!("http://{}", rx.recv().unwrap())
}

fn device(platform: &str) -> NewDevice {
    NewDevice {
        id: new_id(),
        platform: platform.into(),
        app_version: "1.2.0".into(),
    }
}

struct Dev {
    client: Client,
    key: AccountKey,
    id: String,
    refresh: String,
}

fn signup(base: &str, email: &str, pw: &str) -> Dev {
    let mut c = Client::new(base);
    let (s, key) = client::signup(
        &mut c,
        email,
        pw,
        KdfParams::insecure_for_tests(),
        device("windows"),
    )
    .unwrap();
    Dev {
        client: c,
        key,
        id: s.device_id,
        refresh: s.refresh_token,
    }
}

fn signin(base: &str, email: &str, pw: &str, platform: &str) -> Dev {
    let mut c = Client::new(base);
    let (s, key) = client::signin(&mut c, email, pw, device(platform)).unwrap();
    Dev {
        client: c,
        key,
        id: s.device_id,
        refresh: s.refresh_token,
    }
}

fn usage(d: &Dev, id: &str, version: i64, day: &str, secs: i64) -> Envelope {
    Envelope::seal(
        &d.key,
        id,
        kind::USAGE,
        &d.id,
        day,
        version,
        None,
        false,
        &json!({ "app": "win:code.exe", "secs": secs }),
    )
}

fn limit(d: &Dev, id: &str, hlc: &str, mins: i64) -> Envelope {
    Envelope::seal(
        &d.key,
        id,
        kind::LIMIT,
        &d.id,
        "2026-11-19",
        1,
        Some(hlc.into()),
        false,
        &json!({ "target": "site:youtube.com", "mins": mins }),
    )
}

/// Everything `d` can pull, decrypted.
fn pull_all(d: &Dev) -> Vec<(Envelope, Value)> {
    let mut out = Vec::new();
    let mut since = 0;
    loop {
        let r = d.client.pull(since, 2, None).unwrap();
        for e in r.records {
            let body: Value = e
                .open(&d.key)
                .expect("decrypts with the shared account key");
            out.push((e, body));
        }
        since = r.cursor;
        if !r.more {
            break;
        }
    }
    out
}

#[test]
fn second_device_signs_in_with_the_password_and_reads_the_first_devices_usage() {
    let base = start();
    let pc = signup(&base, "Sam@Example.com", "correct horse battery");
    let r = pc
        .client
        .push(&[
            usage(&pc, &new_id(), 1, "2026-11-19", 600),
            usage(&pc, &new_id(), 1, "2026-11-19", 300),
            usage(&pc, &new_id(), 1, "2026-11-19", 120),
        ])
        .unwrap();
    assert_eq!((r.acked.len(), r.rejected.len()), (3, 0));

    // Email is case-insensitive; the account key arrives through the password.
    let phone = signin(&base, "sam@example.com", "correct horse battery", "android");
    assert_eq!(phone.key.bytes(), pc.key.bytes());
    let got = pull_all(&phone);
    assert_eq!(
        got.iter()
            .map(|(_, b)| b["secs"].as_i64().unwrap())
            .sum::<i64>(),
        1020
    );
    // A device never gets its own records back.
    assert!(pull_all(&pc).is_empty());

    // Wrong password: rejected, same message as an unknown email.
    let mut c = Client::new(&base);
    let e = client::signin(&mut c, "sam@example.com", "wrong", device("windows"))
        .err()
        .unwrap();
    assert_eq!(e.status(), Some(401));
    let e = client::signin(&mut c, "nobody@example.com", "x", device("windows"))
        .err()
        .unwrap();
    assert_eq!(e.status(), Some(401));

    // Devices list.
    let devs = phone.client.devices().unwrap();
    assert_eq!(devs.len(), 2);
    assert!(devs.iter().any(|d| d.current && d.platform == "android"));
}

#[test]
fn pushing_twice_or_a_stale_version_never_duplicates() {
    let base = start();
    let pc = signup(&base, "a@example.com", "pw-one-two-three");
    let phone = signin(&base, "a@example.com", "pw-one-two-three", "android");
    let id = new_id();
    // An open span grows: versions 1, 2, 3. Re-sends and out-of-order sends are harmless.
    for v in [1, 2, 2, 3, 1] {
        let r = pc
            .client
            .push(&[usage(&pc, &id, v, "2026-11-19", v * 100)])
            .unwrap();
        assert_eq!(r.acked[0].version, v, "every push is acknowledged");
    }
    let got = pull_all(&phone);
    assert_eq!(got.len(), 1, "one record, not five");
    assert_eq!(got[0].1["secs"], 300, "the highest version wins");
}

#[test]
fn devices_cant_write_each_others_records() {
    let base = start();
    let pc = signup(&base, "b@example.com", "pw-one-two-three");
    let phone = signin(&base, "b@example.com", "pw-one-two-three", "android");
    let id = new_id();
    pc.client
        .push(&[usage(&pc, &id, 1, "2026-11-19", 100)])
        .unwrap();

    // Claiming to be the PC.
    let mut forged = usage(&pc, &new_id(), 1, "2026-11-19", 1);
    forged.device = pc.id.clone();
    let r = phone.client.push(&[forged]).unwrap();
    assert_eq!(r.rejected.len(), 1);
    // Overwriting the PC's record under its own name.
    let r = phone
        .client
        .push(&[usage(&phone, &id, 9, "2026-11-19", 1)])
        .unwrap();
    assert_eq!(r.rejected[0].reason, "record belongs to another device");
}

#[test]
fn shared_settings_newest_edit_wins_whatever_order_they_arrive() {
    let base = start();
    let pc = signup(&base, "c@example.com", "pw-one-two-three");
    let phone = signin(&base, "c@example.com", "pw-one-two-three", "android");
    let id = new_id();
    let mut pc_clock = hlc::Clock::new(&pc.id);
    let mut phone_clock = hlc::Clock::new(&phone.id);
    let older = pc_clock.tick(1_000).to_string();
    let newer = phone_clock.tick(2_000).to_string();

    // The newer edit (phone) reaches the server first, the older (PC) later.
    phone
        .client
        .push(&[limit(&phone, &id, &newer, 30)])
        .unwrap();
    pc.client.push(&[limit(&pc, &id, &older, 90)]).unwrap();

    let on_pc = pull_all(&pc);
    assert_eq!(on_pc.len(), 1);
    assert_eq!(on_pc[0].1["mins"], 30, "the phone's newer edit stands");
    assert!(
        pull_all(&phone).is_empty(),
        "nothing newer than the phone's own edit"
    );
}

#[test]
fn pull_pages_and_resumes_from_the_cursor() {
    let base = start();
    let pc = signup(&base, "d@example.com", "pw-one-two-three");
    let phone = signin(&base, "d@example.com", "pw-one-two-three", "android");
    let batch: Vec<Envelope> = (0..7)
        .map(|i| usage(&pc, &new_id(), 1, "2026-11-19", i))
        .collect();
    pc.client.push(&batch).unwrap();

    let first = phone.client.pull(0, 3, None).unwrap();
    assert_eq!((first.records.len(), first.more), (3, true));
    let rest = pull_all(&phone);
    assert_eq!(rest.len(), 7);
    // Nothing new: the cursor stays put and nothing comes back.
    let r = phone.client.pull(first.cursor, 100, None).unwrap();
    assert_eq!(r.records.len(), 4);
    let r2 = phone.client.pull(r.cursor, 100, None).unwrap();
    assert!(r2.records.is_empty());
    assert_eq!(r2.cursor, r.cursor);
}

#[test]
fn first_sync_skips_old_detail_but_keeps_settings() {
    let base = start();
    let pc = signup(&base, "e@example.com", "pw-one-two-three");
    let old = day_string(chrono_today() - 100);
    let mut clock = hlc::Clock::new(&pc.id);
    let old_limit = Envelope::seal(
        &pc.key,
        &new_id(),
        kind::LIMIT,
        &pc.id,
        &old,
        1,
        Some(clock.tick(1).to_string()),
        false,
        &json!({ "mins": 30 }),
    );
    pc.client
        .push(&[
            usage(&pc, &new_id(), 1, &old, 50),
            usage(&pc, &new_id(), 1, &chrono_today_str(), 60),
            old_limit,
        ])
        .unwrap();
    let phone = signin(&base, "e@example.com", "pw-one-two-three", "android");
    let r = phone.client.pull(0, 100, Some(35)).unwrap();
    let kinds: Vec<&str> = r.records.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds.len(),
        2,
        "recent usage + the old limit, not the 100-day-old usage: {kinds:?}"
    );
    assert!(kinds.contains(&"limit"));
}

#[test]
fn removed_device_is_locked_out_immediately() {
    let base = start();
    let pc = signup(&base, "f@example.com", "pw-one-two-three");
    let lost = signin(&base, "f@example.com", "pw-one-two-three", "android");
    pc.client.remove_device(&lost.id).unwrap();

    assert_eq!(
        lost.client.pull(0, 10, None).err().unwrap().status(),
        Some(401)
    );
    assert_eq!(lost.client.push(&[]).err().unwrap().status(), Some(401));
    let refreshed = Client::new(&base).refresh(&focustrack_sync_proto::api::RefreshRequest {
        device_id: lost.id.clone(),
        refresh_token: lost.refresh.clone(),
    });
    assert_eq!(refreshed.err().unwrap().status(), Some(401));
    assert_eq!(pc.client.devices().unwrap().len(), 1);
}

#[test]
fn refresh_token_gives_a_new_access_token() {
    let base = start();
    let pc = signup(&base, "g@example.com", "pw-one-two-three");
    let mut c = Client::new(&base);
    let t = c
        .refresh(&focustrack_sync_proto::api::RefreshRequest {
            device_id: pc.id.clone(),
            refresh_token: pc.refresh.clone(),
        })
        .unwrap();
    c.set_access_token(Some(t.access_token));
    assert_eq!(c.devices().unwrap().len(), 1);
    // A made-up refresh token doesn't work.
    let bad = c.refresh(&focustrack_sync_proto::api::RefreshRequest {
        device_id: pc.id.clone(),
        refresh_token: "nope".into(),
    });
    assert_eq!(bad.err().unwrap().status(), Some(401));
}

#[test]
fn changing_the_password_keeps_the_data_readable() {
    let base = start();
    let pc = signup(&base, "h@example.com", "old-password-1");
    pc.client
        .push(&[usage(&pc, &new_id(), 1, "2026-11-19", 42)])
        .unwrap();
    client::change_password(
        &pc.client,
        "h@example.com",
        "old-password-1",
        "new-password-2",
        &pc.key,
    )
    .unwrap();

    let mut c = Client::new(&base);
    assert!(client::signin(&mut c, "h@example.com", "old-password-1", device("windows")).is_err());
    let phone = signin(&base, "h@example.com", "new-password-2", "android");
    assert_eq!(phone.key.bytes(), pc.key.bytes());
    assert_eq!(pull_all(&phone)[0].1["secs"], 42);
}

#[test]
fn deleting_history_and_the_account() {
    let base = start();
    let pc = signup(&base, "i@example.com", "pw-one-two-three");
    let mut clock = hlc::Clock::new(&pc.id);
    pc.client
        .push(&[
            usage(&pc, &new_id(), 1, "2026-10-01", 1),
            usage(&pc, &new_id(), 1, "2026-11-19", 2),
            limit(&pc, &new_id(), &clock.tick(5).to_string(), 30),
        ])
        .unwrap();
    assert_eq!(
        pc.client.delete_data("2026-11-01", None).unwrap().deleted,
        1
    );
    let phone = signin(&base, "i@example.com", "pw-one-two-three", "android");
    assert_eq!(
        pull_all(&phone).len(),
        2,
        "recent usage and the limit remain"
    );

    // Deleting the account needs the password again.
    assert_eq!(
        pc.client.delete_account("wrong").err().unwrap().status(),
        Some(401)
    );
    let params = pc.client.auth_params("i@example.com").unwrap();
    let master = focustrack_sync_proto::crypto::MasterKey::derive(
        "pw-one-two-three",
        &params.salt,
        &params.kdf,
    )
    .unwrap();
    pc.client.delete_account(&master.auth_key()).unwrap();
    let mut c = Client::new(&base);
    assert!(client::signin(
        &mut c,
        "i@example.com",
        "pw-one-two-three",
        device("windows")
    )
    .is_err());
    assert_eq!(
        phone.client.pull(0, 10, None).err().unwrap().status(),
        Some(401)
    );
}

#[test]
fn retention_removes_old_history_and_keeps_settings() {
    let base = start();
    let pc = signup(&base, "r@example.com", "pw-one-two-three");
    assert_eq!(
        pc.client.account().unwrap().retention_days,
        None,
        "forever by default"
    );
    let old = day_string(chrono_today() - 100);
    let recent = day_string(chrono_today() - 10);
    let mut clock = hlc::Clock::new(&pc.id);
    pc.client
        .push(&[
            usage(&pc, &new_id(), 1, &old, 1),
            usage(&pc, &new_id(), 1, &recent, 2),
            Envelope::seal(
                &pc.key,
                &new_id(),
                kind::LIMIT,
                &pc.id,
                &old,
                1,
                Some(clock.tick(1).to_string()),
                false,
                &json!({}),
            ),
        ])
        .unwrap();
    // Only the offered periods are accepted.
    assert_eq!(
        pc.client.set_retention(Some(12)).err().unwrap().status(),
        Some(400)
    );
    assert_eq!(
        pc.client.set_retention(Some(90)).unwrap().retention_days,
        Some(90)
    );
    let phone = signin(&base, "r@example.com", "pw-one-two-three", "android");
    let kinds: Vec<String> = pull_all(&phone)
        .into_iter()
        .map(|(e, _)| format!("{}:{}", e.kind, e.day))
        .collect();
    assert_eq!(
        kinds.len(),
        2,
        "the 100-day-old usage is gone, the limit stays: {kinds:?}"
    );
    assert!(kinds.iter().any(|k| k.starts_with("limit:")));
    // Back to forever.
    assert_eq!(pc.client.set_retention(None).unwrap().retention_days, None);
}

#[test]
fn unknown_emails_get_stable_fake_parameters() {
    let base = start();
    let c = Client::new(&base);
    let a = c.auth_params("ghost@example.com").unwrap();
    assert_eq!(a, c.auth_params("ghost@example.com").unwrap());
    assert_ne!(a.salt, c.auth_params("other@example.com").unwrap().salt);
}

#[test]
fn duplicate_signup_and_weak_settings_are_refused() {
    let base = start();
    signup(&base, "j@example.com", "pw-one-two-three");
    let mut c = Client::new(&base);
    let e = client::signup(
        &mut c,
        "J@example.com",
        "x",
        KdfParams::insecure_for_tests(),
        device("windows"),
    )
    .err()
    .unwrap();
    assert_eq!(e.status(), Some(409));
    let e = client::signup(
        &mut c,
        "k@example.com",
        "x",
        KdfParams {
            m_kib: 8,
            t: 1,
            p: 1,
        },
        device("windows"),
    )
    .err()
    .unwrap();
    assert_eq!(e.status(), Some(400));
}

#[test]
fn repeated_wrong_passwords_lock_the_account_for_a_while() {
    let base = start();
    signup(&base, "l@example.com", "pw-one-two-three");
    let mut c = Client::new(&base);
    let mut statuses = Vec::new();
    for _ in 0..6 {
        statuses.push(
            client::signin(&mut c, "l@example.com", "wrong", device("windows"))
                .err()
                .unwrap()
                .status(),
        );
    }
    assert_eq!(statuses[..5], [Some(401); 5]);
    assert_eq!(statuses[5], Some(429), "locked after five wrong passwords");
    // Even the right password waits until the lock expires.
    let e = client::signin(
        &mut c,
        "l@example.com",
        "pw-one-two-three",
        device("windows"),
    )
    .err()
    .unwrap();
    assert_eq!(e.status(), Some(429));
}

fn chrono_today() -> i64 {
    // Days since the Unix epoch, for building dates relative to today.
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400) as i64
}

fn chrono_today_str() -> String {
    day_string(chrono_today())
}

fn day_string(days: i64) -> String {
    // Civil-from-days (Howard Hinnant), to avoid a chrono dependency in the test.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

#[test]
fn an_account_cant_store_more_than_its_quota() {
    let base = start_with(Config {
        quota_bytes: 1000,
        ..Config::default()
    });
    let pc = signup(&base, "q@example.com", "pw-one-two-three");
    let mut stored = 0;
    let mut refused = 0;
    for i in 0..20 {
        let r = pc
            .client
            .push(&[usage(&pc, &new_id(), 1, "2026-11-19", i)])
            .unwrap();
        stored += r.acked.len();
        if let Some(x) = r.rejected.first() {
            assert_eq!(x.reason, "storage quota reached");
            refused += 1;
        }
    }
    assert!(
        stored > 0 && refused > 0,
        "stored {stored}, refused {refused}"
    );
    // Updating a stored record to the same size still works when full.
    let got = pull_all(&signin(
        &base,
        "q@example.com",
        "pw-one-two-three",
        "android",
    ));
    assert_eq!(got.len(), stored);
}

#[test]
fn sign_ups_stop_at_the_account_limit() {
    let base = start_with(Config {
        max_accounts: 2,
        ..Config::default()
    });
    signup(&base, "m1@example.com", "pw-one-two-three");
    signup(&base, "m2@example.com", "pw-one-two-three");
    let mut c = Client::new(&base);
    let e = client::signup(
        &mut c,
        "m3@example.com",
        "pw-one-two-three",
        KdfParams::insecure_for_tests(),
        device("windows"),
    )
    .err()
    .unwrap();
    assert_eq!(e.status(), Some(503));
    // Existing accounts still sign in.
    signin(&base, "m1@example.com", "pw-one-two-three", "android");
}

/// Against a deployed server (after `deploy/deploy.sh`):
/// `FT_LIVE_URL=https://sync.140-238-91-202.sslip.io cargo test --test e2e live -- --ignored`
/// Signs up a throwaway account, syncs between two devices, then deletes it.
#[test]
#[ignore]
fn live_server_round_trip() {
    let base = std::env::var("FT_LIVE_URL").expect("set FT_LIVE_URL");
    let email = format!("live-check-{}@example.com", &new_id()[..8]);
    let pw = "live-check-password-123";
    let pc = signup(&base, &email, pw);
    let phone = signin(&base, &email, pw, "android");
    let id = new_id();
    let r = pc
        .client
        .push(&[usage(&pc, &id, 1, "2026-11-23", 42)])
        .unwrap();
    assert_eq!(r.acked.len(), 1);
    let got = pull_all(&phone);
    assert_eq!(got.len(), 1, "the phone reads the PC's record");
    assert_eq!(got[0].1["secs"], 42, "and decrypts it");
    let params = pc.client.auth_params(&email).unwrap();
    let master =
        focustrack_sync_proto::crypto::MasterKey::derive(pw, &params.salt, &params.kdf).unwrap();
    pc.client.delete_account(&master.auth_key()).unwrap();
}
