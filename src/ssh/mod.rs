use crate::ssh::file_mapping::FileMapping;
use crate::ssh::pageant_window::PageantWindow;
use anyhow::Result;
use std::os::raw::c_ulong;
use std::{io, process};

mod file_mapping;
mod pageant_window;

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

pub struct SshPageant {}

impl SshPageant {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(
        &self,
        pageant_window_name: &str,
        pageant_class_name: &str,
        stdout: &mut dyn io::Write,
        stdin: &mut dyn io::BufRead,
    ) -> Result<()> {
        // build shared memory map
        let map_name = format!("WSLPageantRequest{}", process::id());
        let file_mapping = FileMapping::new(&map_name)?;
        let shared_memory_slice = file_mapping.shared_memory();

        // write our request to the shared memory
        let request = self.read_request(stdin)?;
        for (i, value) in request.iter().enumerate() {
            shared_memory_slice[i] = *value;
        }

        // send message to pageant saying we've written bytes to our shared memory
        let pageant_window = PageantWindow::new(pageant_window_name, pageant_class_name)?;
        pageant_window.send_message(&map_name)?;

        // send the result to stdout
        self.send_result(stdout, shared_memory_slice)?;

        Ok(())
    }

    fn read_request(&self, stdin: &mut dyn io::BufRead) -> Result<Vec<u8>> {
        // first we need to find out how many bytes are in the request
        // convert the first 4 bytes to a u32
        let mut length_buffer = [0u8; 4];
        stdin.read_exact(&mut length_buffer)?;
        let length = u32::from_be_bytes(length_buffer);

        // now we can create a buffer of the length found above
        let mut buffer = vec![0u8; length as usize];
        stdin.read_exact(&mut buffer)?;

        // create the request vec and fill the first 4 bytes with the length
        let mut request = vec![0u8; (length + 4) as usize];
        for (i, value) in length_buffer.iter().enumerate() {
            request[i] = *value;
        }

        // fill the rest of the request with the data from the buffer
        for (i, value) in buffer.iter().enumerate() {
            request[i + 4] = *value;
        }

        Ok(request)
    }

    fn send_result(
        &self,
        stdout: &mut dyn io::Write,
        shared_memory_slice: &mut [u8],
    ) -> Result<()> {
        // find out the length by converting the first 4 bytes to a u32
        let length_buffer: [u8; 4] = [
            shared_memory_slice[0],
            shared_memory_slice[1],
            shared_memory_slice[2],
            shared_memory_slice[3],
        ];
        let length = u32::from_be_bytes(length_buffer);

        // copy our bytes to the a vec
        let mut result = vec![0u8; (length + 4) as usize];
        for i in 0..(length + 4) {
            result[i as usize] = shared_memory_slice[i as usize];
        }

        // push our bytes to stdout
        stdout.write_all(result.as_slice())?;
        stdout.flush()?;

        Ok(())
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

    #[test]
    fn test_run() {
        let mut stdout = Vec::new();
        let (length_bytes, data) = window_input();

        let window = Window::new();
        let ssh = SshPageant::new();
        ssh.run(
            &window.window_name(),
            &window.class_name(),
            &mut stdout,
            &mut data.as_slice(),
        )
        .unwrap();

        for i in 0..(8 + 4) {
            if i < 4 {
                assert_eq!(length_bytes[i], stdout[i]);
            } else {
                assert_eq!(8u8, stdout[i]);
            }
        }
    }

    #[test]
    fn test_read_request() {
        let ssh = SshPageant::new();
        let (_length_bytes, data) = window_input();

        // we should only read as much of the result specified by length
        // as we're expected 8 bytes of data + 4 from length, we want 12 bytes
        let result = ssh.read_request(&mut data.as_slice()).unwrap();
        assert_eq!(12, result.len() as u32);

        // make sure the read data is the same as the request
        for i in 0..12 as usize {
            assert_eq!(data.get(i).unwrap(), result.get(i).unwrap());
        }
    }

    #[test]
    fn test_send_result() {
        let ssh = SshPageant::new();
        let mut stdout = Vec::new();
        let length = 6_u32.to_be_bytes();
        let mut data = [
            length[0], length[1], length[2], length[3], 0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8,
        ];

        ssh.send_result(&mut stdout, &mut data).unwrap();
        assert_eq!(10, stdout.len()); // length of 6 + 4 for u32.to_be_bytes
        for n in 0..10 {
            assert_eq!(&data[n], stdout.get(n).unwrap());
        }
    }

    pub fn window_input() -> ([u8; 4], [u8; 13]) {
        let length: u32 = 8;
        let length_bytes = length.to_be_bytes();
        let data = [
            length_bytes[0],
            length_bytes[1],
            length_bytes[2],
            length_bytes[3],
            0u8,
            1u8,
            2u8,
            3u8,
            4u8,
            5u8,
            6u8,
            7u8,
            8u8,
        ];

        // ensure our data set is larger than 12
        // 8 bytes + 4 length bytes from 8u32 -> u8
        assert!(12 < data.len() as u32);

        (length_bytes, data)
    }

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
                let shared_memory_slice = file_mapping.shared_memory();

                let length_buffer: [u8; 4] = [
                    shared_memory_slice[0],
                    shared_memory_slice[1],
                    shared_memory_slice[2],
                    shared_memory_slice[3],
                ];
                let length = u32::from_be_bytes(length_buffer);

                // the first 4 bytes are the length
                // the rest are 0 - 8, starting at position 4
                for i in 4..(length) {
                    assert_eq!((i as u8) - 4, shared_memory_slice[i as usize]);
                }

                // zero out the data
                for i in 0..AGENT_MAX_LENGTH as usize {
                    shared_memory_slice[i] = 0u8;
                }

                // return 12 bytes
                // first 4 bytes are 8 as a u32
                // the rest of the bytes are 8u8
                let length_bytes = (8 as u32).to_be_bytes();
                for i in 0..12 {
                    if i < 4 {
                        shared_memory_slice[i] = length_bytes[i];
                    } else {
                        shared_memory_slice[i] = 8u8;
                    }
                }
                LRESULT(1)
            }
            _ => unsafe { DefWindowProcW(param0, param1, param2, param3) },
        }
    }
}
