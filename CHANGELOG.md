# Changelog

Entries are grouped by development build, newest first. Builds tested locally and published together have separate entries. Earlier entries include their Git commit.

## 2026-11-28 - Continue Watching removal notice

- The removal notice uses Discover’s dark glass and white **Undo** button. Long episode titles wrap above the confirmation, leaving **Undo** and dismiss available in narrow windows. Screen readers receive the full removal message.

## 2026-11-28 - Shelf and panel animations

- Discover’s shelf arrows ease between pages and respond when you reverse direction mid-scroll. Continue Watching cards close the gap after a removal, and a restored card rises and fades in when you choose **Undo**.
- Settings and Customise Home slide in beneath a stationary header. The **Mixed formats** breakdown unfolds on its card. With Windows animations turned off, each change appears immediately.

## 2026-11-28 - Remove individual episodes from Continue Watching

- Open a Continue Watching card’s **⋯** menu or right-click it to remove a film, one episode or the whole series. Removing an episode allows another eligible episode of the same show to appear. Playback progress is preserved.
- **Undo** restores the last removal. Earlier removals can be restored in **Customise Home** for the current RustyFin profile.

## 2026-11-28 - Quality labels on Discover and title pages

- Restored the black resolution, dynamic range and codec strip on Continue Watching, Recently Added TV and Movies, episode thumbnails, extras and More like this. Shows with different episode formats have an expandable **Mixed formats** label.
- Series details also show the resume or next episode’s quality beside its title when Jellyfin supplies it.

## 2026-11-28 - Smoother ambient gradients

- Dark ambient lighting now preserves fractional colour levels and uses a stationary dither at the display’s native pixel density. This smooths visible bands across large windows, including the indigo background used when artwork is unavailable.
- In a lossless capture of maximized Settings on a 4K display, the longest run of identical colour in the same 1,000-pixel sample fell from 452 pixels to two.

## 2026-11-28 - Ambient lighting and utility panels

- Settings and other browsing pages borrow two hues from the featured artwork. A softly lit obsidian and midnight-indigo background appears when artwork is unavailable.
- Settings groups playback, controls, downloads, connections and advanced options in translucent panels. Customise Home groups shelf order and visibility separately from pinned shortcuts. Both pages switch from one column to two at 1,020 logical pixels.
- Settings rows give distinct hover, press and keyboard focus feedback. Switches respect the Windows animation preference. Shelf reorder buttons identify the shelf they move and align across rows.
- Discover, Browse, search and title-detail shelves align to a shared 24 px side margin.

## 2026-11-28 - Discover shelves, title details and player controls

- Continue Watching uses smaller landscape cards. Recently Added uses larger posters, and the hero leaves room for the TV shelf captions on desktop. Discover shows the current server’s connection status in the navigation bar.
- Movie and show recommendations display the year and available star rating beneath clean poster artwork. Both detail views use the same glass back button. Cast portraits have a fine outline and a soft fade at the end of a continuing row.
- Series sidebars use compact cards and group the selected episode’s video, audio and subtitle formats into subtle badges. Episode rows pair white semibold titles with silver synopses and a muted emerald watched check.
- Playback controls share a floating translucent dock. The seekbar distinguishes played and buffered video, expands on hover and has a larger scrubber. The title header and playback notices share the dock’s glass styling.

## 2026-11-28 - Discover and series details redesign

- White Play and Resume buttons, translucent secondary actions and a floating navigation gradient sit over artwork on an obsidian background.
- Discover uses a shorter hero and up to six landscape Continue Watching cards. Episode numbers and time remaining appear below each still, with a thin progress line along its bottom edge. Recently Added uses clean poster artwork.
- Series pages pair the season and episode list with cast, episode-specific technical specs and show information. Episode rows show larger stills, runtime, synopsis and a watched checkmark. Play appears on hover or keyboard selection. The sidebar stacks below the list in narrow windows.

## 2026-11-28 - Cinematic colour and contrast

- Discover, Favourites, Collections and title details pick up a soft light from the featured artwork. The Home wash fades to a blue-black canvas by Recently Added Movies.
- Play, Resume and featured Details use electric cyan. The active navigation destination gains an underline. Settings, menus and the Browse tray use distinct surface and hover colours.
- Posters have a dark bottom fade, fine hover edges and a two-tone focus ring. White titles and silver metadata stay legible across the library and player controls.

## 2026-11-28 - Cinematic spotlight and Browse poster gallery

- Favourites and Collections feature the selected title's artwork, logo and synopsis above a compact poster grid. Browse shows the selected title in a details tray below the gallery.
- Click a poster or move with the arrow keys to preview a title. Open Details with the button, Enter or a double-click. The poster moves into the details header when Windows animations are enabled.

## 2026-11-28 - Search actions, Settings layout and series resume

- The best search result has a Details button beside Play or Resume. Windows accessibility tools identify the title for each action.
- Settings uses a centered reading column on wide displays. Video playback settings group the playback, video output, subtitle and audio controls.
- Series details show the current episode, progress and time remaining with the main playback action. Season-list Play buttons sit closer to their episodes on wide displays.

## 2026-11-28 - Title details, pairing and search

- Quick Connect exposes the pairing instructions and reads the code digit by digit to screen readers. Episode, cast and extras cards expose their names and actions to Windows accessibility tools.
- Series pages load episodes independently of Next Up. If Jellyfin cannot retrieve the resume episode, viewers can play an episode directly or retry Next Up from the header.
- Search shows each category as its server responds and keeps focus on the selected result as more arrive.
- Retrying a failed Favourite or watched-state change resubmits the original action to the title’s server.
- Movies and shows without a backdrop use a compact header with the poster when available.

## 2026-11-27 - Discover crossfades and playback menu motion

- Discover keeps the current featured artwork visible until the next image loads, then crossfades between the two. The navigation tint and artwork wash follow the incoming image.
- Playback settings rise into place and the Episodes drawer slides in from the right. Submenu content moves in the direction of navigation while the drawer header stays visible.
- Featured artwork, tint, menu and poster focus transitions respect the Windows animation setting.

## 2026-11-27 - Transparent subtitles and Windows window controls

- Windows Dolby Vision subtitles use outlined lettering on a transparent background.
- Fixed the player intercepting title bar and resize border clicks. Dragging, resizing, minimizing, maximizing, restoring and closing the window work during playback.

## 2026-11-27 - Subtitle track switching

- During Windows Dolby Vision playback, requesting subtitles from Jellyfin keeps the video playing. SubRip tracks load in their original format when available, with WebVTT as a fallback.
- The subtitle menu shows when a track is loading. If loading fails, the current track remains active. Select **Off** to disable subtitles immediately and cancel a pending selection.
- Track selections are saved to the profile on the server that owns the title.

## 2026-11-27 - Menu entrance animations

- Added short fade and slide transitions to navigation and Browse menus, Browse filters, title actions and version selection, playback menus, and the Continue Watching undo notice.

## 2026-11-27 - Playback startup

- The player covers the video area until the first frame is ready in mpv or Windows Dolby Vision, so windows behind RustyFin stay hidden while playback opens.

## 2026-11-27 - Dolby Vision playback controls

- Fixed playback controls staying visible when an accessibility client was connected. The controls now fade after inactivity in windowed and fullscreen playback, including Dolby Vision Profile 8 through the Windows renderer.
- Narrator users can select **Show playback controls** to bring back the overlay and move focus to Play or Pause.

## 2026-11-26 - Narrator headings, per-server Search and independent trailers

- Discover, Search, Browse, title details and settings expose headings with native Windows UI Automation levels. Settings and player switches report their on/off state and can be toggled using assistive technology.
- Search gives each responding server its own labelled result rows. Results from slower servers appear even when the first server has filled its initial batch, and each row can be paged independently.
- Title details and playback choices load independently of trailers and bonus features. A failed request leaves the detail page and any available extras usable, with a retry in the extras section.

## 2026-11-25 - Accessible controls and multi-server browsing

- Windows UI Automation exposes named controls for sign-in, navigation, posters, settings and playback. Assistive technology can focus posters, seek and adjust volume.
- Opening or playing a title from another Jellyfin server keeps the active RustyFin profile. Playback and progress use the title’s server, and Back returns to the original view.
- Search displays the first results while other servers are still responding. If a server fails, results from available servers stay visible and Search offers a retry.

## 2026-11-25 - Discover personalisation

- **Customise Home** lets each profile reorder or hide shelves, pin Favourites and libraries, and restore the default layout. The controls work with mouse, keyboard and controller.
- **Remove from Continue Watching** hides a title from that profile’s shelf while preserving its playback progress. **Undo** restores the title to the shelf.

## 2026-11-24 - Profile 7 FEL playback and Windows Dolby Vision

- RustyFin uses mpv to reconstruct Profile 7 FEL from both video layers. If reconstruction fails, playback resumes at the same position using the HDR10 base layer. Both paths support SDR tone mapping.
- **Use Windows Dolby Vision output** in **Settings → Video playback** is enabled by default for compatible Dolby Vision profiles. Turn it off to use mpv for those profiles as well. Profile 7 FEL uses mpv in either setting.

## 2026-11-24 - SDR tone mapping

- HDR and supported Dolby Vision sources play with mpv tone mapping when Windows reports SDR on the playback display. Native Dolby Vision playback resumes through mpv if the display switches to SDR.
- A brief player notice confirms SDR tone mapping and points to Windows HDR settings when the display supports HDR.

## 2026-11-24 - Poster and title ratings

- Discover and Browse posters and title details show Jellyfin's critic percentage with a tomato icon. This includes Rotten Tomatoes scores supplied by Jellyfin and works without an MDBList key.
- Jellyfin's community score uses a star. Title details show a labelled IMDb score when the optional MDBList integration identifies the source as IMDb.

## 2026-11-24 - Player UI builds (`0c2c47b`)

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
