# Changelog

Entries are grouped by development build, newest first. The three November 24 player builds were verified locally and published together. Earlier entries give the corresponding Git commit.

## 2026-11-24 - Player UI builds

### Episode previews and performance overlay

- Previous and Next Episode buttons flank the ten-second seek controls. Hover for a little over a second to see the episode number, title, still, runtime and synopsis when available. The buttons are disabled when there is no episode in that direction.
- Toggle **Performance overlay** in Playback Settings for live video, audio, colour, timing, buffering and app metrics. The overlay shows active mpv shader passes and GPU timings when mpv reports them. RustyFin remembers the toggle per profile.

### Chapter markers, skip controls and File Info

- Double-click the video to enter or leave fullscreen. Clicking a playback control returns keyboard focus to the video so Space pauses or resumes playback.
- Chapter markers appear on the seek bar, with the chapter name in the seek preview. Translucent **Skip intro** and **Skip credits** buttons appear during the corresponding Jellyfin segments.
- The translucent **File Info** panel now scrolls through the source overview, file details and individual video, audio and subtitle tracks. Copy the source path when Jellyfin provides one.

### Episodes drawer

- Open **Episodes** during TV playback to browse seasons or play another episode while the video continues. The drawer opens on the current episode and shows episode titles, preview images, synopses, runtimes and watch progress.

## 2026-11-24 - Shader diagnostics, RTX Video Super Resolution, and fullscreen exit (`aa4f025`)

- Leaving video playback restores the window mode used before the video started, including maximized and fullscreen browsing.
- Temporarily disabled the ArtCNN C4F32 preset because shader compilation can stall playback. The Shaders menu shows a warning, and saved C4F32 selections use standard scaling.
- Added RAVU Zoom AR r3, NNEDI3, and SSimSuperRes presets. The Shaders menu shows when mpv is rendering the selected shader and when a preset is skipped because the scaling factor is too low.
- Added an RTX Video Super Resolution toggle to the mpv Shaders menu for supported NVIDIA GPUs. On unsupported hardware, the toggle is disabled and a warning tooltip explains why.

## 2026-11-24 - Floating playback controls and live shaders (`28190d0`)

- The playback timeline floats over the picture, with separate translucent groups for transport and settings. Hover or focus the timeline to preview a seek while the playhead marks the current time. The title and controls fade together during playback in mpv and the native Dolby Vision player.
- Switch mpv shaders during playback from **Playback Settings → Shaders**. Presets include FSRCNNX, RAVU Lite AR, ArtCNN and Anime4K. Add a local mpv shader file or select **Off** to restore standard scaling.
- Fixed an intermittent horizontal line across featured artwork on Discover.
- Sign in with a Jellyfin username and password when Quick Connect is unavailable, or select password sign-in directly.
- Home loads Continue Watching and recent additions independently across connected servers. A failed refresh keeps the last loaded titles visible and offers a retry for the affected feed or server. Open Downloads directly when Home is offline.
- Search has an on-screen keyboard for controllers. Use the D-pad and A to type, X to delete and Start to submit. Settings supports directional navigation and restores focus when you go back.
- Choose **Play from beginning** from title details or a Continue Watching card’s **More** menu. The choice carries through version selection.
- Discover’s Refresh, slideshow and shelf controls and the Back buttons on title artwork have a dark backing. Profiles is accessible from the floating navigation bar.
- On Windows, mpv negotiates HDR with the playback display. The seek bar, previews and menus share a transparent GPUI layer above the video, so showing or hiding controls leaves the video surface intact.
- Search shows a best match with separate details and playback actions, followed by matching movies, shows, episodes, people, music and videos. Press `/` to search. Recent searches belong to the current profile, and Back restores the query, scroll position and focused result.
- Continue Watching cards have a **More** menu for title details, opening the series and marking a title watched. Open it with the mouse, Shift+F10 or X on a controller. Selecting the card still starts playback.

## 2026-11-23 - Compact Browse toolbar (`4bd175d`)

- Browse shows the library name and matching title count in a compact toolbar. Choose a sort order directly, remove active filters, or switch between Small, Medium and Large posters. RustyFin remembers the poster size for each profile.

## 2026-11-23 - High-refresh library scrolling (`62c7d0a`)

- Library scrolling is smoother on high-refresh displays. The poster grid draws artwork, captions and format strips for the visible rows in fewer layout passes.

## 2026-11-23 - Waterline branding (`d0fa0d7`)

- Added the Waterline logo to Home, Quick Connect and the README, with a matching fin icon for the Windows app.

## 2026-11-23 - Mouse navigation and smooth scrolling (`21f5705`)

- Mouse-wheel scrolling eases into place on browsing pages and in playback menus. Fine scrolling input stays immediate, and RustyFin respects the Windows animation setting.
- Use the mouse Back and Forward buttons to revisit pages, including the selected library poster and scroll position. During playback, Back closes the current player menu or returns to browsing.

## 2026-11-23 - Discover artwork wash and TV posters (`c64f8b6`)

- Recently Added TV uses the same portrait posters as Recently Added Movies. Both rows centre titles and years beneath the artwork and show ratings when available. Series posters also show unwatched episode counts and label ongoing shows with “Present”.
- On Discover, colours from the featured artwork flow behind Continue Watching and Recently Added TV, fading to charcoal by Recently Added Movies. Only Continue Watching has a frosted background, with a soft highlight that follows the focused card.

## 2026-11-23 - Adaptive navigation and poster focus (`8a4e739`)

- Navigation floats over featured artwork and picks up its colour. Scrolling docks the bar at the top and makes it more opaque.
- Focused posters lift slightly when browsing shelves, search results, libraries and related titles with a keyboard or controller.

## 2026-11-23 - Top navigation and title details (`b90681e`)

- Moved navigation to a fixed top bar with Favourites, Collections and server library shortcuts. Additional destinations are available in More.
- Added a More like this poster shelf to movie, series and episode details, using Jellyfin’s recommendations.
- Series details now lead with the episode to resume or play next, including its still, remaining time and progress. Press Enter to play it, or use the arrow keys to browse seasons and episodes.
- Movie details show the selected edition and its media formats beside Play. Open **More** for downloads, watched status, tracks and editing. Cast portraits and trailer stills appear when Jellyfin provides them.
- Returning from title details fades back to the selected library poster.

## 2026-11-23 - Continuous library browsing (`508ccdf`)

- Scroll continuously through the library. The grid renders visible poster rows and fetches nearby titles as you browse.
- Jump to a letter when sorting by title in either direction. Click the A–Z rail, type a letter, or move right from the last poster in a row to use the rail with a controller. The index follows Jellyfin’s sort titles.
- Return from title details to the same poster and scroll position. If the window changed size, the grid keeps the selected poster visible.
- Use Page Up and Page Down to move through the grid one screen at a time, or Home and End to jump to either end.

## 2026-11-23 - Initial RustyFin app (`f0eb323`)

- Added the Windows Jellyfin client with Quick Connect, Discover, library browsing and search, title details, playback progress and saved server profiles.
- Added Direct Play through embedded mpv and a Windows Media Foundation path for Dolby Vision, with audio passthrough and refresh-rate matching.
- Added offline downloads, music playback, Live TV and DVR, and optional connections to Seerr, AniList, MDBList and Discord.
