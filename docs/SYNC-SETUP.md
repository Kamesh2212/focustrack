# Testing sync on your phone and computer (1.2.0 preview)

Sync needs a FocusTrack sync server. Until a hosted one exists, run it on one of your computers. Your devices
encrypt everything before uploading, so the server only ever stores scrambled data.

## 1. Run the server on your PC

1. Download `focustrack-server-1.2.0-windows-x64.exe` from the release into a folder of its own (it keeps its
   database next to it).
2. Open PowerShell in that folder and start it so other devices on your Wi-Fi can reach it:

   ```powershell
   $env:FT_SYNC_ADDR = "0.0.0.0:8787"; .\focustrack-server-1.2.0-windows-x64.exe
   ```

3. If Windows Firewall asks, allow it on **Private networks** only.
4. Find the PC's home-network address: `ipconfig` → **IPv4 Address**, e.g. `192.168.1.20`.

Leave the window open while testing. The server's data is in `focustrack-sync.db` next to it.

## 2. Desktop

Install FocusTrack 1.2.0 (the `-setup.exe`). In **Settings → Account & Sync**:

- **Create account** (email + a password of at least 10 characters). Open **Server** and enter
  `http://127.0.0.1:8787` on the PC running the server, or `http://192.168.1.20:8787` (your address) on other PCs.
- **Save the recovery key** it shows. Without it, a forgotten password means synced history can't be read.

## 3. Android phone

1. Copy `FocusTrack-1.2.0-android-arm64.apk` to the phone and open it. Android asks to allow installing from this
   source (the APK isn't from the Play Store); allow it once.
2. Open FocusTrack and tap **Allow** → turn on **Usage access** for FocusTrack → go back.
3. Menu → **Settings → Account & Sync → Sign in** with the same email and password; under **Server** enter
   `http://<your PC's address>:8787`. The phone must be on the same Wi-Fi as the PC.

The phone imports screen time when you open FocusTrack and about every 15 minutes in the background, then syncs.
Your PC shows it on the dashboard (device filter, **Devices** section) within a minute or two of a sync.

## Good to know

- On your home network the apps talk to the server over plain http. Your usage data is end-to-end encrypted either
  way, but sign-in tokens travel unencrypted on your Wi-Fi. Use this setup only at home; the hosted service will
  use https.
- The Android build is signed with a development key: to update it, uninstall the old one first if Android refuses.
- Phone app blocking isn't available yet; limits sync to the phone but are only enforced on Windows.
- To start over: sign out on each device, stop the server, delete `focustrack-sync.db`.
