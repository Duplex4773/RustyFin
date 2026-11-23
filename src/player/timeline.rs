use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND, POINT},
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            DirectComposition::*,
            DirectWrite::*,
            Dxgi::{Common::*, IDXGIDevice, IDXGISurface},
        },
    },
    core::{Interface, w},
};

pub const HEIGHT: f32 = 29.;
pub const RAIL_INSET: f32 = 76.;

pub(super) struct Timeline {
    composition: IDCompositionDevice,
    target: IDCompositionTarget,
    visual: IDCompositionVisual,
    dc: ID2D1DeviceContext,
    brush: ID2D1SolidColorBrush,
    elapsed_format: IDWriteTextFormat,
    remaining_format: IDWriteTextFormat,
    surface: Option<IDCompositionSurface>,
    size: (i32, i32),
    last: Option<(i32, i32, f32, String, String)>,
}

impl Timeline {
    pub fn new(parent: HWND) -> Result<Self> {
        unsafe {
            let mut d3d = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let dxgi: IDXGIDevice = d3d.context("Creating the seek overlay device")?.cast()?;
            let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
            // The parent's topmost target is above its video child HWND. Only
            // the UI surface is blended here; the decoder retains its own surface.
            let target = composition.CreateTargetForHwnd(parent, true)?;
            let visual = composition.CreateVisual()?;
            let dc = D2D1CreateDevice(&dxgi, None)?
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = dc.CreateSolidColorBrush(&color(1., 1.), None)?;
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = |align| -> windows::core::Result<IDWriteTextFormat> {
                let f = factory.CreateTextFormat(
                    w!("Segoe UI"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    13.,
                    w!(""),
                )?;
                f.SetTextAlignment(align)?;
                f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                Ok(f)
            };
            Ok(Self {
                composition,
                target,
                visual,
                dc,
                brush,
                elapsed_format: format(DWRITE_TEXT_ALIGNMENT_TRAILING)?,
                remaining_format: format(DWRITE_TEXT_ALIGNMENT_LEADING)?,
                surface: None,
                size: (0, 0),
                last: None,
            })
        }
    }

    pub fn hide(&mut self) {
        if self.last.take().is_some() {
            unsafe {
                let _ = self.target.SetRoot(None);
                let _ = self.composition.Commit();
            }
        }
    }

    pub fn show(
        &mut self,
        width: i32,
        y: i32,
        scale: f32,
        progress: f32,
        elapsed: &str,
        remaining: &str,
    ) -> Result<()> {
        let height = (HEIGHT * scale).round().max(1.) as i32;
        let inset = RAIL_INSET * scale;
        let handle = (inset + (width as f32 - 2. * inset) * progress).round() as i32;
        unsafe {
            if self.size != (width, height) {
                let surface = self.composition.CreateSurface(
                    width as u32,
                    height as u32,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                )?;
                self.visual.SetContent(&surface)?;
                self.surface = Some(surface);
                self.size = (width, height);
                self.last = None;
            }
            if self.last.as_ref().is_some_and(|(oy, ox, s, e, r)| {
                *oy == y && *ox == handle && *s == scale && e == elapsed && r == remaining
            }) {
                return Ok(());
            }
            let surface = self.surface.as_ref().unwrap();
            let mut offset = POINT::default();
            let texture: IDXGISurface = surface.BeginDraw(None, &mut offset)?;
            let drawn = self.draw(&texture, offset, scale, handle, elapsed, remaining);
            let ended = surface.EndDraw();
            drawn?;
            ended?;
            self.visual.SetOffsetY2(y as f32)?;
            if self.last.is_none() {
                self.target.SetRoot(&self.visual)?;
            }
            self.composition.Commit()?;
        }
        self.last = Some((y, handle, scale, elapsed.to_owned(), remaining.to_owned()));
        Ok(())
    }

    fn draw(
        &self,
        texture: &IDXGISurface,
        offset: POINT,
        scale: f32,
        handle: i32,
        elapsed: &str,
        remaining: &str,
    ) -> Result<()> {
        let width = self.size.0 as f32 / scale;
        let height = self.size.1 as f32 / scale;
        let ox = offset.x as f32 / scale;
        let oy = offset.y as f32 / scale;
        let rect = |l, t, r, b| D2D_RECT_F {
            left: ox + l,
            top: oy + t,
            right: ox + r,
            bottom: oy + b,
        };
        let mid = height * 0.5;
        let end = width - RAIL_INSET;
        let x = handle as f32 / scale;
        unsafe {
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96. * scale,
                dpiY: 96. * scale,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let bitmap = self.dc.CreateBitmapFromDxgiSurface(texture, Some(&props))?;
            self.dc.SetTarget(&bitmap);
            self.dc.SetDpi(96. * scale, 96. * scale);
            self.dc.BeginDraw();
            // DirectComposition may hand us a subrectangle of a larger texture.
            self.dc
                .PushAxisAlignedClip(&rect(0., 0., width, height), D2D1_ANTIALIAS_MODE_ALIASED);
            self.dc.Clear(Some(&color(0., 0.)));
            let pill = |bounds, radius, tint| {
                self.brush.SetColor(&tint);
                self.dc.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: bounds,
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &self.brush,
                );
            };
            pill(
                rect(RAIL_INSET, mid - 2.5, end, mid + 3.5),
                3.,
                color(0., 0.45),
            );
            pill(
                rect(RAIL_INSET, mid - 2.5, end, mid + 2.5),
                2.5,
                color(1., 0.44),
            );
            if x > RAIL_INSET {
                pill(
                    rect(RAIL_INSET, mid - 2.5, x, mid + 2.5),
                    2.5,
                    color(1., 1.),
                );
            }
            pill(
                rect(x - 1.5, mid - 6.5, x + 1.5, mid + 6.5),
                1.5,
                color(1., 1.),
            );
            for (text, format, left, right) in [
                (elapsed, &self.elapsed_format, 0., RAIL_INSET - 7.),
                (remaining, &self.remaining_format, end + 7., width),
            ] {
                let text: Vec<u16> = text.encode_utf16().collect();
                for (dy, tint) in [(1., color(0., 0.85)), (0., color(1., 1.))] {
                    self.brush.SetColor(&tint);
                    self.dc.DrawText(
                        &text,
                        format,
                        &rect(left, dy, right, height + dy),
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }
            self.dc.PopAxisAlignedClip();
            let result = self.dc.EndDraw(None, None);
            self.dc.SetTarget(None);
            result?;
        }
        Ok(())
    }
}
fn color(gray: f32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: gray,
        g: gray,
        b: gray,
        a: alpha,
    }
}
