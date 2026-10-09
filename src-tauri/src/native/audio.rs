//! Default output device: peak meter (for the liquid wave), master volume, name.

use windows::core::Result;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;

use super::util::take_pwstr;

pub struct Audio {
    enumerator: IMMDeviceEnumerator,
    device_id: String,
    device_name: String,
    meter: Option<IAudioMeterInformation>,
    volume: Option<IAudioEndpointVolume>,
}

impl Audio {
    /// Must be called on a COM-initialised thread.
    pub fn new() -> Result<Self> {
        let enumerator: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let mut a = Self {
            enumerator,
            device_id: String::new(),
            device_name: String::new(),
            meter: None,
            volume: None,
        };
        a.refresh();
        Ok(a)
    }

    /// Re-binds to the default output device if it changed. Returns true on change.
    pub fn refresh(&mut self) -> bool {
        let device: IMMDevice = match unsafe { self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
            Ok(d) => d,
            Err(_) => {
                let changed = self.meter.is_some();
                self.meter = None;
                self.volume = None;
                self.device_id.clear();
                return changed;
            }
        };
        let id = unsafe { device.GetId() }.map(take_pwstr).unwrap_or_default();
        if id == self.device_id && self.meter.is_some() {
            return false;
        }
        self.device_id = id;
        self.meter = unsafe { device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) }.ok();
        self.volume = unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }.ok();
        self.device_name = unsafe {
            device
                .OpenPropertyStore(STGM_READ)
                .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
                .and_then(|pv| PropVariantToStringAlloc(&pv))
                .map(take_pwstr)
                .unwrap_or_default()
        };
        true
    }

    pub fn peak(&self) -> f32 {
        self.meter
            .as_ref()
            .and_then(|m| unsafe { m.GetPeakValue() }.ok())
            .unwrap_or(0.0)
    }

    pub fn volume(&self) -> (f32, bool) {
        match &self.volume {
            Some(v) => unsafe {
                (
                    v.GetMasterVolumeLevelScalar().unwrap_or(0.0),
                    v.GetMute().map(|b| b.as_bool()).unwrap_or(false),
                )
            },
            None => (0.0, false),
        }
    }

    pub fn set_volume(&self, level: f32) {
        if let Some(v) = &self.volume {
            unsafe {
                let _ = v.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
                if level > 0.0 {
                    let _ = v.SetMute(false, std::ptr::null());
                }
            }
        }
    }

    pub fn set_mute(&self, mute: bool) {
        if let Some(v) = &self.volume {
            unsafe {
                let _ = v.SetMute(mute, std::ptr::null());
            }
        }
    }

    /// "Динамики (Realtek(R) Audio)" -> "Динамики"
    pub fn device_short_name(&self) -> String {
        let n = self.device_name.trim();
        match n.find(" (") {
            Some(i) if i > 0 => n[..i].to_string(),
            _ => n.to_string(),
        }
    }
}
