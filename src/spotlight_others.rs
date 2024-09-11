use super::Error;
use super::{PluginConfig, WindowConfig};
use std::sync::Mutex;
use tauri::{GlobalShortcutManager, Manager, Window, WindowEvent, Wry};
use winapi::shared::windef::HWND;
use winapi::um::winuser::{SetForegroundWindow, ShowWindow, SW_RESTORE};

#[derive(Default, Debug)]
pub struct SpotlightManager {
    pub config: PluginConfig,
    registered_window: Mutex<Vec<String>>,
}

impl SpotlightManager {
    pub fn new(config: PluginConfig) -> Self {
        let mut manager = Self::default();
        manager.config = config;
        manager
    }

    fn get_window_config(&self, window: &Window<Wry>) -> Option<WindowConfig> {
        if let Some(window_configs) = self.config.windows.clone() {
            for window_config in window_configs {
                if window.label() == window_config.label {
                    return Some(window_config.clone());
                }
            }
        }
        None
    }

    pub fn init_spotlight_window(&self, window: &Window<Wry>) -> Result<(), Error> {
        let window_config = match self.get_window_config(&window) {
            Some(window_config) => window_config,
            None => return Ok(()),
        };
        let auto_hide = window_config.auto_hide.unwrap_or(true);
        let label = window.label().to_string();
        let handle = window.app_handle();
        let state = handle.state::<SpotlightManager>();
        let mut registered_window = state
            .registered_window
            .lock()
            .map_err(|_| Error::Mutex(String::from("failed to lock registered window")))?;
        let registered = registered_window.contains(&label);
        if !registered {
            register_shortcut_for_window(&window, &window_config)?;
            register_close_shortcut(&window)?;
            handle_focus_state_change(&window, auto_hide);
            registered_window.push(label);
        }
        Ok(())
    }

    pub fn show(&self, window: &Window<Wry>) -> Result<(), Error> {
        if !window
            .is_visible()
            .map_err(|_| Error::FailedToCheckWindowVisibility)?
        {
            window.show().map_err(|_| Error::FailedToShowWindow)?;
            window.set_focus().map_err(|_| Error::FailedToShowWindow)?;
            bring_window_to_front(window);
        }
        Ok(())
    }

    pub fn hide(&self, window: &Window<Wry>) -> Result<(), Error> {
        if window
            .is_visible()
            .map_err(|_| Error::FailedToCheckWindowVisibility)?
        {
            window.hide().map_err(|_| Error::FailedToHideWindow)?;
        }
        Ok(())
    }
}

fn bring_window_to_front(window: &Window<Wry>) {
    unsafe {
        let hwnd = window.hwnd().expect("Failed to get HWND").0 as HWND;
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

fn register_shortcut_for_window(
    window: &Window<Wry>,
    window_config: &WindowConfig,
) -> Result<(), Error> {
    let shortcut = match window_config.shortcut.clone() {
        Some(shortcut) => shortcut,
        None => return Ok(()),
    };
    let window = window.to_owned();
    let mut shortcut_manager = window.app_handle().global_shortcut_manager();
    shortcut_manager
        .register(&shortcut, move || {
            let app_handle = window.app_handle();
            let manager = app_handle.state::<SpotlightManager>();
            if window.is_visible().unwrap() {
                manager.hide(&window).unwrap();
            } else {
                manager.show(&window).unwrap();
            }
        })
        .map_err(|_| Error::Other(String::from("failed to register shortcut")))?;
    Ok(())
}

fn register_close_shortcut(window: &Window<Wry>) -> Result<(), Error> {
    let window = window.to_owned();
    let mut shortcut_manager = window.app_handle().global_shortcut_manager();
    let app_handle = window.app_handle();
    let manager = app_handle.state::<SpotlightManager>();
    if let Some(close_shortcut) = &manager.config.global_close_shortcut {
        if let Ok(registered) = shortcut_manager.is_registered(close_shortcut) {
            if !registered {
                shortcut_manager
                    .register(close_shortcut, move || {
                        let app_handle = window.app_handle();
                        let state = app_handle.state::<SpotlightManager>();
                        let registered_window = state.registered_window.lock().unwrap();
                        let window_labels = registered_window.clone();
                        std::mem::drop(registered_window);
                        for label in window_labels {
                            if let Some(window) = app_handle.get_window(&label) {
                                window.hide().unwrap();
                            }
                        }
                    })
                    .map_err(tauri::Error::Runtime)?;
            }
        } else {
            return Err(Error::Other(String::from("failed to register shortcut")));
        }
    }
    Ok(())
}

fn unregister_close_shortcut(window: &Window<Wry>) -> Result<(), Error> {
    let window = window.to_owned();
    let mut shortcut_manager = window.app_handle().global_shortcut_manager();
    let app_handle = window.app_handle();
    let manager = app_handle.state::<SpotlightManager>();
    if let Some(close_shortcut) = manager.config.global_close_shortcut.clone() {
        if let Ok(registered) = shortcut_manager.is_registered(&close_shortcut) {
            if registered {
                shortcut_manager
                    .unregister(&close_shortcut)
                    .map_err(tauri::Error::Runtime)?;
            }
        } else {
            return Err(Error::Other(String::from("failed to unregister shortcut")));
        }
    }
    Ok(())
}

fn handle_focus_state_change(window: &Window<Wry>, auto_hide: bool) {
    let w = window.to_owned();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            unregister_close_shortcut(&w).unwrap(); // FIXME:
            if auto_hide {
                w.hide().unwrap();
            } else {
                // send a message to js
                let window = app_handle.get_window(&label).unwrap();
                window
                    .emit_and_trigger("window_did_resign_key", Some(true))
                    .unwrap();
            }
        } else {
            register_close_shortcut(&w).unwrap(); // FIXME:
        }
    });
}
