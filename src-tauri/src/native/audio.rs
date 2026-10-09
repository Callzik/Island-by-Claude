//! Default output device: peak meter (for the liquid wave), master volume, name.

use windows::core::{Result, GUID};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    eCommunications, eConsole, eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};

use super::util::{pcwstr, take_pwstr, wide};

#[allow(non_snake_case, dead_code)]
mod policy {
    use std::ffi::c_void;
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, HRESULT, PCWSTR};
    use windows::Win32::Media::Audio::ERole;

    /// Undocumented (but stable since Windows 7) interface used by the Sound control
    /// panel to change the default endpoint. Only `SetDefaultEndpoint` is called;
    /// the other slots just keep the vtable layout.
    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: IUnknown {
        pub fn GetMixFormat(&self, id: PCWSTR, fmt: *mut *mut c_void) -> HRESULT;
        pub fn GetDeviceFormat(&self, id: PCWSTR, default: i32, fmt: *mut *mut c_void) -> HRESULT;
        pub fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
        pub fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
        pub fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, def: *mut i64, min: *mut i64) -> HRESULT;
        pub fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
        pub fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
        pub fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
        pub fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        pub fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        pub fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
        pub fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
    }

}
use policy::IPolicyConfig;

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub default: bool,
}

fn friendly_name(device: &IMMDevice) -> String {
    unsafe {
        device
            .OpenPropertyStore(STGM_READ)
            .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
            .and_then(|pv| PropVariantToStringAlloc(&pv))
            .map(take_pwstr)
            .unwrap_or_default()
    }
}

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
        self.device_name = friendly_name(&device);
        true
    }

    /// Active output devices (speakers, headphones, HDMI…).
    pub fn outputs(&self) -> Vec<OutputDevice> {
        let mut out = Vec::new();
        unsafe {
            let Ok(list) = self.enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
            let n = list.GetCount().unwrap_or(0);
            for i in 0..n {
                if let Ok(d) = list.Item(i) {
                    let id = d.GetId().map(take_pwstr).unwrap_or_default();
                    out.push(OutputDevice {
                        default: id == self.device_id,
                        name: friendly_name(&d),
                        id,
                    });
                }
            }
        }
        out
    }

    /// Makes `id` the default device for every role.
    pub fn set_default(&mut self, id: &str) -> bool {
        let w = wide(id);
        let ok = unsafe {
            match CoCreateInstance::<_, IPolicyConfig>(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) {
                Ok(pc) => [eConsole, eMultimedia, eCommunications]
                    .into_iter()
                    .all(|role| pc.SetDefaultEndpoint(pcwstr(&w), role).is_ok()),
                Err(_) => false,
            }
        };
        self.refresh();
        ok
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
