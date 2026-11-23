# Setting up sync on your phone and computers (preview)

FocusTrack syncs through its sync server at **https://sync.140-238-91-202.sslip.io**, which the apps use by
default. Your devices encrypt everything before uploading, so the server only ever stores scrambled data. (How
the server is run: [DEPLOY.md](DEPLOY.md).)

## 1. Desktop

In **Settings → Account & Sync**:

- **Create account** (email + a password of at least 10 characters).
- **Save the recovery key** it shows. Without it, a forgotten password means synced history can't be read.

On your other computers, **Sign in** with the same email and password.

## 2. Android phone

1. Copy the Android APK to the phone and open it. Android asks to allow installing from this source (the APK
   isn't from the Play Store); allow it once.
2. Open FocusTrack and tap **Allow** → turn on **Usage access** for FocusTrack → go back.
3. Menu → **Settings → Account & Sync → Sign in** with the same email and password.
4. Optional: **Settings → App blocking on this phone → Turn on** to enforce limits and focus sessions on the phone.

The phone imports screen time when you open FocusTrack and about every 15 minutes in the background, then syncs.
Your computers show it on the dashboard (device filter, **Devices** section) within a minute or two.

## Running your own server instead

Start `focustrack-server` on a computer (`$env:FT_SYNC_ADDR = "0.0.0.0:8787"; .\focustrack-server.exe`; allow it on
**Private networks** if Windows Firewall asks), then on each device open **Server** in the sign-in form and enter
`http://127.0.0.1:8787` on that computer or `http://<its home-network address>:8787` (e.g. `http://192.168.1.20:8787`)
elsewhere. Plain http is only allowed on your own computer and home network; anything else needs https.

## Good to know

- The Android build is signed with a development key: to update it, uninstall the old one first if Android refuses.
- To start over: **Settings → Account & Sync → Delete account** (needs your password), or sign out on each device.
