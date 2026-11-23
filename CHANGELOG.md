# Changelog

## Unreleased

- Added the Waterline logo to Home and Quick Connect, and the matching fin icon to the Windows app.
- Mouse-wheel scrolling eases into place on browsing pages and in playback menus. Fine scrolling input stays immediate, and RustyFin respects the Windows animation setting.
- Use the mouse Back and Forward buttons to revisit pages, including the selected library poster and scroll position. During playback, Back closes the current player menu or returns to browsing.
- Recently Added TV uses the same portrait posters as Recently Added Movies. Both rows centre titles and years beneath the artwork and show ratings when available. Series posters also show unwatched episode counts and label ongoing shows with “Present”.
- On Discover, colours from the featured artwork flow behind Continue Watching and Recently Added TV, fading to charcoal by Recently Added Movies. Only Continue Watching has a frosted background, with a soft highlight that follows the focused card.
- Navigation floats over featured artwork and picks up its colour. Scrolling docks the bar at the top and makes it more opaque. Favourites, Collections and server library shortcuts are available directly, with additional destinations in More.
- Focused posters lift slightly when browsing shelves, search results, libraries and related titles with a keyboard or controller.
- Added a More like this poster shelf to movie, series and episode details, using Jellyfin’s recommendations.
- Series details now lead with the episode to resume or play next, including its still, remaining time and progress. Press Enter to play it, or use the arrow keys to browse seasons and episodes.
- Movie details show the selected edition and its media formats beside Play. Open **More** for downloads, watched status, tracks and editing. Cast portraits and trailer stills appear when Jellyfin provides them.
- Returning from title details fades back to the selected library poster.

- Scroll continuously through the library. The grid renders visible poster rows and fetches nearby titles as you browse.
- Jump to a letter when sorting by title in either direction. Click the A–Z rail, type a letter, or move right from the last poster in a row to use the rail with a controller. The index follows Jellyfin’s sort titles.
- Return from title details to the same poster and scroll position. If the window changed size, the grid keeps the selected poster visible.
- Use Page Up and Page Down to move through the grid one screen at a time, or Home and End to jump to either end.
