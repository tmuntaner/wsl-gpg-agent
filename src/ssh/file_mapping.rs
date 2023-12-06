use crate::ssh::AGENT_MAX_LENGTH;
use anyhow::{bail, Result};
use std::ffi::c_void;
use std::slice;
use widestring::U16CString;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_EXECUTE_READWRITE,
};

pub struct FileMapping {
    handle: HANDLE,
    shared_memory: *mut c_void,
}

impl FileMapping {
    pub fn new(map_name: &str) -> Result<Self> {
        let map_name_u16 = U16CString::from_str(map_name)?;
        let map_name_u16 = PCWSTR(map_name_u16.as_ptr() as *mut u16);

        let file_mapping: HANDLE;
        unsafe {
            file_mapping = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_EXECUTE_READWRITE,
                0,
                AGENT_MAX_LENGTH,
                map_name_u16,
            )?;
        }

        let shared_memory;
        unsafe {
            shared_memory = MapViewOfFile(file_mapping, FILE_MAP_ALL_ACCESS, 0u32, 0u32, 0_usize);
        }

        if shared_memory.Value.is_null() {
            unsafe {
                _ = CloseHandle(file_mapping);
            }
            bail!("failed MapViewOfFile");
        }

        Ok(Self {
            handle: file_mapping,
            shared_memory: shared_memory.Value,
        })
    }

    #[allow(clippy::mut_from_ref)]
    pub fn shared_memory(&self) -> &mut [u8] {
        unsafe {
            slice::from_raw_parts_mut(self.shared_memory as *mut _, AGENT_MAX_LENGTH as usize)
        }
    }
}

impl Drop for FileMapping {
    fn drop(&mut self) {
        unsafe {
            _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.shared_memory,
            });
            _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::process;
    use windows::Win32::System::Threading::CreateMutexW;

    #[test]
    fn test_new() {
        // generate the file map, it should work
        let map_name = format!("WSLPageantRequest-create-file-mapping-1{}", process::id());
        let file_mapping = FileMapping::new(map_name.as_str()).unwrap();
        assert_ne!(0, file_mapping.handle.0);

        // testing for failure:
        // you cannot create a file mapping if its name is already taken by a mutex
        // To make it fail, let's first create a mutex
        let map_name = format!("WSLPageantRequest-create-file-mapping-2{}", process::id());
        let map_name_c_string = U16CString::from_str(map_name.clone()).unwrap();
        let map_name_c_string = PCWSTR(map_name_c_string.as_ptr() as *mut u16);
        let mutex: HANDLE;
        unsafe {
            mutex = CreateMutexW(None, true, map_name_c_string).unwrap();
        }
        assert_ne!(0, mutex.0);

        // now that we have a mutex, creating a file mapping should fail
        let file_mapping = FileMapping::new(map_name.as_str());
        assert!(file_mapping.is_err());

        unsafe {
            _ = CloseHandle(mutex);
        }
    }

    #[test]
    fn test_shared_memory() {
        let map_name = format!("WSLPageantRequest-test-shared-memory{}", process::id());
        let file_mapping = FileMapping::new(map_name.as_str()).unwrap();
        let shared_memory = file_mapping.shared_memory();

        assert_eq!(AGENT_MAX_LENGTH as usize, shared_memory.len());
    }
}
