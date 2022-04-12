use crate::ssh::{CopyDataStruct, AGENT_COPY_DATA_ID};
use anyhow::{bail, Result};
use std::ffi::CString;
use std::os::raw::c_ulong;
use std::process::Command;
use widestring::U16CString;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_COPYDATA};

pub struct PageantWindow {
    hwnd: HWND,
}

impl PageantWindow {
    pub fn new(window_name: &str, class_name: &str) -> Result<Self> {
        let window_name = U16CString::from_str(window_name)?;
        let window_name = PCWSTR(window_name.as_ptr() as *mut u16);

        let class_name = U16CString::from_str(class_name)?;
        let class_name = PCWSTR(class_name.as_ptr() as *mut u16);

        let mut hwnd;
        unsafe {
            hwnd = FindWindowW(class_name, window_name);
        }

        // todo: test failure of finding window
        if hwnd.0 == 0 {
            log::info!("pageant window not found. launching");
            let connect_command = Command::new("gpg-connect-agent")
                .args(["/bye"])
                .output()
                .expect("foo");
            log::info!("pageant launch status: {}", connect_command.status);
        }

        unsafe {
            hwnd = FindWindowW(class_name, window_name);
        }

        if hwnd.0 == 0 {
            log::info!("hwnd not found");
            bail!("could not find pageant window");
        }

        log::info!("found hwnd {:?}", hwnd);

        Ok(Self { hwnd })
    }

    pub fn send_message(&self, map_name: &str) -> Result<()> {
        let map_name_c = CString::new(map_name)?;
        let map_name_slice = map_name_c.as_bytes_with_nul();
        let copy_data = Box::new(CopyDataStruct {
            dw_data: AGENT_COPY_DATA_ID,
            cb_data: map_name_slice.len() as c_ulong,
            lp_data: map_name_slice.as_ptr() as isize,
        });
        let copy_data_ptr = Box::into_raw(copy_data);
        let lparam = LPARAM(copy_data_ptr as isize);

        let result;
        unsafe {
            result = SendMessageW(self.hwnd, WM_COPYDATA, WPARAM(0_usize), lparam);
        }
        if result.0 == 0 {
            log::info!("could not send data");
            bail!("could not send data");
        }

        unsafe {
            // after calling Box::into_raw, we're responsible for cleaning up.
            drop(Box::from_raw(copy_data_ptr));
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ssh::file_mapping::FileMapping;
    use crate::ssh::test::{window_input, Window};
    use std::process;

    #[test]
    fn test_new() {
        let window = Window::new();
        let pageant_window = PageantWindow::new(&window.window_name(), &window.class_name());
        assert!(pageant_window.is_ok())
    }

    #[test]
    fn test_send_message() {
        // generate the shared memory
        let map_name = format!("WSLPageantRequest-test-send-message{}", process::id());
        let file_mapping = FileMapping::new(&map_name).unwrap();
        let shared_slice = file_mapping.shared_memory();

        // set our shared memory to the expected data
        let (length_bytes, data) = window_input();
        for (k, v) in data.to_vec().iter().enumerate() {
            shared_slice[k] = *v;
        }

        // send the request to the pageant window
        let window = Window::new();
        let pageant_window =
            PageantWindow::new(&window.window_name(), &window.class_name()).unwrap();
        pageant_window.send_message(&map_name).unwrap();

        // verify the data
        // the first 4 bytes are the length
        // the last 8 bytes are 8u8
        for i in 0..12 {
            if i < 4 {
                assert_eq!(length_bytes[i], shared_slice[i]);
            } else {
                assert_eq!(8u8, shared_slice[i]);
            }
        }
    }
}
