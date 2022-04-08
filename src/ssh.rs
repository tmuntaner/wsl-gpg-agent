use anyhow::{bail, Result};
use clap::Parser;
use std::ffi::{c_void, CString};
use std::io::{Read, Stdin};
use std::os::raw::c_ulong;
use std::process::Command;
use std::{io, process, slice};
use widestring::U16CString;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, WPARAM};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_EXECUTE_READWRITE,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_COPYDATA};

#[derive(Parser)]
pub struct Ssh {}

// https://net-ssh.github.io/ssh/v2/api/classes/Net/SSH/Authentication/Pageant.html
const AGENT_COPY_DATA_ID: isize = 0x804e50ba;
const AGENT_MAX_LENGTH: c_ulong = 8192;

impl Ssh {
    pub fn run(&self) -> Result<()> {
        log::info!("start");

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            let request = self.read_request(&stdin)?;
            let hwnd = self.find_pageant()?;

            let map_name = format!("WSLPageantRequest{}", process::id());

            let file_mapping = self.create_file_mapping(&map_name)?;
            let (shared_memory, shared_memory_slice) =
                self.map_file_of_view(file_mapping, request)?;
            self.send_message(&hwnd, &map_name)?;

            self.send_result(&mut stdout, shared_memory_slice)?;

            unsafe {
                UnmapViewOfFile(shared_memory);
                CloseHandle(file_mapping);
            }

            log::info!("all done");
        }
    }

    fn read_request(&self, stdin: &Stdin) -> Result<Vec<u8>> {
        let mut stdin_handle = stdin.lock();

        let mut length_buffer = [0u8; 4];
        stdin_handle.read_exact(&mut length_buffer)?;
        let length = u32::from_be_bytes(length_buffer);

        let mut buffer = vec![0u8; length as usize];
        stdin_handle.read_exact(&mut buffer)?;

        let mut request = vec![0u8; (length + 4) as usize];
        for (i, value) in length_buffer.iter().enumerate() {
            request[i] = *value;
        }
        for (i, value) in buffer.iter().enumerate() {
            request[i + 4] = *value;
        }

        Ok(request)
    }

    fn find_pageant(&self) -> Result<HWND> {
        let pageant_window_name = U16CString::from_str("Pageant")?;
        let pageant_window_name = PCWSTR(pageant_window_name.as_ptr() as *mut u16);

        let mut hwnd;
        unsafe {
            hwnd = FindWindowW(pageant_window_name, pageant_window_name);
        }

        if hwnd.0 == 0 {
            log::info!("pageant window not found. launching");
            let connect_command = Command::new("gpg-connect-agent")
                .args(["/bye"])
                .output()
                .expect("foo");
            log::info!("pageant launch status: {}", connect_command.status);
        }

        unsafe {
            hwnd = FindWindowW(pageant_window_name, pageant_window_name);
        }

        if hwnd.0 == 0 {
            log::info!("hwnd not found");
            bail!("could not find pageant window");
        }

        log::info!("found hwnd {:?}", hwnd);

        Ok(hwnd)
    }

    fn map_file_of_view(
        &self,
        file_mapping: HANDLE,
        request: Vec<u8>,
    ) -> Result<(*mut c_void, &mut [u8])> {
        let shared_memory;
        unsafe {
            shared_memory = MapViewOfFile(file_mapping, FILE_MAP_ALL_ACCESS, 0u32, 0u32, 0_usize);
        }

        let shared_memory_slice;
        unsafe {
            shared_memory_slice =
                slice::from_raw_parts_mut(shared_memory as *mut _, AGENT_MAX_LENGTH as usize);
        }
        for (i, value) in request.iter().enumerate() {
            shared_memory_slice[i] = *value;
        }

        Ok((shared_memory, shared_memory_slice))
    }

    fn send_message(&self, hwnd: &HWND, map_name: &str) -> Result<()> {
        log::info!("sending message");
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
            result = SendMessageW(hwnd, WM_COPYDATA, WPARAM(0_usize), lparam);
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

    fn send_result(
        &self,
        stdout: &mut dyn io::Write,
        shared_memory_slice: &mut [u8],
    ) -> Result<()> {
        let length_buffer: [u8; 4] = [
            shared_memory_slice[0],
            shared_memory_slice[1],
            shared_memory_slice[2],
            shared_memory_slice[3],
        ];
        let length = u32::from_be_bytes(length_buffer);

        let mut result = vec![0u8; (length + 4) as usize];
        for i in 0..(length + 4) {
            result[i as usize] = shared_memory_slice[i as usize];
        }

        stdout.write_all(result.as_slice())?;
        stdout.flush()?;

        Ok(())
    }

    fn create_file_mapping(&self, map_name: &str) -> Result<HANDLE> {
        let map_name_u16 = U16CString::from_str(map_name)?;
        let map_name_u16 = PCWSTR(map_name_u16.as_ptr() as *mut u16);

        let file_mapping;
        unsafe {
            file_mapping = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null::<SECURITY_ATTRIBUTES>(),
                PAGE_EXECUTE_READWRITE,
                0,
                AGENT_MAX_LENGTH,
                map_name_u16,
            )?;
        }

        Ok(file_mapping)
    }
}

// https://docs.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-copydatastruct
#[repr(C)]
struct CopyDataStruct {
    dw_data: isize,   // type of data
    cb_data: c_ulong, // length of data
    lp_data: isize,   // the data
}

#[cfg(test)]
mod test {
    use super::*;
    use windows::Win32::System::Threading::CreateMutexW;

    #[test]
    fn test_create_file_mapping() {
        let ssh = Ssh {};
        let map_name = format!("WSLPageantRequest-create-file-mapping{}", process::id());
        let mapping = ssh.create_file_mapping(map_name.as_str()).unwrap();
        assert_ne!(0, mapping.0);
        unsafe {
            CloseHandle(mapping);
        }

        // you cannot create a file mapping if its name is already taken by a mutex
        // let's create one
        let map_name_c_string = U16CString::from_str(map_name.clone()).unwrap();
        let map_name_c_string = PCWSTR(map_name_c_string.as_ptr() as *mut u16);
        let mutex: HANDLE;
        unsafe {
            mutex = CreateMutexW(
                std::ptr::null::<SECURITY_ATTRIBUTES>(),
                true,
                map_name_c_string,
            )
            .unwrap();
        }
        assert_ne!(0, mutex.0);

        // now the create file mapping should fail
        let mapping = ssh.create_file_mapping(map_name.as_str());
        assert!(mapping.is_err());

        unsafe {
            CloseHandle(mutex);
        }
    }

    #[test]
    fn test_send_result() {
        let ssh = Ssh {};
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

    #[test]
    fn test_map_file_of_view() {
        let ssh = Ssh {};
        let map_name = format!("WSLPageantRequest-test-map-file-of-view{}", process::id());
        let mapping = ssh.create_file_mapping(map_name.as_str()).unwrap();
        let data = vec![0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8];
        let (shared_memory, shared_slice) = ssh.map_file_of_view(mapping, data.clone()).unwrap();

        for n in 0..8 {
            assert_eq!(data.get(n).unwrap(), shared_slice.get(n).unwrap());
        }

        unsafe {
            CloseHandle(mapping);
            UnmapViewOfFile(shared_memory);
        }
    }
}
