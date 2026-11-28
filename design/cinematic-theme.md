# RustyFin's cinematic theme

RustyFin gives the artwork the strongest colour on screen. A white Play or Resume button draws attention to the next action. A silver metadata line gives context beneath each title. The shared GPUI tokens and rendering helpers live in [theme.rs](../src/ui/theme.rs).

## Colour and interaction

| Role | Token or value | Application |
| --- | --- | --- |
| Ambient canvas | `AMBIENT_BASE` · `#07090E` to `AMBIENT_MIDNIGHT` · `#121622` | Browsing and utility pages. Without artwork, `AMBIENT_INDIGO` · `#34395D` supplies the main light. |
| Charcoal | `BG` · `#0B0D12` | Discover’s movie shelf and the end of the hero fade |
| Glass panels | `PANEL_GLASS` · `#13161F` at 75% | Section cards, series sidebar panels and the connection dialog |
| Popovers | `POPOVER_GLASS` · `#13161F` at 92% | Small menus over variable backgrounds |
| Raised surface | `SURFACE` · `#13161F` | Poster placeholders and opaque controls |
| Hover | `HOVER_FILL` · white at 4% | Interactive rows with a 1 px white edge at 10% |
| Title | `TEXT` · `#FFFFFF` | 14 px semibold card titles and 15 px semibold episode titles |
| Supporting text | `META` · `#94A3B8` | Episode synopses, years and playback metadata |
| Primary action | White fill and canvas-coloured ink | Bold label, soft shadow and a 1 px hover lift when motion is enabled |
| Secondary hero action | `GLASS_FILL` and `GLASS_EDGE` | 14% white fill, 22% white edge and white text |
| Focus | `FOCUS` · `#F2F6FF` | Visible outline on cards and controls reached by keyboard or controller |

The navigation paints a top-down canvas-coloured gradient that starts at 78% opacity over the artwork. Scrolling deepens it behind the navigation labels. The hero has a separate cached mask. Its left edge starts at 92% opacity and reaches transparency at 55% of the width. Its bottom edge fades into the canvas. Both curves use cubic smoothstep. Sampling them into one small texture avoids seams where gradients meet.

## Ambient rendering

The browsing root paints the light behind the page content. [ArtworkGlass](../src/ui/glass.rs) samples coloured midtones from the decoded artwork, draws two broad pools of diffuse light and caches that texture until the artwork changes. Translucent panels pick up the light as they scroll. Windows reduced motion makes palette and switch transitions immediate.

Near-black gradients can lose fractional colour levels when a small 8-bit texture is enlarged to 4K. The ambient texture stores the colours in an expanded range. A cached, stationary dither tile composites them back to the intended brightness after scaling and palette crossfading. Each tile texel covers one physical display pixel at the current Windows scale. Text, artwork and controls paint above it.

## GPUI components

The homepage applies the backdrop, scrim and controls in paint order. Keep over-art utility controls on a dark translucent backing so their glyphs stay readable over bright sky. The rest of the hero uses the shared button treatments.

```rust
div().relative().overflow_hidden().bg(rgb(BG))
    .child(backdrop)
    .child(hero_scrim())
    .child(copy)
```

```rust
pill("play-title", "Play", Some(PLAY), PillStyle::Primary)
pill("marquee-info", "More info", None, PillStyle::Glass)
```

A settings section uses a translucent fill, 16 px corners, a fine white edge and a soft shadow. Hover uses a 4% white fill. Pressing a row deepens the fill, and keyboard focus adds a near-white outline.

```rust
glass_panel(div())
    .p(px(16.))
    .child(form)
```

## Layout

| View | Geometry | Behaviour |
| --- | --- | --- |
| Settings and Customise Home | 24 px side inset, two columns from 1020 px, up to 1040 px wide | Related controls share one card. Single-purpose Settings pages use a reading column up to 800 px wide. |
| Content shelves | `CONTENT_GUTTER` · 24 px | Discover, Browse, search and detail shelves use the same inset for visible content and fitting calculations. |
| Discover hero | Up to 55% of viewport height, with a 238 px floor on short windows | The hero reserves room for both shelf headings, cover art and captions. The server status shares the navigation row and collapses to a dot with a tooltip on narrow windows. |
| Continue Watching | 220–230 px wide, 16:9 artwork, 14 px gaps | A 3 px progress line marks unfinished titles. Episode number and remaining time sit under the artwork. |
| Recently Added | 145–155 px wide, 2:3 artwork, 14 px gaps | The cover, title and year carry the row. Windows display scale and available width determine how many covers fit. |
| Movie and TV details | 36 px circular back button, 24 px below the floating navigation | The cast portrait has a 1 px white edge at 10%. Recommendations place the year and available star score below the title. |
| Series details | Centred 1600 px maximum, 65% episodes, 3% gutter and 32% sidebar | Below 920 px of available content width, cast, technical specs and show information follow the episode list. |
| Series sidebar | 14 px horizontal and 11 px vertical panel padding, 7 px between elements | Technical badges use `MICRO_FILL` (8% white). Video, audio and subtitles each have a labelled row. |
| Episode row | 220 × 124 px still, 15 px semibold white title and 13 px silver synopsis | Stills scale down on narrow screens. Hover and keyboard selection reveal Play. Watched episodes carry a muted emerald check. |
| Player dock | 24 px side margins and inner horizontal padding, 20 px bottom margin, 14 px corners | The dock, title and toast use `OSD_GLASS` (78% obsidian) and a 1 px white edge at 12%. Transport icons are 20 px in 36 px targets. |
| Seekbar | 4 px resting track, 6 px active track, 12 px thumb in a 22 px pointer target | White shows elapsed playback. Translucent white shows the buffered range ahead of the playhead when the engine reports it. |

All geometry is in GPUI logical pixels. The seek pointer mapping and preview use the same horizontal inset as the dock’s track. The title and toast sit in a vertical stack so that a second line of text can grow without covering the next element. See [Discover](../src/ui/screens/discover.rs), [movie details](../src/ui/screens/detail.rs), [series details](../src/ui/screens/series.rs) and [player controls](../src/ui/screens/player_chrome.rs).
