#![windows_subsystem = "windows"]

extern crate native_windows_derive as nwd;
extern crate native_windows_gui as nwg;

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use std::ffi;
use std::fmt;
use std::iter;
use std::mem;
use std::ptr;
use winapi::um::wingdi::DEVMODEA;
use winapi::um::wingdi::DISPLAY_DEVICEA;
use winapi::um::wingdi::DISPLAY_DEVICE_ACTIVE;
use winapi::um::wingdi::{DMDO_180, DMDO_270, DMDO_90, DMDO_DEFAULT};
use winapi::um::wingdi::{
    DM_DISPLAYFREQUENCY, DM_DISPLAYORIENTATION, DM_PELSHEIGHT, DM_PELSWIDTH, DM_POSITION,
};
use winapi::um::winuser::ChangeDisplaySettingsA;
use winapi::um::winuser::ChangeDisplaySettingsExA;
use winapi::um::winuser::EnumDisplayDevicesA;
use winapi::um::winuser::EnumDisplaySettingsA;
use winapi::um::winuser::EnumDisplaySettingsExA;
use winapi::um::winuser::DISP_CHANGE_SUCCESSFUL;
use winapi::um::winuser::ENUM_CURRENT_SETTINGS;
use winapi::um::winuser::{CDS_FULLSCREEN, CDS_NORESET, CDS_UPDATEREGISTRY};

use druid::widget::{Button, Flex};
use druid::{AppLauncher, PlatformError, Widget, WidgetExt, WindowDesc};

const RESOLUTIONS: &[&[(u32, u32, u32, (i32, i32), Orientation)]] = &[
    &[(3440, 1440, 100, (0, 0), Orientation::Zero)],
    &[(3440, 1440, 60, (0, 0), Orientation::Zero)],
    &[(2560, 1440, 100, (0, 0), Orientation::Zero)],
    &[(2560, 1440, 60, (0, 0), Orientation::Zero)],
    &[(3840, 2160, 60, (0, 0), Orientation::Zero)],
];

fn main() -> Result<(), PlatformError> {
    // TODO: logger is not yet initialized so nothing shows up
    log::info!("Displays:");
    for (display, active) in list_devices()?.into_iter() {
        log::info!(" -  {}", display);
        let display_settings = list_display_settings(&display)?;
        log::info!("    Display settings: {:?}", display_settings);
        if active {
            log::info!(
                "    Current display settings: {:?}",
                current_display_settings(&display)?
            );
        }
    }

    let main_window = WindowDesc::new(ui_builder)
        .with_min_size((800., 100. * RESOLUTIONS.len() as f64))
        .window_size((0., 0.));
    AppLauncher::with_window(main_window)
        .use_simple_logger()
        .launch(())
}

fn ui_builder() -> impl Widget<()> {
    let mut flex = Flex::column();

    for settings in RESOLUTIONS {
        let label = settings
            .iter()
            .map(|(width, height, frequency, (x, y), orientation)| {
                format!(
                    "{}x{}@{}Hz{:+}px{:+}px{}deg",
                    width, height, frequency, x, y, orientation
                )
            })
            .join(", ");
        flex = flex.with_flex_child(
            Button::new(label)
                .on_click(move |ctx, _data, _env| {
                    for (setting, (display, active)) in settings
                        .iter()
                        .map(|x| Some(x))
                        .chain(iter::repeat(None))
                        .zip(list_devices().unwrap_or_default().into_iter())
                    {
                        if let Some((width, height, frequency, position, orientation)) =
                            setting.as_deref()
                        {
                            if let Err(err) = change_display_settings(
                                &display,
                                *width,
                                *height,
                                *frequency,
                                *position,
                                *orientation,
                                true,
                            ) {
                                log::error!("{} error: {}", display, err);
                            }
                        } else if active {
                            let _ = change_display_settings(
                                &display,
                                0,
                                0,
                                0,
                                (0, 0),
                                Orientation::Zero,
                                true,
                            );
                        }
                    }
                    if let Err(err) = apply_display_settings() {
                        log::error!("final error: {}", err);
                    }
                    ctx.window().close();
                })
                .expand()
                .padding(5.0),
            1.0,
        )
    }

    flex
}

fn list_devices() -> Result<Vec<(String, bool)>> {
    unsafe {
        let mut res = Vec::new();
        let mut display_device: DISPLAY_DEVICEA = mem::zeroed();
        display_device.cb = mem::size_of::<DISPLAY_DEVICEA>() as u32;
        let mut dev_num = 0;
        loop {
            if EnumDisplayDevicesA(ptr::null(), dev_num, &mut display_device as *mut _, 1) == 0 {
                break;
            }
            res.push((
                ffi::CStr::from_ptr(&display_device.DeviceName[0])
                    .to_str()?
                    .to_string(),
                display_device.StateFlags & DISPLAY_DEVICE_ACTIVE == 1,
            ));
            dev_num += 1;
        }
        Ok(res)
    }
}

fn list_display_settings(display: &str) -> Result<Vec<(u32, u32, u32)>> {
    unsafe {
        let display = ffi::CString::new(display)?;
        let mut res = Vec::new();
        let mut dev_mode: DEVMODEA = mem::zeroed();
        dev_mode.dmSize = mem::size_of::<DEVMODEA>() as u16;
        let mut mode_num = 0;
        loop {
            if EnumDisplaySettingsA(display.as_ptr(), mode_num, &mut dev_mode as *mut _) == 0 {
                break;
            }
            if dev_mode.dmBitsPerPel == 32 {
                res.push((
                    dev_mode.dmPelsWidth,
                    dev_mode.dmPelsHeight,
                    dev_mode.dmDisplayFrequency,
                ));
            }
            mode_num += 1;
        }
        Ok(res)
    }
}

fn current_display_settings(display: &str) -> Result<(u32, u32, u32, (i32, i32), Orientation)> {
    unsafe {
        let display = ffi::CString::new(display)?;
        let mut dev_mode: DEVMODEA = mem::zeroed();
        dev_mode.dmSize = mem::size_of::<DEVMODEA>() as u16;
        let dw_flags = 0;
        if EnumDisplaySettingsExA(
            display.as_ptr(),
            ENUM_CURRENT_SETTINGS,
            &mut dev_mode as *mut _,
            dw_flags,
        ) == 0
        {
            return Err(anyhow::Error::msg("could not get current display settings"));
        }
        let position = {
            let position = dev_mode.u1.s2().dmPosition;
            (position.x, position.y)
        };
        let orientation = Orientation::from_u32(dev_mode.u1.s2().dmDisplayOrientation)
            .context("invalid orientation")?;
        Ok((
            dev_mode.dmPelsWidth,
            dev_mode.dmPelsHeight,
            dev_mode.dmDisplayFrequency,
            position,
            orientation,
        ))
    }
}

// Simpler version for single screen
pub fn change_default_display_settings(
    width: u32,
    height: u32,
    frequency: u32,
    permanent: bool,
) -> Result<()> {
    unsafe {
        let mut dev_mode: DEVMODEA = mem::zeroed();
        dev_mode.dmSize = mem::size_of::<DEVMODEA>() as u16;
        dev_mode.dmFields = DM_PELSWIDTH + DM_PELSHEIGHT + DM_DISPLAYFREQUENCY;
        if width == 0 {
            dev_mode.dmFields += DM_POSITION;
        }
        dev_mode.dmPelsWidth = width;
        dev_mode.dmPelsHeight = height;
        dev_mode.dmDisplayFrequency = frequency;
        let dw_flags = if permanent {
            CDS_UPDATEREGISTRY
        } else {
            CDS_FULLSCREEN
        };
        match ChangeDisplaySettingsA(&mut dev_mode as *mut _, dw_flags) {
            DISP_CHANGE_SUCCESSFUL => Ok(()),
            err => bail!("could not change display settings: {}", err),
        }
    }
}

fn change_display_settings(
    display: &str,
    width: u32,
    height: u32,
    frequency: u32,
    position: (i32, i32),
    orientation: Orientation,
    permanent: bool,
) -> Result<()> {
    log::info!(
        "{}: {}x{}@{}{:+}{:+}{}",
        display,
        width,
        height,
        frequency,
        position.0,
        position.1,
        orientation
    );
    unsafe {
        let display = ffi::CString::new(display)?;
        let mut dev_mode: DEVMODEA = mem::zeroed();
        dev_mode.dmSize = mem::size_of::<DEVMODEA>() as u16;
        dev_mode.dmFields = DM_PELSWIDTH
            + DM_PELSHEIGHT
            + DM_DISPLAYFREQUENCY
            + DM_POSITION
            + DM_DISPLAYORIENTATION;
        dev_mode.dmPelsWidth = width;
        dev_mode.dmPelsHeight = height;
        dev_mode.dmDisplayFrequency = frequency;
        dev_mode.u1.s2_mut().dmPosition.x = position.0;
        dev_mode.u1.s2_mut().dmPosition.y = position.1;
        dev_mode.u1.s2_mut().dmDisplayOrientation = orientation.into_u32();
        let dw_flags = CDS_NORESET
            + if permanent {
                CDS_UPDATEREGISTRY
            } else {
                CDS_FULLSCREEN
            };
        match ChangeDisplaySettingsExA(
            display.as_ptr(),
            &mut dev_mode as *mut _,
            ptr::null_mut(),
            dw_flags,
            ptr::null_mut(),
        ) {
            DISP_CHANGE_SUCCESSFUL => Ok(()),
            err => bail!("could not change display settings: {}", err),
        }
    }
}

fn apply_display_settings() -> Result<()> {
    log::info!("Apply display settings...");
    unsafe {
        let dw_flags = 0;
        match ChangeDisplaySettingsExA(
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            dw_flags,
            ptr::null_mut(),
        ) {
            DISP_CHANGE_SUCCESSFUL => Ok(()),
            err => bail!("could not change display settings: {}", err),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Orientation {
    Zero,
    Cw90,
    Cw180,
    Cw270,
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:+}",
            match self {
                Self::Zero => 0,
                Self::Cw90 => 90,
                Self::Cw180 => 180,
                Self::Cw270 => 270,
            }
        )
    }
}

impl Orientation {
    fn from_u32(value: u32) -> Option<Self> {
        match value {
            DMDO_DEFAULT => Some(Self::Zero),
            DMDO_90 => Some(Self::Cw90),
            DMDO_180 => Some(Self::Cw180),
            DMDO_270 => Some(Self::Cw270),
            _ => None,
        }
    }

    fn into_u32(&self) -> u32 {
        match self {
            Self::Zero => DMDO_DEFAULT,
            Self::Cw90 => DMDO_90,
            Self::Cw180 => DMDO_180,
            Self::Cw270 => DMDO_270,
        }
    }
}
