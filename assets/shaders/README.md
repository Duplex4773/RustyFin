# Bundled mpv shaders

Cargo copies these files and their licenses to the `shaders` directory beside the executable. Keep that directory when distributing the app. The shader files remain separate so they can be replaced without rebuilding RustyFin.

| Files | Upstream revision | License |
| --- | --- | --- |
| `ArtCNN_C4F16.glsl`, `ArtCNN_C4F32.glsl`, `ArtCNN_C4F16_DS.glsl` | [ArtCNN at 0837e53](https://github.com/Artoriuz/ArtCNN/tree/0837e5326e710f49e2ab0a8d995da0708047442e/GLSL) | [MIT](ArtCNN-LICENSE.txt) |
| `FSRCNNX_x2_8-0-4-1.glsl`, `FSRCNNX_x2_16-0-4-1.glsl` | [igv’s FSRCNNX 1.1 release](https://github.com/igv/FSRCNN-TensorFlow/releases/tag/1.1) | [LGPL-3.0-or-later](LGPL-3.0.txt), incorporating [GPL 3.0](GPL-3.0.txt) |
| `ravu-lite-ar-r3.hook` | [mpv-prescalers at b3f0a59](https://github.com/bjin/mpv-prescalers/blob/b3f0a59d68f33b7162051ea5970a5169558f0ea2/gather/ravu-lite-ar-r3.hook) | [LGPL-3.0-or-later](LGPL-3.0.txt), incorporating [GPL 3.0](GPL-3.0.txt) |
| `ravu-zoom-ar-r3.hook`, `nnedi3-nns64-win8x4.hook` | [mpv-prescalers at b3f0a59](https://github.com/bjin/mpv-prescalers/tree/b3f0a59d68f33b7162051ea5970a5169558f0ea2/gather) | [LGPL-3.0-or-later](LGPL-3.0.txt), incorporating [GPL 3.0](GPL-3.0.txt) |
| `SSimSuperRes.glsl` | [Shiandow’s SSimSuperRes as published by igv at 15d9344](https://gist.github.com/igv/2364ffa6e81540f29cb7ab4c9bc05b6b/15d93440d0a24fc4b8770070be6a9fa2af6f200b) | [LGPL-3.0-or-later](LGPL-3.0.txt), incorporating [GPL 3.0](GPL-3.0.txt) |
| `Anime4K_Upscale_CNN_x2_M.glsl` | [Anime4K at 7684e95](https://github.com/bloc97/Anime4K/blob/7684e9586f8dcc738af08a1cdceb024cc184f426/glsl/Upscale/Anime4K_Upscale_CNN_x2_M.glsl) | [MIT](Anime4K-LICENSE.txt) |

`sharpen.glsl` and `soften.glsl` are RustyFin shaders under the project license.
