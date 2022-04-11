use crate::ssh::ssh_agent::SshAgent;
use anyhow::Result;
use std::io;
use std::os::raw::c_ulong;

mod file_mapping;
mod pageant_window;
mod shared_memory;
pub mod ssh_agent;

// https://net-ssh.github.io/ssh/v2/api/classes/Net/SSH/Authentication/Pageant.html
const AGENT_COPY_DATA_ID: isize = 0x804e50ba;
const AGENT_MAX_LENGTH: c_ulong = 8192;

// https://docs.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-copydatastruct
#[repr(C)]
#[derive(Debug)]
struct CopyDataStruct {
    dw_data: isize,   // type of data
    cb_data: c_ulong, // length of data
    lp_data: isize,   // the data
}

pub struct SshPageantClient {}

impl SshPageantClient {
    pub fn run() -> Result<()> {
        let pageant_window_name = String::from("Pageant");
        let pageant_class_name = String::from("Pageant");

        loop {
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            let mut stdout = io::stdout();
            let pageant = SshAgent::new();
            pageant.run(
                &pageant_window_name,
                &pageant_class_name,
                &mut stdout,
                &mut reader,
            )?
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ssh::file_mapping::FileMapping;
    use crate::ssh::AGENT_MAX_LENGTH;
    use rand::Rng;
    use std::ffi::CStr;
    use std::os::raw::c_char;
    use std::{process, ptr};
    use widestring::U16CString;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::WM_COPYDATA;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, UnregisterClassW,
        CW_USEDEFAULT, HCURSOR, HICON, HMENU, WINDOW_EX_STYLE, WNDCLASSW, WNDCLASS_STYLES,
        WS_OVERLAPPEDWINDOW,
    };

    pub struct Window {
        window_name: U16CString,
        class_name: U16CString,
        h_instance: HINSTANCE,
        hwnd: HWND,
    }

    impl Window {
        pub fn new() -> Self {
            let mut rng = rand::thread_rng();
            let rand: u16 = rng.gen();

            let window_name =
                U16CString::from_str(format!("WSLPageant-window-{}-{}", rand, process::id()))
                    .unwrap();
            let class_name =
                U16CString::from_str(format!("WSLPageant-class-{}-{}", rand, process::id()))
                    .unwrap();

            let h_instance;
            unsafe {
                h_instance = GetModuleHandleW(PCWSTR::default());
            }
            assert_ne!(0, h_instance.0);

            let lpwndclass = WNDCLASSW {
                style: WNDCLASS_STYLES::default(),
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_instance,
                hIcon: HICON::default(),
                hCursor: HCURSOR::default(),
                hbrBackground: Default::default(),
                lpszMenuName: PCWSTR::default(),
                lpszClassName: PCWSTR(class_name.as_ptr() as *mut u16),
            };

            unsafe {
                let result = RegisterClassW(&lpwndclass);
                assert_ne!(0, result);
            }

            let hwnd;
            unsafe {
                hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    PCWSTR(class_name.as_ptr() as *mut u16),
                    PCWSTR(window_name.as_ptr() as *mut u16),
                    WS_OVERLAPPEDWINDOW,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    HWND::default(),
                    HMENU::default(),
                    h_instance,
                    ptr::null(),
                );
            }
            assert_ne!(0, hwnd.0);

            Self {
                window_name,
                class_name,
                h_instance,
                hwnd,
            }
        }

        pub fn window_name(&self) -> String {
            self.window_name.to_string().unwrap()
        }

        pub fn class_name(&self) -> String {
            self.class_name.to_string().unwrap()
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                UnregisterClassW(
                    PCWSTR(self.class_name.as_ptr() as *mut u16),
                    self.h_instance,
                );
                DestroyWindow(self.hwnd);
            }
        }
    }

    extern "system" fn wnd_proc(
        param0: HWND,
        param1: u32,
        param2: WPARAM,
        param3: LPARAM,
    ) -> LRESULT {
        match param1 {
            WM_COPYDATA => {
                let data: &CopyDataStruct;
                let map_name;
                unsafe {
                    data = &*(param3.0 as *const CopyDataStruct);
                    map_name = CStr::from_ptr(data.lp_data as *mut c_char)
                        .to_str()
                        .unwrap();
                }
                let file_mapping = FileMapping::new(&map_name).unwrap();
                let mut shared_memory = file_mapping.shared_memory().unwrap();
                let shared_memory_slice = shared_memory.shared_memory();

                for i in 0..8 {
                    assert_eq!(i as u8, shared_memory_slice[i as usize]);
                }
                // reset
                for i in 0..AGENT_MAX_LENGTH as usize {
                    shared_memory_slice[i] = 0u8;
                }
                for i in 0..9 {
                    shared_memory_slice[i as usize] = 8u8;
                }
                LRESULT(1)
            }
            _ => unsafe { DefWindowProcW(param0, param1, param2, param3) },
        }
    }
}
