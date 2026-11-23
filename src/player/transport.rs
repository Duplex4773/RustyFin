use crate::jellyfin::Item;
use anyhow::Result;
use std::sync::mpsc::{self, Receiver};
use windows::{
    Foundation::TypedEventHandler,
    Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls as Smtc,
        SystemMediaTransportControlsButton as Button,
        SystemMediaTransportControlsButtonPressedEventArgs,
    },
    Win32::{Foundation::HWND, System::WinRT::ISystemMediaTransportControlsInterop},
    core::{HSTRING, factory},
};

pub enum MediaCommand {
    Play,
    Pause,
    Stop,
    Next,
    Restart,
}

pub struct Transport {
    api: Smtc,
    token: i64,
    events: Receiver<MediaCommand>,
    last: Option<(bool, bool)>,
}

impl Transport {
    pub fn new(window: HWND, item: &Item, audio: bool, seekable: bool) -> Result<Self> {
        let interop = factory::<Smtc, ISystemMediaTransportControlsInterop>()?;
        let api: Smtc = unsafe { interop.GetForWindow(window)? };
        let display = api.DisplayUpdater()?;
        display.ClearAll()?;
        if audio {
            display.SetType(MediaPlaybackType::Music)?;
            let music = display.MusicProperties()?;
            music.SetTitle(&HSTRING::from(&item.name))?;
            music.SetArtist(&HSTRING::from(
                item.artists.as_deref().unwrap_or_default().join(", "),
            ))?;
            music.SetAlbumTitle(&HSTRING::from(item.album.as_deref().unwrap_or_default()))?;
        } else {
            display.SetType(MediaPlaybackType::Video)?;
            let video = display.VideoProperties()?;
            video.SetTitle(&HSTRING::from(&item.name))?;
            video.SetSubtitle(&HSTRING::from(
                item.series_name.as_deref().unwrap_or_default(),
            ))?;
        }
        display.Update()?;
        api.SetIsPlayEnabled(true)?;
        api.SetIsPauseEnabled(true)?;
        api.SetIsStopEnabled(true)?;
        api.SetIsPreviousEnabled(seekable)?;
        api.SetIsNextEnabled(false)?;
        api.SetPlaybackStatus(MediaPlaybackStatus::Changing)?;
        api.SetIsEnabled(true)?;
        let (send, events) = mpsc::channel();
        let token = api.ButtonPressed(&TypedEventHandler::<
            Smtc,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_, args| {
            if let Some(event) = args.as_ref() {
                let command = match event.Button()? {
                    Button::Play => MediaCommand::Play,
                    Button::Pause => MediaCommand::Pause,
                    Button::Stop => MediaCommand::Stop,
                    Button::Next => MediaCommand::Next,
                    Button::Previous => MediaCommand::Restart,
                    _ => return Ok(()),
                };
                let _ = send.send(command);
            }
            Ok(())
        }))?;
        Ok(Self {
            api,
            token,
            events,
            last: None,
        })
    }
    pub fn poll(&mut self, paused: bool, next: bool) -> Option<MediaCommand> {
        if self.last != Some((paused, next)) {
            let status = if paused {
                MediaPlaybackStatus::Paused
            } else {
                MediaPlaybackStatus::Playing
            };
            if self
                .api
                .SetPlaybackStatus(status)
                .and_then(|_| self.api.SetIsNextEnabled(next))
                .is_ok()
            {
                self.last = Some((paused, next));
            }
        }
        self.events.try_recv().ok()
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.api.RemoveButtonPressed(self.token);
        let _ = self.api.SetPlaybackStatus(MediaPlaybackStatus::Stopped);
        let _ = self.api.SetIsEnabled(false);
    }
}
