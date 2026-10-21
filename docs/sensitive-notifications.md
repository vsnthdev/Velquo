# Fixing "Sensitive notification content hidden" on Android 15+

Starting with **Android 15**, Android automatically redacts notifications containing 2FA/OTP codes or private messages from third-party notification listeners, replacing the text with:

> `Sensitive notification content hidden`

Because **Velquo** receives notifications from the **KDE Connect** app on your Android phone, you can grant KDE Connect a one-time system permission via **ADB** so your Mac receives the full notification text and OTP codes.

Once granted, you can immediately turn **USB debugging** and **Developer options** back off — the permission stays active permanently.

---

## Step 1: Install ADB on Your Mac

If you don't already have `adb` installed on your Mac, install it with Homebrew:

```bash
brew install --cask android-platform-tools
```

---

## Step 2: Enable USB Debugging on Your Phone

1. Open **Settings** on your Android phone and go to **About device** (or **About phone**).
2. Tap **Version** (if on OnePlus / OxygenOS) → tap **Build number** **7 times** until it says *"You are now in Developer mode!"*.
3. Go back to **Settings** → **System & update** (or **Additional settings** / **System**) → **Developer options**.
4. Turn on **USB debugging**.
   - **Important for OnePlus / Oppo / Realme (OxygenOS / ColorOS):** Scroll down inside **Developer options** and also turn on **"Disable permission monitoring"** (otherwise OxygenOS blocks `appops` commands over ADB).

---

## Step 3: Run the Permission Command from Your Mac

1. Connect your phone to your Mac with a USB-C cable.
2. On your phone screen, tap **Allow** on the *"Allow USB debugging?"* prompt.
3. Run the following command in your Mac terminal:

```bash
adb shell appops set org.kde.kdeconnect_tp RECEIVE_SENSITIVE_NOTIFICATIONS allow
```

*(If you use the F-Droid nightly build of KDE Connect, replace `org.kde.kdeconnect_tp` with your installed package name).*

You can verify the permission was granted by running:

```bash
adb shell appops get org.kde.kdeconnect_tp RECEIVE_SENSITIVE_NOTIFICATIONS
```

It should output: `RECEIVE_SENSITIVE_NOTIFICATIONS: allow`.

---

## Step 4: Turn Off USB Debugging & Developer Options

1. Unplug your phone from your Mac.
2. On your phone, go back to **Settings** → **Developer options**.
3. Turn off **USB debugging** (or toggle off **Developer options** entirely).

The `RECEIVE_SENSITIVE_NOTIFICATIONS` permission will remain active, and **Velquo** will now display full notification messages and auto-copy OTP codes on your Mac!
