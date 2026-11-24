<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/waterline.png">
    <img src="assets/brand/waterline-on-light.png" alt="RustyFin" width="320">
  </picture>
</h1>

RustyFin is a Windows x64 Jellyfin player built with Rust and GPUI. Sign in with Quick Connect or your Jellyfin username and password, browse video, music and live TV, and play the original media source. The client saves the session token in Windows Credential Manager and reports playback progress to Jellyfin.

[Changelog](CHANGELOG.md)

## Run

Install the stable Rust MSVC toolchain, the Visual Studio C++ build tools and a Windows SDK. From the project directory in PowerShell, run

```powershell
cargo build --locked
.\scripts\Get-Mpv.ps1
.\scripts\Get-NativeMedia.ps1
.\target\debug\rustyfin.exe
```

The scripts install libmpv and build the x64 C++/WinRT Media Foundation source against a pinned FFmpeg development package. Both downloads are SHA-256 verified. Keep the `media` and `shaders` directories beside `rustyfin.exe`. For a release build, run `cargo build --locked --release` and pass `-OutputDirectory .\target\release` to both scripts.

Enter the server URL, including any reverse-proxy base path. Choose **Use username and password** to sign in directly. For Quick Connect, choose **Get pairing code** and enter the code in a signed-in Jellyfin client. If the server has disabled Quick Connect, RustyFin opens the password form automatically. [Jellyfin Quick Connect](https://jellyfin.org/docs/general/server/quick-connect/)

## Playback

During mpv video playback, open **Playback Settings → Shaders** to switch presets or turn them **Off**. Changes apply to the current video and the selected preset follows your profile. **Add shader file…** accepts a local mpv `.glsl` or `.hook` file and keeps a reference to that path. Upscalers run when the displayed video exceeds the source size by the amount specified by each shader. The first use of a preset can take time while the GPU driver compiles it. Native Dolby Vision playback uses the Windows renderer.

Start with **FSRCNNX 8** for live action. Try **FSRCNNX 16** on clean footage if your GPU can sustain the frame rate, or **RAVU Lite AR** to limit ringing around edges. **ArtCNN C4F16 DS** denoises and sharpens as it upscales, which can help compressed sources and can soften intentional film grain. For animation, try **ArtCNN C4F16**, **C4F32** or **Anime4K ×2**. [Bundled shader sources and licenses](assets/shaders/README.md)

| Source | Playback path |
| --- | --- |
| SDR, HDR10, HDR10+ or HLG | Embedded libmpv |
| Dolby Vision on a compatible HDR display | Windows Media Foundation with **Use Windows Dolby Vision output** enabled, or mpv rendering with the setting off |
| HDR or Dolby Vision on an active SDR display | Embedded libmpv with SDR tone mapping |
| Profile 7 MEL, with compatibility enabled | Local adaptation to Profile 8.1 and the Windows Dolby renderer |
| Profile 7 FEL | mpv enhancement-layer reconstruction, with HDR10 base-layer fallback |
| Profile 7 MEL with metadata disabled | HDR10 base layer via embedded libmpv |
| Unsupported Dolby profile or unverified Dolby output while HDR is active | Playback stops and the library shows the reason |

For ordinary HDR playback, mpv negotiates the output colour space with the active display. Its video surface sits beneath a transparent GPUI control layer. An HDR10+ source can play in HDR10 on Windows. The format strip identifies the source, while the display’s signal information identifies the output.

When Windows reports SDR on the playback display, RustyFin uses mpv to tone-map HDR and supported Dolby Vision sources to SDR. Profile 7 FEL reconstruction works with tone mapping. A brief player notice identifies the conversion and mentions Windows HDR when the display supports it.

RustyFin requests the original file through Jellyfin Direct Play. For native Dolby Vision playback, Windows provides the seekable input stream. A client-side media source demuxes it and feeds the original compressed video and timestamps to the Windows decoder and Dolby renderer. It checks the file's Dolby configuration, obtains the renderer profile and decoder attributes from the Windows MP4 demuxer using one compressed picture in memory, and verifies the selected Media Engine stream.

For single-track HEVC Profile 7 remuxes, RustyFin inspects the enhancement-layer metadata before playback. mpv reconstructs FEL by default and retries with the HDR10 base layer if reconstruction fails. Windows playback converts MEL to Profile 8.1 when **Use Windows Dolby Vision output** and **Use Dolby Vision metadata for Profile 7 MEL** are enabled. If Windows encounters FEL during that conversion, RustyFin switches to mpv at the current position. [Dolby Vision profiles](https://ott.dolby.com/OnDelKits/Dolby_Vision_Online_Delivery_Kit/v1/Documentation/Specs/Visio_Profiles/help_files/topics/c_dovi_profiles.html) · [mpv FEL support](https://github.com/mpv-player/mpv/pull/17932)

Audio follows the selected Windows multimedia output. For AC-3, E-AC-3, TrueHD, DTS and DTS-HD, the Dolby Vision path checks the selected digital output for the track's encoded format and opens it in WASAPI exclusive mode. **Audio bitstream** indicates that RustyFin has reserved the output for the original encoded audio. An Atmos-capable receiver can render the objects carried in E-AC-3 or TrueHD. For headphones, speakers, an unsupported format or a busy exclusive endpoint, RustyFin decodes the selected track locally to PCM and mixes it to the available channel layout. **PCM 2 ch** indicates stereo. Conventional PCM downmixing discards Atmos object positioning. The video and audio use the original file and presentation timestamps on both paths. The ordinary mpv playback path also enables WASAPI passthrough with PCM fallback.

Windows owns Dolby Vision video decoding, processing and presentation. Dolby and Windows can perform display mapping. The in-video Dolby indicator identifies player-side processing. The display's input information describes the signal it receives. [Microsoft HDR settings](https://support.microsoft.com/en-us/windows/hardware/display-graphics/hdr-settings-in-windows)

Open **Settings → Playback display** in RustyFin to see the monitor name, the Windows HDR preference and the active color mode. The HDR button changes the preference for the displayed monitor. After a playback failure, the retry button reopens the source at its saved position.

The output check follows the monitor containing the playback window. It accepts an active Dolby Vision low-latency wire format, or active PQ HDR on a monitor that explicitly identifies HDR as its special Dolby Vision mode. That second configuration can report HDR10 as its wire format. A standard HDR10 display alone does not qualify. [Windows display wire formats](https://learn.microsoft.com/en-us/uwp/api/windows.devices.display.core.displaywireformathdrmetadata) · [Dolby Vision in HDR mode](https://learn.microsoft.com/en-us/uwp/api/windows.devices.display.displaymonitor.isdolbyvisionsupportedinhdrmode)

## Using RustyFin

- **Top navigation** keeps Home, Favourites, Collections and your Jellyfin libraries within reach while browsing. On smaller windows, additional libraries move into **More**, alongside All libraries, Explore and Live TV. Search, Downloads, Settings and Profiles are on the right.
- **Discover** opens with a featured title. Select a Continue Watching card to play it. Open its **More** menu with the mouse, Shift+F10 or controller X for title details, the series, watched status or **Play from beginning**. Scroll down for Recently Added TV and Recently Added Movies. If a server or feed fails to refresh, Home keeps the titles already loaded and offers a retry.
- **Search** opens with `/` and remembers recent searches for each profile. Select the best match artwork for details, or use its Play or Resume button. Movies and shows use posters, episodes use stills, and people use portraits. Back returns to the same query and selected result. Choose **Clear history** on the empty search screen to remove recent searches.
- **Series details** show the episode to resume or play next, its still and how much time is left. Press Enter to play it. Press Down to browse seasons, Left or Right to switch seasons, and Down again to choose an episode. Browsing another season keeps the main Resume action on your unfinished episode.
- **Movie details** show the edition and format beside Play. Choose an edition when more than one is available, or open **More** for tracks, chapters, downloads, watched status and metadata editing. Select a cast portrait to explore that person's titles, or a trailer to open its details.
- **More like this** shows recommendations from the current Jellyfin server beneath the title details. Select a poster to open it, or use the arrows to see more. Episode pages use their series for recommendations.
- **Playback options** changes tracks while a title is playing, selects a chapter or speed, manages the queue, and sets the sleep timer. The skip button appears during Jellyfin intro or credit segments. When another episode is available, the credits card offers **Play next** and **Watch credits**. Watch credits keeps the current episode playing through the end and cancels autoplay for that episode. Use Settings to enable automatic skipping or fullscreen refresh-rate matching. Refresh matching selects an advertised mode for the playback display at the existing resolution and restores the previous rate when leaving fullscreen. Native Dolby Vision text subtitles occupy a caption area below the video.
- **Music** supports albums, artists, Instant Mix and synchronized lyrics when supplied by Jellyfin. Choose **Back to library** in Now Playing to keep the mini-player active. Windows media controls expose the title, play, pause, stop, restart and queue advance when available.
- **Downloads** saves the original version and selected local-playback subtitle sidecar when needed. Pause or resume transfers here. Completed items play without reaching the server. RustyFin syncs offline progress when the server becomes available. A series page can queue the next three or five unwatched episodes and include specials. Rules run while RustyFin is open.
- **Live TV** opens the channels, favorites, program guide, scheduled recordings and series rules for the selected server. Recording requires a configured Jellyfin DVR and suitable account permissions.
- **Settings** manages saved profiles, playback shortcuts and settings files. **Find local servers** on the connection screen discovers Jellyfin on the LAN. Switch on **Discover and search all servers** to combine results from saved servers.
- **Services** in Settings connects Seerr with its server URL and API key. Explore can request an entire show or selected seasons and choose a 4K destination. AniList offers public anime discovery and search. To manage a watchlist, ratings or watched progress, connect your own AniList PIN application in Settings. Register its redirect URL as https://anilist.co/api/v2/oauth/pin, authorize the application, then paste the returned token into RustyFin. Automatic completion sync is opt-in and uses matching catalog IDs. An MDBList key enables ratings on matching movie and series pages. Discord activity sharing uses the desktop client and a Discord application ID you supply. Connect each service on the profile that will use it.

Account tokens and service keys are stored in Windows Credential Manager. New installations store downloads, track choices and preferences under `%LOCALAPPDATA%\RustyFin`. Upgrades reuse the existing `%LOCALAPPDATA%\RustFin` directory and sign-in. Settings exports contain playback and navigation preferences. The [feature comparison](FEATURES.md) records the scope and remaining Plezy features.

## Default controls

| Where | Controls |
| --- | --- |
| Browse | Scroll through the library continuously. Arrow keys move between posters. Page Up and Page Down move one screen, and Home and End jump to the ends. When sorting by title, select a letter on the right or type it to jump. On a controller, move right from the last column to enter the alphabet rail, choose a letter with the D-pad, and press A. Enter opens a title. Back returns to its poster. The slash key opens Search. Tab and Shift+Tab move between buttons. |
| Playback | Space or Enter pauses and resumes. Left and Right seek ten seconds. Up and Down adjust volume. T previews a seek. Left and Right move the preview, Enter seeks, and Escape cancels. M toggles mute, O opens playback options, and the square brackets adjust mpv zoom. F or F11 switches fullscreen. Escape leaves fullscreen or returns to the title. |
| XInput controller | D-pad or left stick navigates, A selects, B goes back and shoulders move focus. In Search, A opens the on-screen keyboard, X deletes and Start submits. In Settings, the D-pad moves between controls and B returns focus to the setting you opened. During playback, Y opens settings and X previews a seek. |

## Check a Dolby Vision setup

Install Dolby Vision Extensions and the decoder required by the source, such as HEVC Video Extensions. Select the playback monitor in Windows **Settings → System → Display → HDR** and enable Dolby Vision mode when offered.

```powershell
.\target\debug\rustyfin.exe --diagnostics
.\target\debug\rustyfin.exe --diagnostics "C:\Media\sample.mp4"
.\target\debug\rustyfin.exe --probe-output "C:\Media\sample.mp4"
```

Use `--probe-mel FILE` to inspect the native 8.1 descriptor for a Profile 7 MEL source, or `--probe-mel-output FILE` to run the native rendering test for that source.

The first command reports installed renderer profiles and the primary display state. Adding a file checks which profile Windows sees on its selected streams. The output probe runs a short sample through RustyFin's local media source with muted PCM audio. It reports the output channel count, native renderer frame count and display state. HDMI bitstream output is reserved for normal playback. Jellyfin publishes profile 5 and other sample files. [Jellyfin test videos](https://repo.jellyfin.org/test-videos/)
