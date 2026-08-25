# MOBILE

**Status: the Android project builds from this repository.** It runs the same
React bundle and the same Rust crates as the Windows desktop — one repository,
one database engine, one sync client, two shells.

## Why Tauri and not a second application

The alternative was a separate mobile app talking to the server. It was rejected
for a reason that has nothing to do with taste: the parts of this product that
must not be wrong — the merge, conflict detection, tombstones, deferral, the
search index — would then exist twice, in two languages, kept in step by hand.
They would drift, and the symptom of drift is a contact that looks different
depending on which device you ask, with no single place to go and read what is
correct.

With Tauri, `yanuka-db` is compiled for `aarch64-linux-android` from the same
source that Windows uses. A bug fixed in the merge is fixed on the phone in the
same commit.

## What differs between the two shells

Almost nothing, and deliberately so.

* **Layout.** One breakpoint, at `md`. Above it: the right-hand rail. Below it:
  navigation moves to the bottom of the screen, where a thumb reaches, laid out
  as the last row of the column rather than floating over the content. See
  `app-layout.tsx`.
* **The conflict signal.** The rail can afford to show the sync indicator
  permanently; a bottom bar cannot. Instead a mark appears on הגדרות when a
  decision is waiting — a conflict visible only on a screen nobody visits is a
  conflict nobody answers.
* **Storage.** The database lives in the app's private data directory, which on
  Android is per-application and removed with the app. That is worth knowing
  before treating a phone as the only copy of anything: it is a replica, not the
  archive.

Everything else — search, the contact form, merging, backup — is the same code.

## Building

Prerequisites, once:

```bash
# JDK 17 or newer, then the Android SDK and NDK.
export ANDROID_HOME="$HOME/Android/Sdk"
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
sdkmanager --install "platform-tools" "platforms;android-34" \
                     "build-tools;34.0.0" "ndk;27.0.12077973"

rustup target add aarch64-linux-android armv7-linux-androideabi \
                  i686-linux-android x86_64-linux-android
```

Then, from `apps/desktop`:

```bash
pnpm tauri android dev            # onto a connected device or emulator
pnpm tauri android build --debug  # an installable APK, unsigned
pnpm tauri android build          # release; needs a signing key, see below
```

The output lands in
`src-tauri/gen/android/app/build/outputs/apk/`.

`src-tauri/gen/android` is committed. It is generated once by
`tauri android init` and then edited — the Hebrew launcher name, `supportsRtl`,
the icons — so re-running `init` on a fresh clone would silently discard all of
it. Do not run `init` again; edit the project in place.

## Signing a release build

Android will not install an unsigned release APK. Generate a keystore once and
keep it somewhere that is backed up — losing it means the next version cannot
be installed as an update over this one, only as a separate application:

```bash
keytool -genkey -v -keystore yanuka.jks -keyalg RSA -keysize 2048 \
        -validity 10000 -alias yanuka
```

Then `src-tauri/gen/android/keystore.properties` (already gitignored):

```properties
storeFile=/absolute/path/to/yanuka.jks
keyAlias=yanuka
storePassword=…
password=…
```

## Sync on a phone

A phone syncs on a timer, not on a button — nobody opens a settings screen to
press one. The interval is five minutes while the server is reachable, backing
off to at most thirty while it is not, and returning to five the moment one
round succeeds. See `schedule.rs` and ADR-038.

Everything else about connecting a phone is what `docs/DEPLOY.md` describes for
a second computer: paste the connection code from the machine that already has
one.
